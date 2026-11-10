//! The JSON API under `/api/v1` (docs/api.md), merged into the UI router so
//! it shares the loopback Host check and the hardening layer. Handlers call
//! the same core functions as the CLI and serialize the same named types, so
//! the two outputs agree by construction. Writes (create, edit, delete,
//! comments, Thread state) are multipart or JSON bodies with attribution in
//! body fields, and every non-GET request carrying a foreign `Origin` is
//! refused.
//!
//! Every error is RFC 9457 `application/problem+json` carrying the core
//! error's stable `code`, including bad query strings, bad path segments,
//! unknown routes and wrong methods. The OpenAPI 3.1 document is derived from
//! the Rust types (ADR 0008) and served at `/api/v1/openapi.json`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::Router;
use axum::extract::multipart::MultipartError;
use axum::extract::{DefaultBodyLimit, FromRequest, FromRequestParts, Multipart, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderName, HeaderValue, Method, StatusCode, header};
use axum::middleware::{Next, from_fn};
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, get, post};
use rusqlite::Connection;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use utoipa::{IntoParams, OpenApi, ToSchema};

use crate::artifact::{self, ListFilter};
use crate::bundle::{Input, RawFile};
use crate::cli::{self, Context, Role};
use crate::comments::{self, CommentList};
use crate::db;
use crate::diagram::Format;
use crate::error::Error;
use crate::kind::Kind;
use crate::limits;
use crate::scope::{self, Scope, ScopeFilter, Selection};
use crate::sections::{self, Outline};

const NOSNIFF: HeaderName = HeaderName::from_static("x-content-type-options");
const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;
/// Default and ceiling, in seconds, of `GET .../comments?wait=`.
const DEFAULT_WAIT_SECS: u64 = 60;
const MAX_WAIT_SECS: u64 = 300;
/// Body limit of the multipart routes: the per-Revision limit plus room for
/// part headers and `meta`, so an over-limit upload reaches the core check
/// (422 `rejected_input`) instead of the extractor's 413.
const MAX_UPLOAD_BYTES: usize = limits::MAX_REVISION_BYTES as usize + 2 * 1024 * 1024;

// --- errors -------------------------------------------------------------------

/// The body of every error response.
#[derive(Serialize, ToSchema)]
pub struct Problem {
    /// Always `about:blank`: the status and `code` carry the meaning.
    #[serde(rename = "type")]
    #[schema(rename = "type")]
    pub kind: String,
    /// The HTTP status phrase.
    pub title: String,
    pub status: u16,
    /// The message the CLI prints for the same failure.
    pub detail: String,
    /// Stable identifier shared with the CLI: `usage`, `not_found`,
    /// `rejected_input`, `conflict` or `unexpected`.
    pub code: String,
}

pub(crate) struct ApiError(Error);

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        ApiError(e)
    }
}

fn problem(status: StatusCode, code: &str, detail: String) -> Response {
    let body = Problem {
        kind: "about:blank".into(),
        title: status.canonical_reason().unwrap_or("Error").into(),
        status: status.as_u16(),
        detail,
        code: code.into(),
    };
    (
        status,
        [(header::CONTENT_TYPE, "application/problem+json")],
        serde_json::to_vec(&body).expect("problem serializes"),
    )
        .into_response()
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status =
            StatusCode::from_u16(self.0.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        problem(status, self.0.code(), self.0.to_string())
    }
}

/// Schema stand-in for a raw file body.
#[derive(ToSchema)]
#[schema(value_type = String, format = Binary)]
#[allow(dead_code)]
struct RawBytes(Vec<u8>);

/// Error responses documented on every route.
#[derive(utoipa::IntoResponses)]
#[allow(dead_code)]
enum ProblemResponses {
    /// Malformed query string, path segment or id.
    #[response(status = 400, content_type = "application/problem+json")]
    Usage(Problem),
    /// No such Artifact, Revision or file.
    #[response(status = 404, content_type = "application/problem+json")]
    NotFound(Problem),
    /// Well-formed but refused: a limit out of range, an ambiguous or missing
    /// Section, a Kind without Sections.
    #[response(status = 422, content_type = "application/problem+json")]
    Rejected(Problem),
    /// An `edit` against a stale Revision (`base_revision` is not the latest);
    /// the detail names the latest Revision and its author. No read returns it.
    #[response(status = 409, content_type = "application/problem+json")]
    Conflict(Problem),
    /// Anything else, including a database busy past its timeout.
    #[response(status = 500, content_type = "application/problem+json")]
    Unexpected(Problem),
}

/// Added to every write route.
#[derive(utoipa::IntoResponses)]
#[allow(dead_code)]
enum WriteProblemResponses {
    /// The request carries an `Origin` header naming another host (a browser
    /// page on another site). Code `forbidden`.
    #[response(status = 403, content_type = "application/problem+json")]
    Forbidden(Problem),
}

/// `axum::extract::Query` whose rejection is a problem+json 400.
struct Q<T>(T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequestParts<S> for Q<T> {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        axum::extract::Query::<T>::from_request_parts(parts, state)
            .await
            .map(|q| Q(q.0))
            .map_err(|e| Error::Usage(e.body_text()).into())
    }
}

/// `axum::extract::Path` whose rejection is a problem+json 400.
struct P<T>(T);

impl<S: Send + Sync, T: DeserializeOwned + Send> FromRequestParts<S> for P<T> {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        axum::extract::Path::<T>::from_request_parts(parts, state)
            .await
            .map(|p| P(p.0))
            .map_err(|e| Error::Usage(e.body_text()).into())
    }
}

async fn unknown_route() -> ApiError {
    Error::NotFound("no such API route".into()).into()
}

/// Turn axum's bodyless 405 into a problem+json one, keeping the `Allow`
/// header axum computed from the route's methods (RFC 9110 requires it). Not a
/// per-route fallback, which would also bypass axum's HEAD-from-GET routing.
async fn problem_405(req: Request, next: Next) -> Response {
    let res = next.run(req).await;
    if res.status() != StatusCode::METHOD_NOT_ALLOWED {
        return res;
    }
    let mut out = problem(
        StatusCode::METHOD_NOT_ALLOWED,
        Error::Usage(String::new()).code(),
        "this route does not accept that method; see /api/v1/openapi.json".into(),
    );
    if let Some(allow) = res.headers().get(header::ALLOW) {
        out.headers_mut().insert(header::ALLOW, allow.clone());
    }
    out
}

// --- plumbing -----------------------------------------------------------------

#[derive(Clone)]
struct ApiState {
    db_path: PathBuf,
}

/// Run `f` against a fresh connection off the async threads (SQLite
/// connections are not `Sync`).
async fn run<T, F>(s: &ApiState, f: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&mut Connection) -> Result<T, Error> + Send + 'static,
{
    let db_path = s.db_path.clone();
    tokio::task::spawn_blocking(move || f(&mut db::open(&db_path)?))
        .await
        .map_err(|e| Error::Unexpected(format!("request task failed: {e}")))?
        .map_err(ApiError)
}

fn json<T: Serialize>(v: &T) -> Response {
    json_status(StatusCode::OK, v)
}

fn json_status<T: Serialize>(status: StatusCode, v: &T) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_vec(v).expect("response serializes"),
    )
        .into_response()
}

/// The API takes bare ids: a revision has its own path segment.
fn bare_id(id: &str) -> Result<&str, Error> {
    if id.is_empty() || id.contains('@') {
        return Err(Error::Usage(format!(
            "artifact id must be bare (no @revision), got {id:?}"
        )));
    }
    Ok(id)
}

fn revision_number(n: &str) -> Result<i64, Error> {
    n.parse::<i64>()
        .ok()
        .filter(|n| *n >= 1)
        .ok_or_else(|| Error::Usage(format!("bad revision {n:?}: expected a positive integer")))
}

fn absolute(name: &str, p: &str) -> Result<(), Error> {
    if Path::new(p).is_absolute() {
        Ok(())
    } else {
        Err(Error::Usage(format!("{name} must be an absolute path")))
    }
}

// --- routes -------------------------------------------------------------------

/// Query of `GET /api/v1/artifacts`.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct ListParams {
    /// `markdown`, `html`, `image` or `diagram`.
    kind: Option<String>,
    /// Latest Revision made in this Session.
    session: Option<String>,
    /// Latest Revision made by this author.
    author: Option<String>,
    /// Only Artifacts with at least one open Thread.
    open: Option<bool>,
    /// Only Artifacts last updated longer ago than this: `30d`, `12h`, `90m`, `45s` or `2w`.
    older_than: Option<String>,
    /// 1 to 200; default 50.
    limit: Option<i64>,
    /// Project name.
    project: Option<String>,
    /// Absolute path of the Project's main checkout.
    project_root: Option<String>,
    /// Absolute path of a linked Worktree: shows the Project root's Artifacts
    /// plus that Worktree's, as the CLI does from inside one. Needs `project`
    /// or `project_root` to say which Project it belongs to, else 400.
    worktree: Option<String>,
}

impl ListParams {
    fn filter(self) -> Result<ListFilter, Error> {
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(Error::Rejected(format!(
                "limit must be between 1 and {MAX_LIMIT}, got {limit}"
            )));
        }
        let kind = self
            .kind
            .as_deref()
            .map(|k| {
                Kind::parse(k).map_err(|_| {
                    Error::Usage(format!(
                        "unknown kind {k:?}: use markdown, html, image or diagram"
                    ))
                })
            })
            .transpose()?;
        let older_than_secs = self
            .older_than
            .as_deref()
            .map(artifact::parse_duration)
            .transpose()?;
        // No cwd to detect a Project from: unscoped unless a filter is named.
        let sel = match self.project {
            Some(name) if !name.is_empty() => Selection::Name(name),
            _ => Selection::None,
        };
        if let Some(r) = &self.project_root {
            absolute("project_root", r)?;
        }
        // A Worktree belongs to a Project: without a name or root to say which,
        // `worktree` alone would match every Project's Artifacts.
        if self.worktree.is_some()
            && self.project_root.is_none()
            && !matches!(sel, Selection::Name(_))
        {
            return Err(Error::Usage(
                "worktree needs project or project_root".into(),
            ));
        }
        let mut scope = ScopeFilter::build(
            &sel,
            false,
            false,
            self.project_root.as_deref(),
            Path::new("/"),
        );
        scope = match self.worktree.as_deref() {
            Some(w) => {
                absolute("worktree", w)?;
                scope.in_worktree(w)
            }
            None if self.project_root.is_some() => scope,
            None => ScopeFilter::build(&sel, false, true, None, Path::new("/")),
        };
        Ok(ListFilter {
            id: None,
            kind,
            session: self.session,
            author: self.author,
            open: self.open.unwrap_or(false),
            older_than_secs,
            limit,
            scope,
        })
    }
}

/// List Artifacts, newest activity first.
#[utoipa::path(
    get, path = "/api/v1/artifacts", params(ListParams),
    responses((status = 200, description = "Artifacts", body = artifact::ArtifactList), ProblemResponses)
)]
async fn list_artifacts(
    State(s): State<ApiState>,
    Q(p): Q<ListParams>,
) -> Result<Response, ApiError> {
    let filter = p.filter()?;
    let rows = run(&s, move |c| artifact::list(c, &filter)).await?;
    Ok(json(&artifact::ArtifactList { items: rows }))
}

/// One Artifact.
#[utoipa::path(
    get, path = "/api/v1/artifacts/{id}", params(("id" = String, Path, description = "Bare Artifact id")),
    responses((status = 200, description = "The Artifact", body = artifact::ArtifactDetail), ProblemResponses)
)]
async fn get_artifact(State(s): State<ApiState>, P(id): P<String>) -> Result<Response, ApiError> {
    let id = bare_id(&id)?.to_string();
    Ok(json(&run(&s, move |c| artifact::detail(c, &id)).await?))
}

/// Every Revision of an Artifact, oldest first.
#[utoipa::path(
    get, path = "/api/v1/artifacts/{id}/revisions", params(("id" = String, Path, description = "Bare Artifact id")),
    responses((status = 200, description = "Revisions", body = artifact::RevisionList), ProblemResponses)
)]
async fn list_revisions(State(s): State<ApiState>, P(id): P<String>) -> Result<Response, ApiError> {
    let id = bare_id(&id)?.to_string();
    Ok(json(&run(&s, move |c| artifact::revisions(c, &id)).await?))
}

/// One Revision as a file manifest.
#[utoipa::path(
    get, path = "/api/v1/artifacts/{id}/revisions/{n}",
    params(("id" = String, Path, description = "Bare Artifact id"), ("n" = i64, Path, description = "Revision number, 1-based")),
    responses((status = 200, description = "File manifest", body = artifact::RevisionManifest), ProblemResponses)
)]
async fn get_revision(
    State(s): State<ApiState>,
    P((id, n)): P<(String, String)>,
) -> Result<Response, ApiError> {
    let id = bare_id(&id)?.to_string();
    let n = revision_number(&n)?;
    Ok(json(
        &run(&s, move |c| artifact::manifest(c, &id, n)).await?,
    ))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct FileParams {
    /// Markdown only: return just this Section, as the CLI's `get --section`
    /// resolves it (`A > B`, optional `#n`).
    section: Option<String>,
}

/// A `Content-Disposition` filename that cannot break out of the quoted
/// string or carry control characters.
fn attachment(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    let safe: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("attachment; filename=\"{safe}\"")
}

/// Raw bytes of one stored file, with its stored media type. Always an
/// attachment with `nosniff`: rendering untrusted bytes is the content
/// origin's job, never this route's.
#[utoipa::path(
    get, path = "/api/v1/artifacts/{id}/revisions/{n}/files/{path}",
    params(
        ("id" = String, Path, description = "Bare Artifact id"),
        ("n" = i64, Path, description = "Revision number, 1-based"),
        ("path" = String, Path, description = "Stored file path; may contain slashes"),
        FileParams,
    ),
    responses(
        (status = 200, description = "The file's bytes. `Content-Type` is the stored media type (`text/markdown` for a Section); `Content-Disposition: attachment` and `X-Content-Type-Options: nosniff` are always set.",
         content_type = "application/octet-stream", body = RawBytes),
        ProblemResponses
    )
)]
async fn get_file(
    State(s): State<ApiState>,
    P((id, n, path)): P<(String, String, String)>,
    Q(q): Q<FileParams>,
) -> Result<Response, ApiError> {
    let id = bare_id(&id)?.to_string();
    let n = revision_number(&n)?;
    let (file, section) = run(&s, move |c| {
        // Exact match on the stored path: a request path never reaches the
        // filesystem.
        let file = artifact::get_file(c, &id, n, &path)?
            .ok_or_else(|| Error::NotFound(format!("no file {path:?} in {id}@{n}")))?;
        let Some(query) = q.section else {
            return Ok((file, None));
        };
        if artifact::kind_of(c, &id)? != Some(Kind::Markdown) {
            return Err(Error::Usage("section is for markdown Artifacts".into()));
        }
        let text = artifact::markdown_text(c, &id, Some(n))?;
        let part = sections::extract(&text, &query)?.to_string();
        Ok((file, Some(part)))
    })
    .await?;
    let (media_type, bytes) = match section {
        Some(part) => ("text/markdown".to_string(), part.into_bytes()),
        None => (file.media_type.clone(), file.bytes),
    };
    let mut res = bytes.into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&media_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&attachment(&file.path))
            .unwrap_or(HeaderValue::from_static("attachment")),
    );
    h.insert(NOSNIFF, HeaderValue::from_static("nosniff"));
    Ok(res)
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct OutlineParams {
    /// Revision number; latest by default.
    revision: Option<i64>,
}

/// The headings of a markdown Artifact.
#[utoipa::path(
    get, path = "/api/v1/artifacts/{id}/outline",
    params(("id" = String, Path, description = "Bare Artifact id"), OutlineParams),
    responses((status = 200, description = "Outline", body = Outline), ProblemResponses)
)]
async fn get_outline(
    State(s): State<ApiState>,
    P(id): P<String>,
    Q(q): Q<OutlineParams>,
) -> Result<Response, ApiError> {
    let id = bare_id(&id)?.to_string();
    if let Some(n) = q.revision {
        revision_number(&n.to_string())?;
    }
    let outline = run(&s, move |c| {
        Ok(sections::outline(&artifact::markdown_text(
            c, &id, q.revision,
        )?))
    })
    .await?;
    Ok(json(&outline))
}

/// The `cairn:` image embeds of a markdown Artifact, resolved now.
#[utoipa::path(
    get, path = "/api/v1/artifacts/{id}/embeds",
    params(("id" = String, Path, description = "Bare Artifact id"), OutlineParams),
    responses((status = 200, description = "Embeds", body = crate::embeds::EmbedList), ProblemResponses)
)]
async fn get_embeds(
    State(s): State<ApiState>,
    P(id): P<String>,
    Q(q): Q<OutlineParams>,
) -> Result<Response, ApiError> {
    let id = bare_id(&id)?.to_string();
    if let Some(n) = q.revision {
        revision_number(&n.to_string())?;
    }
    let list = run(&s, move |c| {
        let text = artifact::markdown_text(c, &id, q.revision)?;
        Ok(crate::embeds::EmbedList {
            embeds: crate::embeds::resolve(c, &text)?,
        })
    })
    .await?;
    Ok(json(&list))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct CommentParams {
    /// Include resolved Threads.
    all: Option<bool>,
    /// Only Threads holding a Comment with a larger id.
    since: Option<i64>,
    /// Long poll: seconds to wait for a Comment newer than `since` (or, with
    /// no `since`, newer than the highest id when the request arrived).
    /// `true` or an empty value waits 60; a number is clamped to 300; absent,
    /// `false` or `0` answers at once.
    wait: Option<String>,
}

impl CommentParams {
    fn wait_secs(&self) -> Result<Option<u64>, Error> {
        match self.wait.as_deref() {
            None | Some("false") | Some("0") => Ok(None),
            Some("") | Some("true") => Ok(Some(DEFAULT_WAIT_SECS)),
            Some(n) => n
                .parse::<u64>()
                .map(|n| Some(n.min(MAX_WAIT_SECS)))
                .map_err(|_| {
                    Error::Usage(format!("bad wait {n:?}: expected seconds, true or false"))
                }),
        }
    }
}

/// Threads of an Artifact, with their replies. With `wait`, holds the request
/// until a Comment newer than the cursor appears or the wait ends; a timeout
/// is a 200 with no Threads and the same `highest_comment_id`.
#[utoipa::path(
    get, path = "/api/v1/artifacts/{id}/comments",
    params(("id" = String, Path, description = "Bare Artifact id"), CommentParams),
    responses((status = 200, description = "Threads", body = CommentList), ProblemResponses)
)]
async fn list_comments(
    State(s): State<ApiState>,
    P(id): P<String>,
    Q(q): Q<CommentParams>,
) -> Result<Response, ApiError> {
    let id = bare_id(&id)?.to_string();
    let wait = q.wait_secs()?;
    let (all, since) = (q.all.unwrap_or(false), q.since);
    // The API has no caller identity, so no Comment is "its own": a Comment by
    // any author, including one named `api`, ends a wait.
    let ctx = Context {
        role: Role::Reviewer,
        author: "api".into(),
        session: None,
        session_explicit: None,
        db_path: s.db_path.clone(),
        project: None,
    };
    let query = move || comments::ListQuery {
        artifact: Some(id.clone()),
        all,
        since,
        scope: ScopeFilter::unscoped(),
    };
    let Some(secs) = wait else {
        let list = run(&s, move |c| comments::list(c, &ctx, &query())).await?;
        return Ok(json(&list));
    };
    // The wait is an async loop of short blocking checks rather than one
    // blocking sleep loop: dropping this future (the client disconnected)
    // stops it at the next await, and no blocking thread or connection is
    // held between polls.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    let ctx = std::sync::Arc::new(ctx);
    let (c1, q1) = (ctx.clone(), query.clone());
    let baseline = run(&s, move |c| comments::wait_baseline(c, &q1())).await?;
    loop {
        let (c2, q2) = (c1.clone(), query.clone());
        let list = run(&s, move |c| {
            comments::poll_newer(c, &c2, &q2(), baseline, false)
        })
        .await?;
        let now = tokio::time::Instant::now();
        if !list.threads.is_empty() || now >= deadline {
            return Ok(json(&list));
        }
        let poll = Duration::from_millis(comments::WAIT_POLL_MS);
        tokio::time::sleep((deadline - now).min(poll)).await;
    }
}

// --- writes -------------------------------------------------------------------

/// The `meta` part of a create or edit. Unknown fields are a 400.
#[derive(Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct WriteMeta {
    /// Create only, required: `markdown`, `html`, `image` or `diagram`.
    kind: Option<String>,
    /// Create only, required for a diagram and refused otherwise: `mermaid` or `svg`.
    format: Option<String>,
    /// Create only: display name, and the stored path of a single-file
    /// Artifact. Defaults to the file part's name when there is one file part.
    name: Option<String>,
    /// `agent` (default) or `reviewer`.
    role: Option<String>,
    /// Defaults to the role name.
    author: Option<String>,
    session: Option<String>,
    /// Edit only: refuse with 409 unless this is still the latest Revision.
    base_revision: Option<i64>,
    /// Edit only: root Comment ids of Threads this Revision addresses.
    #[serde(default)]
    addresses: Vec<i64>,
    /// Create only: Project name; needs `project_root`.
    project_name: Option<String>,
    /// Create only: absolute path of the Project's main checkout.
    project_root: Option<String>,
    /// Create only: absolute path of a linked Worktree.
    worktree: Option<String>,
}

fn non_empty(v: &Option<String>) -> Option<String> {
    v.as_ref().filter(|s| !s.is_empty()).cloned()
}

/// Attribution from body fields, defaulted as in the CLI: role `agent`,
/// author the role name. No headers, no env, so no session fallback.
fn write_ctx(
    db_path: &Path,
    role: &Option<String>,
    author: &Option<String>,
    session: &Option<String>,
) -> Result<Context, Error> {
    let role = match role.as_deref().filter(|r| !r.is_empty()) {
        None | Some("agent") => Role::Agent,
        Some("reviewer") => Role::Reviewer,
        Some(other) => {
            return Err(Error::Usage(format!(
                "unknown role {other:?}: use agent or reviewer"
            )));
        }
    };
    let session = non_empty(session);
    Ok(Context {
        role,
        author: non_empty(author).unwrap_or_else(|| role.as_str().to_string()),
        session_explicit: session.clone(),
        session,
        db_path: db_path.to_path_buf(),
        project: None,
    })
}

impl WriteMeta {
    fn ctx(&self, db_path: &Path) -> Result<Context, Error> {
        write_ctx(db_path, &self.role, &self.author, &self.session)
    }

    fn refuse(&self, what: &str, fields: &[(&str, bool)]) -> Result<(), Error> {
        match fields.iter().find(|(_, set)| *set) {
            Some((f, _)) => Err(Error::Usage(format!("meta.{f} does not apply to {what}"))),
            None => Ok(()),
        }
    }
}

/// A parsed multipart write: the optional `meta` part and one file per other
/// part, the part name being the stored path (so a file cannot be named
/// `meta`). Per-file, per-Revision and file-count limits are enforced while
/// reading, so an over-limit upload fails early with `rejected_input`.
struct Upload {
    meta: Option<WriteMeta>,
    files: Vec<RawFile>,
}

fn multipart_error(e: MultipartError) -> Error {
    if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
        Error::Rejected(format!(
            "request body is over the per-Revision limit of {} bytes",
            limits::MAX_REVISION_BYTES
        ))
    } else {
        Error::Usage(e.body_text())
    }
}

impl<S: Send + Sync> FromRequest<S> for Upload {
    type Rejection = ApiError;
    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        let mut mp = Multipart::from_request(req, state)
            .await
            .map_err(|e| Error::Usage(e.body_text()))?;
        let mut meta: Option<WriteMeta> = None;
        let mut files: Vec<RawFile> = Vec::new();
        let mut total: u64 = 0;
        while let Some(field) = mp.next_field().await.map_err(multipart_error)? {
            let name = field
                .name()
                .ok_or_else(|| Error::Usage("every multipart part needs a name".into()))?
                .to_string();
            let bytes = field.bytes().await.map_err(multipart_error)?;
            if name == "meta" {
                if meta.is_some() {
                    return Err(Error::Usage("more than one meta part".into()).into());
                }
                meta = Some(
                    serde_json::from_slice(&bytes)
                        .map_err(|e| Error::Usage(format!("bad meta part: {e}")))?,
                );
                continue;
            }
            limits::check_file(bytes.len() as u64)?;
            total += bytes.len() as u64;
            limits::check_revision(total)?;
            limits::check_bundle_files(files.len() + 1)?;
            files.push(RawFile {
                path: name,
                bytes: bytes.to_vec(),
            });
        }
        Ok(Upload { meta, files })
    }
}

/// `axum::Json` whose rejection is a problem+json 400.
struct J<T>(T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for J<T> {
    type Rejection = ApiError;
    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        axum::Json::<T>::from_request(req, state)
            .await
            .map(|j| J(j.0))
            .map_err(|e| Error::Usage(e.body_text()).into())
    }
}

/// Schema of the multipart body of create and edit.
#[derive(ToSchema)]
#[allow(dead_code)]
struct UploadForm {
    /// Optional on edit, required on create.
    meta: WriteMeta,
    /// Every other part is a file; its part name is its stored path.
    /// A single-file Artifact takes one; an html Bundle takes one or more.
    files: Vec<RawBytes>,
}

/// Body of a successful create or edit.
#[derive(Serialize, ToSchema)]
#[allow(dead_code)]
struct RefBody {
    id: String,
    revision: i64,
    /// `id@revision`.
    #[serde(rename = "ref")]
    #[schema(rename = "ref")]
    reference: String,
}

/// Body of a successful delete.
#[derive(Serialize, ToSchema)]
struct Deleted {
    deleted: Vec<String>,
}

fn kind_from_meta(k: &str) -> Result<Kind, Error> {
    Kind::parse(k).map_err(|_| {
        Error::Usage(format!(
            "unknown kind {k:?}: use markdown, html, image or diagram"
        ))
    })
}

fn scope_from_meta(m: &WriteMeta) -> Result<Option<Scope>, Error> {
    let Some(root) = non_empty(&m.project_root) else {
        if let Some(name) = non_empty(&m.project_name) {
            return scope::for_write(Selection::Name(name)).map_err(Error::Usage);
        }
        if m.worktree.is_some() {
            return Err(Error::Usage("meta.worktree needs project_root".into()));
        }
        return Ok(None);
    };
    absolute("project_root", &root)?;
    let worktree = non_empty(&m.worktree);
    if let Some(w) = &worktree {
        absolute("worktree", w)?;
    }
    let name = non_empty(&m.project_name)
        .or_else(|| {
            Path::new(&root)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .ok_or_else(|| Error::Usage("meta.project_name is needed for this project_root".into()))?;
    Ok(Some(Scope {
        name,
        root,
        worktree,
    }))
}

/// Create an Artifact (Revision 1).
#[utoipa::path(
    post, path = "/api/v1/artifacts",
    request_body(content = UploadForm, content_type = "multipart/form-data"),
    responses((status = 201, description = "Created", body = RefBody), ProblemResponses, WriteProblemResponses)
)]
async fn create_artifact(State(s): State<ApiState>, up: Upload) -> Result<Response, ApiError> {
    let meta = up
        .meta
        .ok_or_else(|| Error::Usage("a create needs a meta part".into()))?;
    meta.refuse(
        "create",
        &[
            ("base_revision", meta.base_revision.is_some()),
            ("addresses", !meta.addresses.is_empty()),
        ],
    )?;
    let kind = kind_from_meta(
        meta.kind
            .as_deref()
            .ok_or_else(|| Error::Usage("meta.kind is required".into()))?,
    )?;
    let format = non_empty(&meta.format)
        .map(|f| {
            Format::parse(&f)
                .map_err(|_| Error::Usage(format!("unknown format {f:?}: use mermaid or svg")))
        })
        .transpose()?;
    let mut files = up.files;
    if files.is_empty() {
        return Err(Error::Usage("no file parts".into()).into());
    }
    let dir = files.len() > 1;
    let name = match (non_empty(&meta.name), dir) {
        (Some(n), _) => n,
        (None, false) => files[0].path.clone(),
        (None, true) => {
            return Err(Error::Usage("meta.name is required for several files".into()).into());
        }
    };
    if !dir {
        files[0].path = name.clone();
    }
    let scope = scope_from_meta(&meta)?;
    let ctx = meta.ctx(&s.db_path)?;
    let r = run(&s, move |c| {
        artifact::submit(
            c,
            &ctx,
            artifact::SubmitInput {
                name,
                kind,
                format,
                input: Input { files, dir },
                scope,
            },
        )
    })
    .await?;
    Ok(json_status(StatusCode::CREATED, &r))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct EditParams {
    /// Replace only this markdown Section (`A > B`, optional `#n`) with the
    /// one markdown file part, which must start with its heading.
    section: Option<String>,
}

/// Replace an Artifact's content with a new Revision, or one Section of it.
#[utoipa::path(
    put, path = "/api/v1/artifacts/{id}",
    params(("id" = String, Path, description = "Bare Artifact id"), EditParams),
    request_body(content = UploadForm, content_type = "multipart/form-data"),
    responses((status = 200, description = "The new Revision", body = RefBody), ProblemResponses, WriteProblemResponses)
)]
async fn edit_artifact(
    State(s): State<ApiState>,
    P(id): P<String>,
    Q(q): Q<EditParams>,
    up: Upload,
) -> Result<Response, ApiError> {
    let id = bare_id(&id)?.to_string();
    let meta = up.meta.unwrap_or_default();
    meta.refuse(
        "edit",
        &[
            ("kind", meta.kind.is_some()),
            ("format", meta.format.is_some()),
            ("name", meta.name.is_some()),
            ("project_name", meta.project_name.is_some()),
            ("project_root", meta.project_root.is_some()),
            ("worktree", meta.worktree.is_some()),
        ],
    )?;
    let ctx = meta.ctx(&s.db_path)?;
    let (base, addresses) = (meta.base_revision, meta.addresses);
    let mut files = up.files;
    let r = match q.section {
        Some(section) => {
            if files.len() != 1 {
                return Err(
                    Error::Usage("a section edit takes exactly one file part".into()).into(),
                );
            }
            let bytes = files.remove(0).bytes;
            run(&s, move |c| {
                artifact::section_edit(c, &ctx, &id, &section, &bytes, base, &addresses)
            })
            .await?
        }
        None => {
            if files.is_empty() {
                return Err(Error::Usage("no file parts".into()).into());
            }
            let dir = files.len() > 1;
            if !dir {
                // As the CLI's stdin: edit keeps an Artifact's stored path.
                files[0].path = "-".into();
            }
            run(&s, move |c| {
                artifact::edit(c, &ctx, &id, Input { files, dir }, base, &addresses)
            })
            .await?
        }
    };
    Ok(json(&r))
}

/// Permanently delete an Artifact with its Revisions, Comments and embeds.
#[utoipa::path(
    delete, path = "/api/v1/artifacts/{id}", params(("id" = String, Path, description = "Bare Artifact id")),
    responses((status = 200, description = "Deleted", body = Deleted), ProblemResponses, WriteProblemResponses)
)]
async fn delete_artifact(
    State(s): State<ApiState>,
    P(id): P<String>,
) -> Result<Response, ApiError> {
    let id = bare_id(&id)?.to_string();
    let deleted = run(&s, move |c| artifact::delete(c, &[id])).await?;
    Ok(json(&Deleted { deleted }))
}

/// Body of `POST /api/v1/artifacts/{id}/comments`.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct AddComment {
    /// The Comment text, at most 64 KiB.
    body: String,
    /// Anchor to a line range of a text Artifact: `a-b` or `a`.
    lines: Option<String>,
    /// Anchor to a markdown Section (`A > B`).
    section: Option<String>,
    /// Root Comment id of the Thread to reply in; takes no `lines`/`section`.
    reply: Option<i64>,
    /// Comment on this Revision instead of the latest.
    revision: Option<i64>,
    /// `agent` (default) or `reviewer`.
    role: Option<String>,
    /// Defaults to the role name.
    author: Option<String>,
    session: Option<String>,
}

/// Add a root Comment (new Thread) or, with `reply`, a reply.
#[utoipa::path(
    post, path = "/api/v1/artifacts/{id}/comments", params(("id" = String, Path, description = "Bare Artifact id")),
    request_body = AddComment,
    responses((status = 201, description = "Added", body = comments::Added), ProblemResponses, WriteProblemResponses)
)]
async fn add_comment(
    State(s): State<ApiState>,
    P(id): P<String>,
    J(b): J<AddComment>,
) -> Result<Response, ApiError> {
    let id = bare_id(&id)?;
    let target = match b.revision {
        Some(n) => format!("{id}@{}", revision_number(&n.to_string())?),
        None => id.to_string(),
    };
    let lines = b
        .lines
        .as_deref()
        .map(|l| cli::parse_lines(l).map_err(Error::Usage))
        .transpose()?;
    let ctx = write_ctx(&s.db_path, &b.role, &b.author, &b.session)?;
    let locator = comments::Locator {
        lines,
        section: b.section,
    };
    let (body, reply) = (b.body, b.reply);
    let added = run(&s, move |c| {
        comments::add(c, &ctx, &target, &body, &locator, reply)
    })
    .await?;
    Ok(json_status(StatusCode::CREATED, &added))
}

async fn set_thread_status(
    s: &ApiState,
    id: i64,
    status: &'static str,
) -> Result<Response, ApiError> {
    Ok(json(
        &run(s, move |c| comments::set_status(c, id, status)).await?,
    ))
}

/// Mark a Thread resolved. Idempotent.
#[utoipa::path(
    post, path = "/api/v1/threads/{id}/resolve", params(("id" = i64, Path, description = "Root Comment id of the Thread")),
    responses((status = 200, description = "The Thread", body = comments::ThreadResult), ProblemResponses, WriteProblemResponses)
)]
async fn resolve_thread(State(s): State<ApiState>, P(id): P<i64>) -> Result<Response, ApiError> {
    set_thread_status(&s, id, "resolved").await
}

/// Mark a Thread open again. Idempotent.
#[utoipa::path(
    post, path = "/api/v1/threads/{id}/reopen", params(("id" = i64, Path, description = "Root Comment id of the Thread")),
    responses((status = 200, description = "The Thread", body = comments::ThreadResult), ProblemResponses, WriteProblemResponses)
)]
async fn reopen_thread(State(s): State<ApiState>, P(id): P<i64>) -> Result<Response, ApiError> {
    set_thread_status(&s, id, "open").await
}

/// Refuse a write that carries an `Origin` other than this server's own
/// `Host`. A cross-site form can POST a simple multipart or empty body to a
/// loopback listener, and the Host check does not stop it; browsers always
/// send `Origin` on such a request, while curl and agents send none.
async fn same_origin_writes(req: Request, next: Next) -> Response {
    let safe = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if !safe && let Some(origin) = req.headers().get(header::ORIGIN) {
        let host = req
            .headers()
            .get(header::HOST)
            .and_then(|h| h.to_str().ok());
        let origin = origin
            .to_str()
            .ok()
            .and_then(|o| o.split_once("://"))
            .map(|x| x.1);
        if !matches!((origin, host), (Some(o), Some(h)) if o == h) {
            return problem(
                StatusCode::FORBIDDEN,
                "forbidden",
                "this write carries an Origin header for another host".into(),
            );
        }
    }
    next.run(req).await
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Cairn API",
        description = "The Cairn JSON API. Loopback only, no auth, no CORS; a write carrying a foreign Origin is refused.",
        version = env!("CARGO_PKG_VERSION"),
    ),
    paths(
        list_artifacts,
        get_artifact,
        list_revisions,
        get_revision,
        get_file,
        get_outline,
        get_embeds,
        list_comments,
        create_artifact,
        edit_artifact,
        delete_artifact,
        add_comment,
        resolve_thread,
        reopen_thread,
    ),
    components(schemas(Problem))
)]
pub struct ApiDoc;

/// The OpenAPI 3.1 document, pretty-printed. Key order is deterministic.
pub fn openapi_json() -> String {
    let mut doc = ApiDoc::openapi();
    // utoipa fills a license name from Cargo.toml; this crate has none.
    doc.info.license = None;
    doc.to_pretty_json().expect("OpenAPI document serializes")
}

async fn openapi() -> Response {
    ([(header::CONTENT_TYPE, "application/json")], openapi_json()).into_response()
}

/// A route answering only GET (and HEAD); any other method is a 405.
fn read_only<H, T>(h: H) -> MethodRouter<ApiState>
where
    H: axum::handler::Handler<T, ApiState>,
    T: 'static,
{
    get(h)
}

fn upload_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(MAX_UPLOAD_BYTES)
}

pub(crate) fn router(db_path: PathBuf) -> Router {
    Router::new()
        .route("/api/v1/openapi.json", read_only(openapi))
        .route(
            "/api/v1/artifacts",
            get(list_artifacts)
                .post(create_artifact)
                .layer(upload_limit()),
        )
        .route(
            "/api/v1/artifacts/{id}",
            get(get_artifact)
                .put(edit_artifact)
                .delete(delete_artifact)
                .layer(upload_limit()),
        )
        .route(
            "/api/v1/artifacts/{id}/revisions",
            read_only(list_revisions),
        )
        .route(
            "/api/v1/artifacts/{id}/revisions/{n}",
            read_only(get_revision),
        )
        .route(
            "/api/v1/artifacts/{id}/revisions/{n}/files/{*path}",
            read_only(get_file),
        )
        .route("/api/v1/artifacts/{id}/outline", read_only(get_outline))
        .route("/api/v1/artifacts/{id}/embeds", read_only(get_embeds))
        .route(
            "/api/v1/artifacts/{id}/comments",
            get(list_comments).post(add_comment),
        )
        .route("/api/v1/threads/{id}/resolve", post(resolve_thread))
        .route("/api/v1/threads/{id}/reopen", post(reopen_thread))
        .route("/api/v1/{*rest}", axum::routing::any(unknown_route))
        .layer(from_fn(problem_405))
        .layer(from_fn(same_origin_writes))
        .with_state(ApiState { db_path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::{Input, RawFile};
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    const HOST: &str = "127.0.0.1:7878";

    struct Fixture {
        app: Router,
        db: PathBuf,
        _dir: tempfile::TempDir,
    }

    fn ctx(db: &Path) -> Context {
        Context {
            role: Role::Agent,
            author: "ann".into(),
            session: Some("s1".into()),
            session_explicit: Some("s1".into()),
            db_path: db.to_path_buf(),
            project: None,
        }
    }

    fn input(fs: &[(&str, &[u8])]) -> Input {
        Input {
            files: fs
                .iter()
                .map(|(p, b)| RawFile {
                    path: p.to_string(),
                    bytes: b.to_vec(),
                })
                .collect(),
            dir: fs.len() > 1,
        }
    }

    impl Fixture {
        fn new() -> Fixture {
            let dir = tempfile::tempdir().unwrap();
            let db = dir.path().join("c.db");
            db::open(&db).unwrap();
            Fixture {
                app: crate::serve::ui_router(db.clone(), "rev", 7879, 7878, false),
                db,
                _dir: dir,
            }
        }

        fn submit(
            &self,
            name: &str,
            kind: Kind,
            fs: &[(&str, &[u8])],
            scope: Option<crate::scope::Scope>,
        ) -> String {
            let mut conn = db::open(&self.db).unwrap();
            artifact::submit(
                &mut conn,
                &ctx(&self.db),
                artifact::SubmitInput {
                    name: name.into(),
                    kind,
                    format: None,
                    input: input(fs),
                    scope,
                },
            )
            .unwrap()
            .id
        }

        async fn get(&self, uri: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
            self.request("GET", HOST, uri).await
        }

        async fn request(
            &self,
            method: &str,
            host: &str,
            uri: &str,
        ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
            let res = self
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("host", host)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let (parts, body) = res.into_parts();
            (
                parts.status,
                parts.headers,
                body.collect().await.unwrap().to_bytes().to_vec(),
            )
        }

        async fn json(&self, uri: &str) -> Value {
            let (st, _, body) = self.get(uri).await;
            assert_eq!(
                st,
                StatusCode::OK,
                "{uri}: {}",
                String::from_utf8_lossy(&body)
            );
            serde_json::from_slice(&body).unwrap()
        }

        /// Assert a problem+json response and return its body.
        async fn problem(&self, uri: &str, status: u16, code: &str) -> Value {
            let (st, hd, body) = self.get(uri).await;
            assert_eq!(st.as_u16(), status, "{uri}");
            assert_eq!(
                hd.get("content-type").unwrap(),
                "application/problem+json",
                "{uri}"
            );
            let v: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(v["code"], code, "{uri}");
            assert_eq!(v["status"], status);
            assert_eq!(v["type"], "about:blank");
            assert!(v["detail"].is_string() && v["title"].is_string());
            v
        }
    }

    const MD: &str = "# T\n\nintro\n\n## A\n\nalpha\n\n## B\n\nbeta\n";

    #[tokio::test]
    async fn list_matches_the_cli_type_and_filters() {
        let f = Fixture::new();
        let a = f.submit("plan", Kind::Markdown, &[("a.md", MD.as_bytes())], None);
        f.submit("pic", Kind::Image, &[("p.png", b"\x89PNG\r\n\x1a\n")], None);
        let v = f.json("/api/v1/artifacts").await;
        assert_eq!(v["items"].as_array().unwrap().len(), 2);
        // Same value the CLI prints for the same filter.
        let conn = db::open(&f.db).unwrap();
        let rows = artifact::list(
            &conn,
            &ListFilter {
                id: None,
                kind: None,
                session: None,
                author: None,
                open: false,
                older_than_secs: None,
                limit: DEFAULT_LIMIT,
                scope: ScopeFilter::unscoped(),
            },
        )
        .unwrap();
        assert_eq!(
            v,
            serde_json::to_value(artifact::ArtifactList { items: rows }).unwrap()
        );
        let row = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == a.as_str())
            .unwrap();
        assert_eq!(row["bytes"], MD.len());
        assert!(row["updated_at"].is_string());
        let only = f.json("/api/v1/artifacts?kind=image").await;
        assert_eq!(only["items"].as_array().unwrap().len(), 1);
        assert_eq!(
            f.json("/api/v1/artifacts?session=s1").await["items"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(
            f.json("/api/v1/artifacts?session=zz").await["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            f.json("/api/v1/artifacts?open=true").await["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            f.json("/api/v1/artifacts?limit=1").await["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn list_scope_filters() {
        let f = Fixture::new();
        let scope = |root: &str, wt: Option<&str>| crate::scope::Scope {
            name: root.rsplit('/').next().unwrap().into(),
            root: root.into(),
            worktree: wt.map(Into::into),
        };
        f.submit(
            "p-root",
            Kind::Markdown,
            &[("a.md", b"# a\n")],
            Some(scope("/src/p", None)),
        );
        f.submit(
            "p-wt",
            Kind::Markdown,
            &[("a.md", b"# a\n")],
            Some(scope("/src/p", Some("/wt/x"))),
        );
        f.submit(
            "q",
            Kind::Markdown,
            &[("a.md", b"# a\n")],
            Some(scope("/src/q", None)),
        );
        let names = |v: Value| {
            let mut n: Vec<String> = v["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["name"].as_str().unwrap().to_string())
                .collect();
            n.sort();
            n
        };
        assert_eq!(
            names(f.json("/api/v1/artifacts").await),
            ["p-root", "p-wt", "q"]
        );
        assert_eq!(
            names(f.json("/api/v1/artifacts?project=p").await),
            ["p-root", "p-wt"]
        );
        assert_eq!(
            names(f.json("/api/v1/artifacts?project_root=/src/q").await),
            ["q"]
        );
        assert_eq!(
            names(f.json("/api/v1/artifacts?project_root=/src/p").await),
            ["p-root", "p-wt"]
        );
        assert_eq!(
            names(
                f.json("/api/v1/artifacts?project_root=/src/p&worktree=/wt/x")
                    .await
            ),
            ["p-root", "p-wt"]
        );
        assert_eq!(
            names(
                f.json("/api/v1/artifacts?project_root=/src/p&worktree=/wt/y")
                    .await
            ),
            ["p-root"]
        );
        f.problem("/api/v1/artifacts?project_root=rel", 400, "usage")
            .await;
        f.problem("/api/v1/artifacts?worktree=rel", 400, "usage")
            .await;
        // A Worktree without a Project would match every Project's root
        // Artifacts, so it is refused; with a name it narrows that Project.
        let d = f
            .problem("/api/v1/artifacts?worktree=/wt/x", 400, "usage")
            .await;
        assert!(d["detail"].as_str().unwrap().contains("project"));
        assert_eq!(
            names(f.json("/api/v1/artifacts?project=p&worktree=/wt/x").await),
            ["p-root", "p-wt"]
        );
    }

    #[tokio::test]
    async fn limit_default_and_max() {
        let f = Fixture::new();
        for i in 0..52 {
            f.submit(
                &format!("n{i}"),
                Kind::Markdown,
                &[("a.md", b"# a\n")],
                None,
            );
        }
        let n = |v: Value| v["items"].as_array().unwrap().len();
        assert_eq!(n(f.json("/api/v1/artifacts").await), 50);
        assert_eq!(n(f.json("/api/v1/artifacts?limit=200").await), 52);
        f.problem("/api/v1/artifacts?limit=201", 422, "rejected_input")
            .await;
        f.problem("/api/v1/artifacts?limit=0", 422, "rejected_input")
            .await;
        f.problem("/api/v1/artifacts?limit=abc", 400, "usage").await;
        f.problem("/api/v1/artifacts?kind=nope", 400, "usage").await;
        f.problem("/api/v1/artifacts?older_than=soon", 400, "usage")
            .await;
    }

    #[tokio::test]
    async fn artifact_revisions_and_manifest() {
        let f = Fixture::new();
        let id = f.submit("plan", Kind::Markdown, &[("a.md", MD.as_bytes())], None);
        let mut conn = db::open(&f.db).unwrap();
        artifact::edit(
            &mut conn,
            &ctx(&f.db),
            &id,
            input(&[("a.md", b"# T\n")]),
            None,
            &[],
        )
        .unwrap();
        let v = f.json(&format!("/api/v1/artifacts/{id}")).await;
        assert_eq!(v["id"], id.as_str());
        assert_eq!(v["latest_revision"], 2);
        assert!(v["created_at"].is_string());
        assert!(v["format"].is_null());
        let revs = f.json(&format!("/api/v1/artifacts/{id}/revisions")).await;
        let items = revs["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["number"], 1);
        assert_eq!(items[1]["author"], "ann");
        assert_eq!(items[1]["session"], "s1");
        let m = f.json(&format!("/api/v1/artifacts/{id}/revisions/1")).await;
        assert_eq!(m["artifact"], id.as_str());
        assert_eq!(m["number"], 1);
        let file = &m["files"][0];
        assert_eq!(file["path"], "plan");
        assert_eq!(file["media_type"], "text/markdown");
        assert_eq!(file["size"], MD.len());
        assert_eq!(
            file["sha256"],
            artifact::hex(&<sha2::Sha256 as sha2::Digest>::digest(MD.as_bytes())).as_str()
        );
        f.problem(
            &format!("/api/v1/artifacts/{id}/revisions/9"),
            404,
            "not_found",
        )
        .await;
        f.problem(&format!("/api/v1/artifacts/{id}/revisions/x"), 400, "usage")
            .await;
        f.problem(&format!("/api/v1/artifacts/{id}/revisions/0"), 400, "usage")
            .await;
        f.problem("/api/v1/artifacts/nope", 404, "not_found").await;
        f.problem("/api/v1/artifacts/nope/revisions", 404, "not_found")
            .await;
        f.problem(&format!("/api/v1/artifacts/{id}@1"), 400, "usage")
            .await;
    }

    #[tokio::test]
    async fn raw_files_are_attachments_with_nosniff() {
        let f = Fixture::new();
        let id = f.submit(
            "site",
            Kind::Html,
            &[("index.html", b"<h1>x</h1>"), ("css/s.css", b"a{}")],
            None,
        );
        let (st, hd, body) = f
            .get(&format!(
                "/api/v1/artifacts/{id}/revisions/1/files/css/s.css"
            ))
            .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body, b"a{}");
        assert_eq!(hd["content-type"], "text/css");
        assert_eq!(hd["content-disposition"], "attachment; filename=\"s.css\"");
        assert_eq!(hd["x-content-type-options"], "nosniff");
        let (_, hd, _) = f
            .get(&format!(
                "/api/v1/artifacts/{id}/revisions/1/files/index.html"
            ))
            .await;
        assert_eq!(hd["content-type"], "text/html");
        assert!(
            hd["content-disposition"]
                .to_str()
                .unwrap()
                .starts_with("attachment")
        );
        // Exact stored paths only: no traversal, no prefix match.
        f.problem(
            &format!("/api/v1/artifacts/{id}/revisions/1/files/..%2Findex.html"),
            404,
            "not_found",
        )
        .await;
        f.problem(
            &format!("/api/v1/artifacts/{id}/revisions/1/files/css"),
            404,
            "not_found",
        )
        .await;
        // Sections are markdown only.
        f.problem(
            &format!("/api/v1/artifacts/{id}/revisions/1/files/index.html?section=x"),
            400,
            "usage",
        )
        .await;
    }

    #[tokio::test]
    async fn markdown_section_and_outline_match_the_cli() {
        let f = Fixture::new();
        let id = f.submit("plan", Kind::Markdown, &[("a.md", MD.as_bytes())], None);
        let base = format!("/api/v1/artifacts/{id}/revisions/1/files/plan");
        let (_, hd, body) = f.get(&base).await;
        assert_eq!(body, MD.as_bytes());
        assert_eq!(hd["content-type"], "text/markdown");
        let (st, hd, body) = f.get(&format!("{base}?section=T%20%3E%20A")).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(String::from_utf8(body).unwrap(), "## A\n\nalpha\n\n");
        assert_eq!(hd["content-type"], "text/markdown");
        assert_eq!(hd["x-content-type-options"], "nosniff");
        assert!(
            hd["content-disposition"]
                .to_str()
                .unwrap()
                .starts_with("attachment")
        );
        f.problem(&format!("{base}?section=Nope"), 422, "rejected_input")
            .await;

        let v = f.json(&format!("/api/v1/artifacts/{id}/outline")).await;
        let conn = db::open(&f.db).unwrap();
        let text = artifact::markdown_text(&conn, &id, None).unwrap();
        assert_eq!(v, serde_json::to_value(sections::outline(&text)).unwrap());
        assert_eq!(v["sections"][1]["path"], "T > A");
        let at1 = f
            .json(&format!("/api/v1/artifacts/{id}/outline?revision=1"))
            .await;
        assert_eq!(at1, v);
        f.problem(
            &format!("/api/v1/artifacts/{id}/outline?revision=0"),
            400,
            "usage",
        )
        .await;
        let img = f.submit("pic", Kind::Image, &[("p.png", b"\x89PNG")], None);
        f.problem(
            &format!("/api/v1/artifacts/{img}/outline"),
            422,
            "rejected_input",
        )
        .await;
    }

    #[tokio::test]
    async fn embeds_match_the_cli_resolver() {
        let f = Fixture::new();
        let img = f.submit("pic", Kind::Image, &[("p.png", b"\x89PNG")], None);
        let md = format!("# T\n\n![alt](cairn:{img})\n");
        let id = f.submit("plan", Kind::Markdown, &[("a.md", md.as_bytes())], None);
        let v = f.json(&format!("/api/v1/artifacts/{id}/embeds")).await;
        assert_eq!(v["embeds"][0]["line"], 3);
        assert_eq!(v["embeds"][0]["alt"], "alt");
        assert_eq!(v["embeds"][0]["target_revision"], 1);
        assert_eq!(v["embeds"][0]["dangling"], false);
        f.problem(
            &format!("/api/v1/artifacts/{img}/embeds"),
            422,
            "rejected_input",
        )
        .await;
        f.problem(
            &format!("/api/v1/artifacts/{id}/embeds?revision=0"),
            400,
            "usage",
        )
        .await;
    }

    #[tokio::test]
    async fn comments_list() {
        let f = Fixture::new();
        let id = f.submit("plan", Kind::Markdown, &[("a.md", MD.as_bytes())], None);
        let mut conn = db::open(&f.db).unwrap();
        let added = comments::add(
            &mut conn,
            &ctx(&f.db),
            &id,
            "why?",
            &comments::Locator::default(),
            None,
        )
        .unwrap();
        let v = f.json(&format!("/api/v1/artifacts/{id}/comments")).await;
        assert_eq!(v["threads"][0]["body"], "why?");
        assert_eq!(v["highest_comment_id"], added.highest_comment_id);
        let after = f
            .json(&format!(
                "/api/v1/artifacts/{id}/comments?since={}",
                added.highest_comment_id
            ))
            .await;
        assert!(after["threads"].as_array().unwrap().is_empty());
        f.problem("/api/v1/artifacts/nope/comments", 404, "not_found")
            .await;
    }

    #[tokio::test]
    async fn unknown_routes_and_methods_are_problems() {
        let f = Fixture::new();
        f.problem("/api/v1/nothing", 404, "not_found").await;
        f.problem("/api/v1/artifacts/a/b/c/d", 404, "not_found")
            .await;
        for (method, uri) in [
            ("POST", "/api/v1/openapi.json"),
            ("PUT", "/api/v1/artifacts"),
            ("DELETE", "/api/v1/artifacts"),
            ("POST", "/api/v1/artifacts/a/revisions"),
            ("PUT", "/api/v1/artifacts/a/comments"),
            ("GET", "/api/v1/threads/1/resolve"),
            ("DELETE", "/api/v1/threads/1/reopen"),
        ] {
            let (st, hd, body) = f.request(method, HOST, uri).await;
            assert_eq!(st, StatusCode::METHOD_NOT_ALLOWED, "{method} {uri}");
            assert_eq!(hd["content-type"], "application/problem+json");
            assert_eq!(
                serde_json::from_slice::<Value>(&body).unwrap()["status"],
                405
            );
        }
        // The 405 names the allowed methods; HEAD works on GET routes.
        let (_, hd, _) = f.request("DELETE", HOST, "/api/v1/artifacts").await;
        let allow = hd["allow"].to_str().unwrap().to_string();
        for m in ["GET", "HEAD", "POST"] {
            assert!(allow.contains(m), "{allow}");
        }
        assert!(!allow.contains("DELETE"), "{allow}");
        let (_, hd, _) = f.request("POST", HOST, "/api/v1/openapi.json").await;
        assert!(hd["allow"].to_str().unwrap().contains("GET"));
        let (st, hd, body) = f.request("HEAD", HOST, "/api/v1/artifacts").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(hd["content-type"], "application/json");
        assert!(body.is_empty());
        // No CORS, and the hardening layer applies.
        let (_, hd, _) = f.get("/api/v1/artifacts").await;
        assert!(hd.get("access-control-allow-origin").is_none());
        assert_eq!(hd["x-content-type-options"], "nosniff");
    }

    #[tokio::test]
    async fn host_check_covers_the_api() {
        let f = Fixture::new();
        let (st, _, _) = f
            .request("GET", "evil.example:7878", "/api/v1/artifacts")
            .await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        let (st, _, _) = f
            .request("GET", "evil.example:7878", "/api/v1/openapi.json")
            .await;
        assert_eq!(st, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn database_failure_is_a_500_problem() {
        let f = Fixture::new();
        // A directory where the database file should be: open fails.
        let bad = f._dir.path().join("dir");
        std::fs::create_dir(&bad).unwrap();
        let app = crate::serve::ui_router(bad, "rev", 7879, 7878, false);
        let res = app
            .oneshot(
                Request::get("/api/v1/artifacts")
                    .header("host", HOST)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let v: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(v["code"], "unexpected");
    }

    #[test]
    fn every_error_maps_to_its_documented_status() {
        for (e, status) in [
            (Error::Usage(String::new()), 400),
            (Error::NotFound(String::new()), 404),
            (Error::Rejected(String::new()), 422),
            (Error::Conflict(String::new()), 409),
            (Error::Unexpected(String::new()), 500),
        ] {
            assert_eq!(ApiError(e).into_response().status().as_u16(), status);
        }
    }

    const SNAPSHOT: &str = "docs/openapi.json";

    #[test]
    fn openapi_snapshot_has_not_drifted() {
        let now = openapi_json();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT);
        if std::env::var_os("UPDATE_OPENAPI").is_some() {
            std::fs::write(&path, format!("{now}\n")).unwrap();
            return;
        }
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            committed.trim_end() == now.trim_end(),
            "the OpenAPI document no longer matches {SNAPSHOT}; \
             regenerate with: UPDATE_OPENAPI=1 cargo test openapi_snapshot"
        );
    }

    #[tokio::test]
    async fn openapi_is_served_and_is_3_1() {
        let f = Fixture::new();
        let v = f.json("/api/v1/openapi.json").await;
        assert!(v["openapi"].as_str().unwrap().starts_with("3.1"));
        for (m, p) in [
            ("post", "/api/v1/artifacts"),
            ("put", "/api/v1/artifacts/{id}"),
            ("delete", "/api/v1/artifacts/{id}"),
            ("post", "/api/v1/artifacts/{id}/comments"),
            ("post", "/api/v1/threads/{id}/resolve"),
            ("post", "/api/v1/threads/{id}/reopen"),
        ] {
            assert!(v["paths"][p][m].is_object(), "{m} {p}");
        }
        for p in [
            "/api/v1/artifacts",
            "/api/v1/artifacts/{id}",
            "/api/v1/artifacts/{id}/revisions",
            "/api/v1/artifacts/{id}/revisions/{n}",
            "/api/v1/artifacts/{id}/revisions/{n}/files/{path}",
            "/api/v1/artifacts/{id}/outline",
            "/api/v1/artifacts/{id}/embeds",
            "/api/v1/artifacts/{id}/comments",
        ] {
            assert!(v["paths"][p]["get"].is_object(), "{p}");
        }
    }

    // --- writes ---------------------------------------------------------------

    const BOUNDARY: &str = "XcairnX";

    /// A multipart body: an optional `meta` JSON part, then file parts.
    fn multipart(meta: Option<&str>, files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut b = Vec::new();
        if let Some(m) = meta {
            b.extend(
                format!(
                    "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"meta\"\r\n\
                     Content-Type: application/json\r\n\r\n{m}\r\n"
                )
                .into_bytes(),
            );
        }
        for (name, bytes) in files {
            b.extend(
                format!(
                    "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"; \
                     filename=\"x\"\r\nContent-Type: application/octet-stream\r\n\r\n"
                )
                .into_bytes(),
            );
            b.extend(*bytes);
            b.extend(b"\r\n");
        }
        b.extend(format!("--{BOUNDARY}--\r\n").into_bytes());
        b
    }

    type Reply = (StatusCode, Value);

    /// Assert a problem+json body's status and code.
    fn is_problem(r: &Reply, status: u16, code: &str) {
        assert_eq!(
            (r.0.as_u16(), r.1["code"].as_str()),
            (status, Some(code)),
            "{}",
            r.1
        );
    }

    impl Fixture {
        async fn send(
            &self,
            method: &str,
            uri: &str,
            content_type: &str,
            body: Vec<u8>,
            origin: Option<&str>,
        ) -> Reply {
            let mut req = Request::builder()
                .method(method)
                .uri(uri)
                .header("host", HOST)
                .header("content-type", content_type);
            if let Some(o) = origin {
                req = req.header("origin", o);
            }
            let res = self
                .app
                .clone()
                .oneshot(req.body(Body::from(body)).unwrap())
                .await
                .unwrap();
            let (parts, body) = res.into_parts();
            let bytes = body.collect().await.unwrap().to_bytes();
            (
                parts.status,
                serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            )
        }

        async fn upload(
            &self,
            method: &str,
            uri: &str,
            meta: Option<&str>,
            files: &[(&str, &[u8])],
        ) -> Reply {
            self.send(
                method,
                uri,
                &format!("multipart/form-data; boundary={BOUNDARY}"),
                multipart(meta, files),
                None,
            )
            .await
        }

        async fn post_json(&self, uri: &str, body: &str) -> Reply {
            self.send(
                "POST",
                uri,
                "application/json",
                body.as_bytes().to_vec(),
                None,
            )
            .await
        }

        async fn bare_post(&self, uri: &str) -> Reply {
            self.send("POST", uri, "text/plain", vec![], None).await
        }

        async fn file(&self, id: &str, rev: i64, path: &str) -> Vec<u8> {
            let (st, _, b) = self
                .get(&format!(
                    "/api/v1/artifacts/{id}/revisions/{rev}/files/{path}"
                ))
                .await;
            assert_eq!(st, StatusCode::OK);
            b
        }
    }

    #[tokio::test]
    async fn create_edit_section_edit_delete_markdown() {
        let f = Fixture::new();
        let (st, r) = f
            .upload(
                "POST",
                "/api/v1/artifacts",
                Some(r#"{"kind":"markdown","name":"plan.md","author":"ann","session":"s9"}"#),
                &[("ignored.md", MD.as_bytes())],
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{r}");
        let id = r["id"].as_str().unwrap().to_string();
        assert_eq!(r["revision"], 1);
        assert_eq!(r["ref"], format!("{id}@1"));
        let d = f.json(&format!("/api/v1/artifacts/{id}")).await;
        assert_eq!(d["name"], "plan.md");
        assert_eq!(d["author"], "ann");
        assert_eq!(f.file(&id, 1, "plan.md").await, MD.as_bytes());

        // Edit: any part name; the stored path is kept. Role defaults to agent.
        let uri = format!("/api/v1/artifacts/{id}");
        let (st, r) = f
            .upload(
                "PUT",
                &uri,
                Some(r#"{"base_revision":1}"#),
                &[("whatever", b"# New\n\ntext\n")],
            )
            .await;
        assert_eq!(st, StatusCode::OK, "{r}");
        assert_eq!(r["ref"], format!("{id}@2"));
        assert_eq!(f.file(&id, 2, "plan.md").await, b"# New\n\ntext\n");
        let rev = f.json(&format!("{uri}/revisions")).await;
        assert_eq!(rev["items"][1]["role"], "agent");
        assert_eq!(rev["items"][1]["author"], "agent");

        // Section edit.
        let (st, r) = f
            .upload(
                "PUT",
                &format!("{uri}?section=New"),
                None,
                &[("s", b"# New\n\nchanged\n")],
            )
            .await;
        assert_eq!(st, StatusCode::OK, "{r}");
        assert_eq!(r["revision"], 3);
        assert_eq!(f.file(&id, 3, "plan.md").await, b"# New\n\nchanged\n");
        let two = f
            .upload(
                "PUT",
                &format!("{uri}?section=New"),
                None,
                &[("a", b"x"), ("b", b"y")],
            )
            .await;
        is_problem(&two, 400, "usage");

        // Delete.
        let (st, hd, body) = f.request("DELETE", HOST, &uri).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(hd["content-type"], "application/json");
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            serde_json::json!({ "deleted": [id] })
        );
        f.problem(&uri, 404, "not_found").await;
        let (st, _, _) = f.request("DELETE", HOST, &uri).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn create_edit_every_other_kind() {
        let f = Fixture::new();
        // html Bundle: several parts keyed by path, replaced whole on edit.
        let (st, r) = f
            .upload(
                "POST",
                "/api/v1/artifacts",
                Some(r#"{"kind":"html","name":"site"}"#),
                &[("index.html", b"<p>hi"), ("css/a.css", b"p{}")],
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{r}");
        let id = r["id"].as_str().unwrap().to_string();
        assert_eq!(f.file(&id, 1, "css/a.css").await, b"p{}");
        let (st, r) = f
            .upload(
                "PUT",
                &format!("/api/v1/artifacts/{id}"),
                None,
                &[("index.html", b"<p>2"), ("b.js", b"1")],
            )
            .await;
        assert_eq!(st, StatusCode::OK, "{r}");
        let m = f.json(&format!("/api/v1/artifacts/{id}/revisions/2")).await;
        assert_eq!(m["files"].as_array().unwrap().len(), 2);
        // A single html file becomes the entry page.
        let (st, r) = f
            .upload(
                "POST",
                "/api/v1/artifacts",
                Some(r#"{"kind":"html"}"#),
                &[("page.html", b"<p>")],
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{r}");
        f.file(r["id"].as_str().unwrap(), 1, "index.html").await;

        // image: edit keeps the stored path whatever the part is called.
        let (st, r) = f
            .upload(
                "POST",
                "/api/v1/artifacts",
                Some(r#"{"kind":"image","name":"p.png"}"#),
                &[("up", b"\x89PNG\r\n\x1a\n")],
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{r}");
        let id = r["id"].as_str().unwrap().to_string();
        let (st, r) = f
            .upload(
                "PUT",
                &format!("/api/v1/artifacts/{id}"),
                None,
                &[("other.png", b"\x89PNG2")],
            )
            .await;
        assert_eq!(st, StatusCode::OK, "{r}");
        assert_eq!(f.file(&id, 2, "p.png").await, b"\x89PNG2");

        // diagrams: svg and mermaid.
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"1\"/></svg>";
        let (st, r) = f
            .upload(
                "POST",
                "/api/v1/artifacts",
                Some(r#"{"kind":"diagram","format":"svg","name":"d.svg"}"#),
                &[("d", svg)],
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{r}");
        let id = r["id"].as_str().unwrap().to_string();
        let (st, r) = f
            .upload(
                "PUT",
                &format!("/api/v1/artifacts/{id}"),
                None,
                &[("d", svg)],
            )
            .await;
        assert_eq!(st, StatusCode::OK, "{r}");
        let (st, r) = f
            .upload(
                "POST",
                "/api/v1/artifacts",
                Some(r#"{"kind":"diagram","format":"mermaid","name":"m.mmd"}"#),
                &[("m", b"flowchart LR\n A --> B\n")],
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{r}");
        let mid = r["id"].as_str().unwrap().to_string();
        let (st, r) = f
            .upload(
                "PUT",
                &format!("/api/v1/artifacts/{mid}"),
                None,
                &[("m", b"flowchart LR\n A --> C\n")],
            )
            .await;
        assert_eq!(st, StatusCode::OK, "{r}");
        for id in [id, mid] {
            let (st, _, _) = f
                .request("DELETE", HOST, &format!("/api/v1/artifacts/{id}"))
                .await;
            assert_eq!(st, StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn create_scope_and_attribution() {
        let f = Fixture::new();
        let (st, r) = f
            .upload(
                "POST",
                "/api/v1/artifacts",
                Some(r#"{"kind":"markdown","name":"a.md","role":"reviewer","project_root":"/r/proj","worktree":"/r/wt"}"#),
                &[("a", b"# A\n")],
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{r}");
        let d = f
            .json(&format!("/api/v1/artifacts/{}", r["id"].as_str().unwrap()))
            .await;
        assert_eq!(d["project"], "proj");
        assert_eq!(d["worktree"], "/r/wt");
        assert_eq!(d["author"], "reviewer");
        for meta in [
            r#"{"kind":"markdown","name":"a.md","project_name":"p"}"#,
            r#"{"kind":"markdown","name":"a.md","project_root":"rel"}"#,
            r#"{"kind":"markdown","name":"a.md","worktree":"/w"}"#,
            r#"{"kind":"markdown","name":"a.md","role":"boss"}"#,
            r#"{"kind":"markdown","name":"a.md","base_revision":1}"#,
            r#"{"kind":"markdown","name":"a.md","surprise":1}"#,
            r#"{"name":"a.md"}"#,
            "not json",
        ] {
            let r = f
                .upload("POST", "/api/v1/artifacts", Some(meta), &[("a", b"# A\n")])
                .await;
            is_problem(&r, 400, "usage");
        }
    }

    #[tokio::test]
    async fn stale_base_is_409_naming_latest_and_author() {
        let f = Fixture::new();
        let id = f.submit("plan", Kind::Markdown, &[("a.md", MD.as_bytes())], None);
        let uri = format!("/api/v1/artifacts/{id}");
        let (st, _) = f
            .upload("PUT", &uri, Some(r#"{"author":"bob"}"#), &[("a", b"# B\n")])
            .await;
        assert_eq!(st, StatusCode::OK);
        let r = f
            .upload(
                "PUT",
                &uri,
                Some(r#"{"base_revision":1}"#),
                &[("a", b"# C\n")],
            )
            .await;
        is_problem(&r, 409, "conflict");
        let d = r.1["detail"].as_str().unwrap();
        assert!(
            d.contains("latest_revision is 2") && d.contains("bob"),
            "{d}"
        );
        // Section edits check the base too.
        let r = f
            .upload(
                "PUT",
                &format!("{uri}?section=B"),
                Some(r#"{"base_revision":1}"#),
                &[("a", b"# B\n")],
            )
            .await;
        is_problem(&r, 409, "conflict");
        assert_eq!(f.json(&uri).await["latest_revision"], 2);
    }

    #[tokio::test]
    async fn rejected_input_is_422() {
        let f = Fixture::new();
        let md = r#"{"name":"a.md","kind":"markdown"}"#;
        // A diagram without a format, or a format on another Kind. (An unknown
        // kind or format value is malformed: see malformed_writes_are_400.)
        for meta in [
            r#"{"name":"a","kind":"diagram"}"#,
            r#"{"name":"a","kind":"markdown","format":"svg"}"#,
        ] {
            let r = f
                .upload("POST", "/api/v1/artifacts", Some(meta), &[("a", b"# A\n")])
                .await;
            is_problem(&r, 422, "rejected_input");
        }
        // Bad embed.
        let r = f
            .upload(
                "POST",
                "/api/v1/artifacts",
                Some(md),
                &[("a", b"![x](cairn:nope)\n")],
            )
            .await;
        is_problem(&r, 422, "rejected_input");
        // Over the per-file limit, and over the per-Revision limit by parts.
        let big = vec![b'a'; limits::MAX_FILE_BYTES as usize + 1];
        let r = f
            .upload("POST", "/api/v1/artifacts", Some(md), &[("a", &big)])
            .await;
        is_problem(&r, 422, "rejected_input");
        let part = vec![b'a'; limits::MAX_FILE_BYTES as usize];
        let html = r#"{"kind":"html","name":"s"}"#;
        let r = f
            .upload(
                "POST",
                "/api/v1/artifacts",
                Some(html),
                &[("index.html", &part), ("b.html", &part), ("c.html", &part)],
            )
            .await;
        is_problem(&r, 422, "rejected_input");
        // Too many files.
        let names: Vec<String> = (0..201).map(|i| format!("f{i}.txt")).collect();
        let files: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b"x"[..])).collect();
        let r = f
            .upload("POST", "/api/v1/artifacts", Some(html), &files)
            .await;
        is_problem(&r, 422, "rejected_input");
        // Unsafe and duplicate paths, no entry page.
        for files in [
            vec![("index.html", &b"x"[..]), ("../x.js", &b"x"[..])],
            vec![("index.html", &b"x"[..]), ("index.html", &b"y"[..])],
            vec![("a.html", &b"x"[..]), ("b.html", &b"y"[..])],
        ] {
            let r = f
                .upload("POST", "/api/v1/artifacts", Some(html), &files)
                .await;
            is_problem(&r, 422, "rejected_input");
        }
        // Nothing was stored.
        assert!(
            f.json("/api/v1/artifacts").await["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn malformed_writes_are_400() {
        let f = Fixture::new();
        let uri = "/api/v1/artifacts";
        is_problem(
            &f.upload("POST", uri, None, &[("a", b"# A\n")]).await,
            400,
            "usage",
        );
        is_problem(
            &f.upload("POST", uri, Some(r#"{"kind":"markdown"}"#), &[])
                .await,
            400,
            "usage",
        );
        is_problem(
            &f.send("POST", uri, "application/json", b"{}".to_vec(), None)
                .await,
            400,
            "usage",
        );
        // An unknown kind or format value is malformed, as on the list filter.
        for meta in [
            r#"{"name":"a","kind":"video"}"#,
            r#"{"name":"a","kind":"diagram","format":"dot"}"#,
        ] {
            is_problem(
                &f.upload("POST", uri, Some(meta), &[("a", b"# A\n")]).await,
                400,
                "usage",
            );
        }
        let bad = f
            .send(
                "POST",
                uri,
                &format!("multipart/form-data; boundary={BOUNDARY}"),
                b"garbage".to_vec(),
                None,
            )
            .await;
        is_problem(&bad, 400, "usage");
        is_problem(
            &f.upload("PUT", "/api/v1/artifacts/a@1", None, &[("a", b"x")])
                .await,
            400,
            "usage",
        );
        is_problem(
            &f.upload("PUT", "/api/v1/artifacts/nope", None, &[("a", b"x")])
                .await,
            404,
            "not_found",
        );
        let id = f.submit("plan", Kind::Markdown, &[("a.md", MD.as_bytes())], None);
        let r = f
            .upload(
                "PUT",
                &format!("/api/v1/artifacts/{id}"),
                Some(r#"{"kind":"html"}"#),
                &[("a", b"x")],
            )
            .await;
        is_problem(&r, 400, "usage");
    }

    #[tokio::test]
    async fn writes_refuse_a_foreign_origin() {
        let f = Fixture::new();
        let id = f.submit("plan", Kind::Markdown, &[("a.md", MD.as_bytes())], None);
        let ct = format!("multipart/form-data; boundary={BOUNDARY}");
        let body = multipart(
            Some(r#"{"kind":"markdown","name":"a.md"}"#),
            &[("a", b"# A\n")],
        );
        let r = f
            .send(
                "POST",
                "/api/v1/artifacts",
                &ct,
                body.clone(),
                Some("http://evil.example"),
            )
            .await;
        is_problem(&r, 403, "forbidden");
        for (m, u) in [
            ("DELETE", format!("/api/v1/artifacts/{id}")),
            ("POST", "/api/v1/threads/1/resolve".to_string()),
        ] {
            let (st, _) = f
                .send(m, &u, "text/plain", vec![], Some("http://evil.example"))
                .await;
            assert_eq!(st, StatusCode::FORBIDDEN, "{m} {u}");
        }
        assert_eq!(
            f.json("/api/v1/artifacts").await["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        // The server's own origin is allowed, as are requests with no Origin.
        let (st, _) = f
            .send(
                "POST",
                "/api/v1/artifacts",
                &ct,
                body,
                Some(&format!("http://{HOST}")),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED);
    }

    #[tokio::test]
    async fn comment_writes_match_the_cli() {
        let f = Fixture::new();
        let id = f.submit("plan", Kind::Markdown, &[("a.md", MD.as_bytes())], None);
        let uri = format!("/api/v1/artifacts/{id}/comments");

        let (st, added) = f
            .post_json(
                &uri,
                r#"{"body":"why?","lines":"1-2","role":"reviewer","author":"rae","session":"r1"}"#,
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{added}");
        // Same shape as comments::add.
        let expected = {
            let mut conn = db::open(&f.db).unwrap();
            serde_json::to_value(
                comments::add(
                    &mut conn,
                    &ctx(&f.db),
                    &id,
                    "same",
                    &comments::Locator {
                        lines: Some((1, 2)),
                        section: None,
                    },
                    None,
                )
                .unwrap(),
            )
            .unwrap()
        };
        let keys = |v: &Value| {
            let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            k.sort();
            k
        };
        assert_eq!(keys(&added), keys(&expected));
        assert_eq!(added["thread_id"], added["id"]);
        assert_eq!(added["locator"]["line_start"], 1);
        let tid = added["thread_id"].as_i64().unwrap();

        // Reply keeps the Thread; attribution is body fields.
        let (st, reply) = f
            .post_json(&uri, &format!(r#"{{"body":"because","reply":{tid}}}"#))
            .await;
        assert_eq!(st, StatusCode::CREATED, "{reply}");
        assert_eq!(reply["thread_id"], tid);
        let l = f.json(&uri).await;
        let t = l["threads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == tid)
            .unwrap();
        assert_eq!(t["author"], "rae");
        assert_eq!(t["role"], "reviewer");
        assert_eq!(t["session"], "r1");
        assert_eq!(t["replies"][0]["author"], "agent");

        // Resolve and reopen return the CLI's ThreadResult and are idempotent.
        let (st, v) = f.bare_post(&format!("/api/v1/threads/{tid}/resolve")).await;
        assert_eq!(st, StatusCode::OK, "{v}");
        assert_eq!(v["thread"]["status"], "resolved");
        let cli = {
            let mut conn = db::open(&f.db).unwrap();
            serde_json::to_value(comments::set_status(&mut conn, tid, "resolved").unwrap()).unwrap()
        };
        assert_eq!(v, cli);
        let (_, v) = f.bare_post(&format!("/api/v1/threads/{tid}/reopen")).await;
        assert_eq!(v["thread"]["status"], "open");

        // Errors.
        for (body, status, code) in [
            (r#"{"body":""}"#, 400, "usage"),
            (r#"{"body":"x","lines":"zz"}"#, 400, "usage"),
            (r#"{"body":"x","reply":99999}"#, 404, "not_found"),
            (r#"{"body":"x","bogus":1}"#, 400, "usage"),
            (r#"{"body":"x","role":"boss"}"#, 400, "usage"),
            (r#"{"body":"x","revision":9}"#, 404, "not_found"),
            ("nope", 400, "usage"),
        ] {
            is_problem(&f.post_json(&uri, body).await, status, code);
        }
        let big = format!(r#"{{"body":"{}"}}"#, "a".repeat(70 * 1024));
        is_problem(&f.post_json(&uri, &big).await, 422, "rejected_input");
        let r = f
            .post_json("/api/v1/artifacts/nope/comments", r#"{"body":"x"}"#)
            .await;
        is_problem(&r, 404, "not_found");
        is_problem(
            &f.bare_post("/api/v1/threads/99999/resolve").await,
            404,
            "not_found",
        );
        is_problem(
            &f.bare_post("/api/v1/threads/x/resolve").await,
            400,
            "usage",
        );
    }

    #[tokio::test]
    async fn long_poll_returns_early_and_times_out_empty() {
        let f = Fixture::new();
        let id = f.submit("plan", Kind::Markdown, &[("a.md", MD.as_bytes())], None);
        let uri = format!("/api/v1/artifacts/{id}/comments");
        let high = f.json(&uri).await["highest_comment_id"].as_i64().unwrap();

        // Timeout: a plain 200, no Threads, the same highest id.
        let t = std::time::Instant::now();
        let v = f.json(&format!("{uri}?since={high}&wait=1")).await;
        assert!(
            t.elapsed() >= Duration::from_millis(900),
            "{:?}",
            t.elapsed()
        );
        assert!(v["threads"].as_array().unwrap().is_empty());
        assert_eq!(v["highest_comment_id"], high);

        // A Comment added meanwhile ends the wait early.
        let db = f.db.clone();
        let id2 = id.clone();
        let adder = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(700));
            let mut conn = db::open(&db).unwrap();
            comments::add(
                &mut conn,
                &ctx(&db),
                &id2,
                "late",
                &comments::Locator::default(),
                None,
            )
            .unwrap();
        });
        let t = std::time::Instant::now();
        let v = f.json(&format!("{uri}?since={high}&wait=20")).await;
        adder.join().unwrap();
        assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
        assert_eq!(v["threads"][0]["body"], "late");

        // wait=0 answers at once; a bad value is a 400.
        let v = f.json(&format!("{uri}?wait=0")).await;
        assert_eq!(v["threads"].as_array().unwrap().len(), 1);
        f.problem(&format!("{uri}?wait=soon"), 400, "usage").await;
        f.problem("/api/v1/artifacts/nope/comments?wait=1", 404, "not_found")
            .await;
    }

    #[tokio::test]
    async fn long_poll_is_woken_by_a_comment_from_author_api() {
        let f = Fixture::new();
        let id = f.submit("plan", Kind::Markdown, &[("a.md", MD.as_bytes())], None);
        let uri = format!("/api/v1/artifacts/{id}/comments");
        let high = f.json(&uri).await["highest_comment_id"].as_i64().unwrap();
        let db = f.db.clone();
        let id2 = id.clone();
        let adder = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            let mut conn = db::open(&db).unwrap();
            let mut c = ctx(&db);
            c.author = "api".into();
            c.session = None;
            comments::add(
                &mut conn,
                &c,
                &id2,
                "from api",
                &comments::Locator::default(),
                None,
            )
            .unwrap();
        });
        let t = std::time::Instant::now();
        let v = f.json(&format!("{uri}?since={high}&wait=20")).await;
        adder.join().unwrap();
        assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
        assert_eq!(v["threads"][0]["body"], "from api");
    }

    /// An abandoned long poll must release its resources: with a single
    /// blocking thread, a request made after the dropped wait still completes.
    #[test]
    fn dropped_long_poll_frees_the_blocking_pool() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let f = Fixture::new();
        let id = f.submit("plan", Kind::Markdown, &[("a.md", MD.as_bytes())], None);
        rt.block_on(async {
            let uri = format!("/api/v1/artifacts/{id}/comments?wait=300");
            let dropped = tokio::time::timeout(Duration::from_millis(700), f.get(&uri)).await;
            assert!(dropped.is_err(), "the wait should still be running");
            let ok = tokio::time::timeout(Duration::from_secs(5), f.get("/api/v1/artifacts"))
                .await
                .expect("blocking pool is free");
            assert_eq!(ok.0, StatusCode::OK);
        });
    }

    #[test]
    fn wait_values() {
        let w = |s: Option<&str>| {
            CommentParams {
                all: None,
                since: None,
                wait: s.map(String::from),
            }
            .wait_secs()
            .unwrap()
        };
        assert_eq!(w(None), None);
        assert_eq!(w(Some("0")), None);
        assert_eq!(w(Some("false")), None);
        assert_eq!(w(Some("true")), Some(60));
        assert_eq!(w(Some("")), Some(60));
        assert_eq!(w(Some("5")), Some(5));
        assert_eq!(w(Some("99999")), Some(MAX_WAIT_SECS));
    }
}
