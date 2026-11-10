//! The read side of the JSON API under `/api/v1` (docs/api.md), merged into
//! the UI router so it shares the loopback Host check and the hardening
//! layer. Handlers call the same core functions as the CLI and serialize the
//! same named types, so the two outputs agree by construction.
//!
//! Every error is RFC 9457 `application/problem+json` carrying the core
//! error's stable `code`, including bad query strings, bad path segments,
//! unknown routes and wrong methods. The OpenAPI 3.1 document is derived from
//! the Rust types (ADR 0008) and served at `/api/v1/openapi.json`.

use std::path::{Path, PathBuf};

use axum::Router;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, get};
use rusqlite::Connection;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use utoipa::{IntoParams, OpenApi, ToSchema};

use crate::artifact::{self, ListFilter};
use crate::cli::{Context, Role};
use crate::comments::{self, CommentList};
use crate::db;
use crate::error::Error;
use crate::kind::Kind;
use crate::scope::{ScopeFilter, Selection};
use crate::sections::{self, Outline};

const NOSNIFF: HeaderName = HeaderName::from_static("x-content-type-options");
const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

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
    /// Reserved for writes (`edit` against a stale Revision); no read returns it.
    #[response(status = 409, content_type = "application/problem+json")]
    Conflict(Problem),
    /// Anything else, including a database busy past its timeout.
    #[response(status = 500, content_type = "application/problem+json")]
    Unexpected(Problem),
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

async fn wrong_method() -> Response {
    problem(
        StatusCode::METHOD_NOT_ALLOWED,
        Error::Usage(String::new()).code(),
        "this route does not accept that method; every v1 read is GET".into(),
    )
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
    F: FnOnce(&Connection) -> Result<T, Error> + Send + 'static,
{
    let db_path = s.db_path.clone();
    tokio::task::spawn_blocking(move || f(&db::open(&db_path)?))
        .await
        .map_err(|e| Error::Unexpected(format!("request task failed: {e}")))?
        .map_err(ApiError)
}

fn json<T: Serialize>(v: &T) -> Response {
    (
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
    /// plus that Worktree's, as the CLI does from inside one.
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

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct CommentParams {
    /// Include resolved Threads.
    all: Option<bool>,
    /// Only Threads holding a Comment with a larger id.
    since: Option<i64>,
}

/// Threads of an Artifact, with their replies.
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
    let db_path = s.db_path.clone();
    let list = run(&s, move |c| {
        let ctx = Context {
            role: Role::Reviewer,
            author: "api".into(),
            session: None,
            session_explicit: None,
            db_path,
            project: None,
        };
        comments::list(
            c,
            &ctx,
            &comments::ListQuery {
                artifact: Some(id),
                all: q.all.unwrap_or(false),
                since: q.since,
                scope: ScopeFilter::unscoped(),
            },
        )
    })
    .await?;
    Ok(json(&list))
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Cairn API",
        description = "Read routes of the Cairn JSON API. Loopback only, no auth, no CORS.",
        version = env!("CARGO_PKG_VERSION"),
    ),
    paths(
        list_artifacts,
        get_artifact,
        list_revisions,
        get_revision,
        get_file,
        get_outline,
        list_comments,
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

/// A route answering only GET; any other method is a problem+json 405.
fn read_only<H, T>(h: H) -> MethodRouter<ApiState>
where
    H: axum::handler::Handler<T, ApiState>,
    T: 'static,
{
    get(h).fallback(wrong_method)
}

pub(crate) fn router(db_path: PathBuf) -> Router {
    Router::new()
        .route("/api/v1/openapi.json", read_only(openapi))
        .route("/api/v1/artifacts", read_only(list_artifacts))
        .route("/api/v1/artifacts/{id}", read_only(get_artifact))
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
        .route("/api/v1/artifacts/{id}/comments", read_only(list_comments))
        .route("/api/v1/{*rest}", axum::routing::any(unknown_route))
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
                app: crate::serve::ui_router(db.clone(), "rev", 7879, 7878),
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
        let (st, hd, body) = f.request("POST", HOST, "/api/v1/artifacts").await;
        assert_eq!(st, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(hd["content-type"], "application/problem+json");
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap()["status"],
            405
        );
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
        let app = crate::serve::ui_router(bad, "rev", 7879, 7878);
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
        for p in [
            "/api/v1/artifacts",
            "/api/v1/artifacts/{id}",
            "/api/v1/artifacts/{id}/revisions",
            "/api/v1/artifacts/{id}/revisions/{n}",
            "/api/v1/artifacts/{id}/revisions/{n}/files/{path}",
            "/api/v1/artifacts/{id}/outline",
            "/api/v1/artifacts/{id}/comments",
        ] {
            assert!(v["paths"][p]["get"].is_object(), "{p}");
        }
    }
}
