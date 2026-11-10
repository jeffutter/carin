//! The review UI on the UI port: server-rendered with maud, enhanced by htmx.
//!
//! Handlers call core in-process. SQLite connections are not `Sync`, so each
//! request opens its own inside `spawn_blocking`. Everything an Agent supplied
//! (names, authors, Section titles, markdown) is untrusted: text goes through
//! maud's escaping, and markdown is rendered by [`render_blocks`], which
//! drops raw HTML and unsafe link schemes. The page CSP ([`csp`]) is the
//! second line of defence; it forbids inline script and style.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use axum::Form;
use axum::Router;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use maud::{DOCTYPE, Markup, PreEscaped, html};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::artifact::{self, ListFilter, ListRow};
use crate::cli::{Context, Role};
use crate::comments;
use crate::db::{self, db_err};
use crate::error::Error;
use crate::kind::Kind;
use crate::scope::{ScopeFilter, Selection};
use crate::sections;

const HTMX_JS: &str = include_str!(env!("HTMX_JS"));
const APP_CSS: &str = include_str!("ui/style.css");
const APP_JS: &str = include_str!("ui/app.js");
/// More rows than any one reviewer scrolls; the list has no paging yet.
const LIST_LIMIT: i64 = 10_000;

/// Short content hash: the asset URL changes exactly when the bytes do, so
/// the assets can be cached forever.
fn short_hash(s: &str) -> String {
    artifact::hex(&Sha256::digest(s.as_bytes()))[..12].to_string()
}

static HTMX_HASH: LazyLock<String> = LazyLock::new(|| short_hash(HTMX_JS));
static CSS_HASH: LazyLock<String> = LazyLock::new(|| short_hash(APP_CSS));
static JS_HASH: LazyLock<String> = LazyLock::new(|| short_hash(APP_JS));

/// The UI's own CSP. Script and style only from this origin (no inline), no
/// framing except by itself, Artifact images and frames only from the content
/// origin.
pub(crate) fn csp(content_port: u16) -> HeaderValue {
    let content = format!("http://127.0.0.1:{content_port}");
    HeaderValue::from_str(&format!(
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src {content}; \
         connect-src 'self'; frame-src {content}; form-action 'self'; base-uri 'none'; \
         frame-ancestors 'self'"
    ))
    .expect("CSP is ASCII")
}

#[derive(Clone)]
pub(crate) struct UiState {
    pub db_path: PathBuf,
    pub reviewer: String,
}

pub(crate) fn router(state: UiState) -> Router {
    Router::new()
        .route("/", get(list_page))
        .route("/a/{idrev}", get(artifact_page))
        .route("/a/{idrev}/live", get(live_fragment))
        .route("/a/{idrev}/compose", get(compose))
        .route("/a/{idrev}/threads", post(new_thread))
        .route("/a/{idrev}/threads/{thread}/reply", post(reply))
        .route("/a/{idrev}/threads/{thread}/status", post(set_status))
        .route("/static/{hash}/htmx.js", get(htmx_js))
        .route("/static/{hash}/app.css", get(app_css))
        .route("/static/{hash}/app.js", get(app_js))
        .with_state(state)
}

fn plain(status: StatusCode, body: &'static str) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
        .into_response()
}

pub(crate) fn not_found() -> Response {
    plain(StatusCode::NOT_FOUND, "not found")
}

/// An HTTP status with a fixed body.
type Failure = (StatusCode, &'static str);

/// Run `f` against a fresh connection off the async threads. `None` is a 404.
/// Error text could echo stored content, so every error is a fixed string:
/// bad input is a 400, a missing target a 404, anything else a 500.
async fn blocking<T, F>(db_path: &Path, f: F) -> Result<T, Failure>
where
    T: Send + 'static,
    F: FnOnce(&mut Connection) -> Result<Option<T>, Error> + Send + 'static,
{
    let db_path = db_path.to_path_buf();
    let r = tokio::task::spawn_blocking(move || f(&mut db::open(&db_path)?)).await;
    match r {
        Ok(Ok(Some(v))) => Ok(v),
        Ok(Ok(None)) | Ok(Err(Error::NotFound(_))) => Err((StatusCode::NOT_FOUND, "not found")),
        Ok(Err(Error::Usage(_) | Error::Rejected(_))) => {
            Err((StatusCode::BAD_REQUEST, "bad request"))
        }
        _ => Err((StatusCode::INTERNAL_SERVER_ERROR, "server error")),
    }
}

fn html_response(m: Markup) -> Response {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], m).into_response()
}

fn asset(hash: &str, expected: &str, ct: &'static str, body: &'static str) -> Response {
    if hash != expected {
        return not_found();
    }
    (
        [
            (header::CONTENT_TYPE, ct),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        body,
    )
        .into_response()
}

async fn htmx_js(UrlPath(hash): UrlPath<String>) -> Response {
    asset(&hash, &HTMX_HASH, "text/javascript; charset=utf-8", HTMX_JS)
}

async fn app_css(UrlPath(hash): UrlPath<String>) -> Response {
    asset(&hash, &CSS_HASH, "text/css; charset=utf-8", APP_CSS)
}

async fn app_js(UrlPath(hash): UrlPath<String>) -> Response {
    asset(&hash, &JS_HASH, "text/javascript; charset=utf-8", APP_JS)
}

fn layout(s: &UiState, title: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width,initial-scale=1";
                // htmx would inject an inline style and may eval attributes;
                // the CSP forbids both, so turn them off rather than log errors.
                meta name="htmx-config" content=r#"{"includeIndicatorStyles":false,"allowEval":false}"#;
                title { (title) " - cairn" }
                link rel="stylesheet" href={ "/static/" (*CSS_HASH) "/app.css" };
                script src={ "/static/" (*HTMX_HASH) "/htmx.js" } defer {}
                script src={ "/static/" (*JS_HASH) "/app.js" } defer {}
            }
            body {
                header.top {
                    b { a href="/" { "cairn" } }
                    span.mut.small { "Reviewing as " (s.reviewer) }
                }
                (body)
            }
        }
    }
}

// --- list ---------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct ListParams {
    project: Option<String>,
    kind: Option<String>,
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.is_empty())
}

struct ListView {
    rows: Vec<ListRow>,
    projects: Vec<String>,
}

fn list_view(
    conn: &Connection,
    project: Option<&str>,
    kind: Option<Kind>,
) -> Result<ListView, Error> {
    let filter = |kind, scope| ListFilter {
        kind,
        session: None,
        author: None,
        open: false,
        older_than_secs: None,
        limit: LIST_LIMIT,
        scope,
    };
    // Unscoped by default: the UI is not tied to a cwd.
    let scope = match project {
        Some(p) => ScopeFilter::build(
            &Selection::Name(p.to_string()),
            false,
            true,
            None,
            Path::new(""),
        ),
        None => ScopeFilter::unscoped(),
    };
    let rows = artifact::list(conn, &filter(kind, scope))?;
    let mut projects: Vec<String> = artifact::list(conn, &filter(None, ScopeFilter::unscoped()))?
        .into_iter()
        .filter_map(|r| r.project)
        .collect();
    projects.sort();
    projects.dedup();
    Ok(ListView { rows, projects })
}

fn human_bytes(n: i64) -> String {
    match n {
        n if n < 1024 => format!("{n} B"),
        n if n < 1024 * 1024 => format!("{:.1} KiB", n as f64 / 1024.0),
        n => format!("{:.1} MiB", n as f64 / (1024.0 * 1024.0)),
    }
}

/// `2026-11-10T01:42:00.123Z` as `2026-11-10 01:42`.
fn short_time(ts: &str) -> String {
    ts.get(..16).unwrap_or(ts).replace('T', " ")
}

fn table(rows: &[ListRow]) -> Markup {
    html! {
        // Polled so Open Thread counts follow resolves and replies made elsewhere.
        div #artifacts data-poll hx-get="/" hx-trigger="every 2s" hx-swap="outerHTML"
            hx-include=".filters" {
            @if rows.is_empty() {
                p.mut { "No Artifacts." }
            } @else {
                table {
                    thead { tr {
                        th { "Name" } th { "Kind" } th { "Project" } th.num { "Revision" }
                        th.num { "Open Threads" } th.num { "Size" } th { "Updated" }
                    } }
                    tbody {
                        @for r in rows {
                            tr {
                                td { a href={ "/a/" (r.id) } { (r.name) } }
                                td { span.pill { (r.kind) } }
                                td {
                                    (r.project.as_deref().unwrap_or("-"))
                                    @if let Some(w) = &r.worktree { span.mut { " @ " (w) } }
                                }
                                td.num { "r" (r.latest_revision) }
                                td.num {
                                    @if r.open_threads > 0 { span.pill.open { (r.open_threads) } }
                                    @else { span.mut { "0" } }
                                }
                                td.num { (human_bytes(r.bytes)) }
                                td { (short_time(&r.updated_at)) }
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn list_page(
    State(s): State<UiState>,
    Query(p): Query<ListParams>,
    headers: HeaderMap,
) -> Response {
    let project = non_empty(p.project);
    let kind = match non_empty(p.kind).as_deref().map(Kind::parse) {
        None => None,
        Some(Ok(k)) => Some(k),
        Some(Err(_)) => return plain(StatusCode::BAD_REQUEST, "bad kind"),
    };
    let (proj, k) = (project.clone(), kind);
    let view = match blocking(&s.db_path, move |c| {
        list_view(c, proj.as_deref(), k).map(Some)
    })
    .await
    {
        Ok(v) => v,
        Err(r) => return r.into_response(),
    };
    // htmx swaps only the table when a filter changes. A history restore
    // wants the whole page, and so does any other request.
    let fragment = headers.get("hx-target").is_some_and(|t| t == "artifacts")
        && !headers.contains_key("hx-history-restore-request");
    let mut res = if fragment {
        html_response(table(&view.rows))
    } else {
        let body = html! {
            div.wrap {
                form.filters method="get" action="/"
                    hx-get="/" hx-trigger="change, submit" hx-target="#artifacts"
                    hx-select="#artifacts" hx-swap="outerHTML" hx-push-url="true" {
                    label { "Project " select name="project" {
                        option value="" { "All" }
                        @for name in &view.projects {
                            option value=(name) selected[project.as_deref() == Some(name)] { (name) }
                        }
                    } }
                    label { "Kind " select name="kind" {
                        option value="" { "All" }
                        @for k in [Kind::Markdown, Kind::Html, Kind::Image, Kind::Diagram] {
                            option value=(k.as_str()) selected[kind == Some(k)] { (k.as_str()) }
                        }
                    } }
                    button type="submit" { "Filter" }
                }
                (table(&view.rows))
            }
        };
        html_response(layout(&s, "Artifacts", body))
    };
    res.headers_mut().insert(
        header::VARY,
        HeaderValue::from_static("hx-target, hx-history-restore-request"),
    );
    res
}

// --- Artifact page --------------------------------------------------------------

struct RevInfo {
    number: i64,
    author: String,
    created_at: String,
}

struct OutlineItem {
    level: u8,
    text: String,
    open_threads: usize,
}

enum Body {
    Markdown {
        doc: Markup,
        outline: Vec<OutlineItem>,
    },
    Other,
}

struct Page {
    id: String,
    name: String,
    kind: Kind,
    format: Option<String>,
    project: Option<String>,
    worktree: Option<String>,
    revisions: Vec<RevInfo>,
    viewing: i64,
    latest: i64,
    /// [`comments::change_marker`] read before the Threads, so the page is at
    /// least as new as the marker it carries.
    marker: String,
    body: Body,
}

/// The identity UI posts carry: the `--reviewer-name`, role reviewer, no Session.
fn reviewer_ctx(db_path: &Path, reviewer: &str) -> Context {
    Context {
        role: Role::Reviewer,
        author: reviewer.to_string(),
        session: None,
        session_explicit: None,
        db_path: db_path.to_path_buf(),
        project: None,
    }
}

/// 1-based line of a byte offset.
fn line_of(text: &str, offset: usize) -> i64 {
    text[..offset.min(text.len())].matches('\n').count() as i64 + 1
}

/// Open Threads per heading of `text`, in heading order. A Thread belongs to
/// the Section its locator names; a line Thread on the viewed Revision belongs
/// to the innermost Section containing its first line.
fn thread_counts(text: &str, viewing: i64, threads: &[comments::Thread]) -> Vec<usize> {
    let headings = sections::parse(text);
    let spans: Vec<(i64, i64)> = headings
        .iter()
        .map(|h| {
            (
                line_of(text, h.start),
                line_of(text, h.end.saturating_sub(1).max(h.start)),
            )
        })
        .collect();
    let mut counts = vec![0usize; headings.len()];
    for t in threads.iter().filter(|t| t.status == "open") {
        let idx = match (&t.locator.section_path, t.locator.line_start) {
            (Some(p), _) => headings.iter().position(|h| h.display_path() == *p),
            (None, Some(l)) if t.revision == viewing => {
                spans.iter().rposition(|&(from, to)| from <= l && l <= to)
            }
            _ => None,
        };
        if let Some(i) = idx {
            counts[i] += 1;
        }
    }
    counts
}

/// The Revision a request views: the named one if it exists, else the latest.
/// `None` when the Artifact or the named Revision does not exist.
fn viewing_rev(conn: &Connection, id: &str, rev: Option<i64>) -> Result<Option<i64>, Error> {
    let latest: Option<i64> = conn
        .query_row(
            "SELECT MAX(number) FROM revisions WHERE artifact_id = ?1",
            [id],
            |r| r.get(0),
        )
        .map_err(db_err)?;
    Ok(match (latest, rev) {
        (None, _) => None,
        (Some(l), None) => Some(l),
        (Some(l), Some(n)) => (1..=l).contains(&n).then_some(n),
    })
}

fn load_page(
    ctx: &Context,
    conn: &Connection,
    id: &str,
    rev: Option<i64>,
) -> Result<Option<Page>, Error> {
    type Head = (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let head: Option<Head> = conn
        .query_row(
            "SELECT name, kind, format, project_name, worktree FROM artifacts WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()
        .map_err(db_err)?;
    let Some((name, kind, format, project, worktree)) = head else {
        return Ok(None);
    };
    let kind = Kind::parse(&kind)?;
    let mut stmt = conn
        .prepare(
            "SELECT number, author, created_at FROM revisions
             WHERE artifact_id = ?1 ORDER BY number DESC",
        )
        .map_err(db_err)?;
    let revisions = stmt
        .query_map([id], |r| {
            Ok(RevInfo {
                number: r.get(0)?,
                author: r.get(1)?,
                created_at: r.get(2)?,
            })
        })
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    let Some(latest) = revisions.first().map(|r| r.number) else {
        return Ok(None);
    };
    let viewing = rev.unwrap_or(latest);
    if !revisions.iter().any(|r| r.number == viewing) {
        return Ok(None);
    }
    let marker = comments::change_marker(conn, id)?;
    let body = if kind == Kind::Markdown {
        let text = artifact::markdown_text(conn, id, Some(viewing))?;
        let list = comments::list(
            conn,
            ctx,
            &comments::ListQuery {
                artifact: Some(id.to_string()),
                all: true,
                since: None,
                scope: ScopeFilter::unscoped(),
            },
        )?;
        // Open Threads from any Revision; a resolved Thread only on its own.
        let threads: Vec<comments::Thread> = list
            .threads
            .into_iter()
            .filter(|t| t.status == "open" || t.revision == viewing)
            .collect();
        let counts = thread_counts(&text, viewing, &threads);
        let outline = sections::parse(&text)
            .into_iter()
            .zip(counts)
            .map(|(h, open_threads)| OutlineItem {
                level: h.level,
                text: h.text,
                open_threads,
            })
            .collect();
        Body::Markdown {
            doc: doc_markup(&text, &format!("/a/{id}@{viewing}"), viewing, &threads),
            outline,
        }
    } else {
        Body::Other
    };
    Ok(Some(Page {
        id: id.to_string(),
        name,
        kind,
        format,
        project,
        worktree,
        revisions,
        viewing,
        latest,
        marker,
        body,
    }))
}

#[derive(Deserialize, Default)]
struct PageParams {
    rev: Option<String>,
}

async fn artifact_page(
    State(s): State<UiState>,
    UrlPath(idrev): UrlPath<String>,
    Query(q): Query<PageParams>,
) -> Response {
    let Ok((id, rev)) = artifact::parse_target(&idrev) else {
        return not_found();
    };
    // The revision dropdown is a plain GET form; send it to the canonical URL.
    if let Some(n) = non_empty(q.rev) {
        return match artifact::parse_target(&format!("{id}@{n}")) {
            Ok((id, Some(n))) if rev.is_none() => {
                Redirect::to(&format!("/a/{id}@{n}")).into_response()
            }
            _ => not_found(),
        };
    }
    let ctx = reviewer_ctx(&s.db_path, &s.reviewer);
    let page = match blocking(&s.db_path, move |c| load_page(&ctx, c, &id, rev)).await {
        Ok(p) => p,
        Err(r) => return r.into_response(),
    };
    html_response(layout(&s, &page.name, page_markup(&page)))
}

// --- live region and Thread actions ---------------------------------------------

/// Parse the `{id}@{rev}` URL segment of a fragment or action route.
fn target(idrev: &str) -> Result<(String, Option<i64>), Failure> {
    artifact::parse_target(idrev).map_err(|_| (StatusCode::NOT_FOUND, "not found"))
}

#[derive(Deserialize, Default)]
struct LiveParams {
    m: Option<String>,
}

/// The poll target. When the client's marker is current there is nothing to
/// redraw: 204, which htmx does not swap.
async fn live_fragment(
    State(s): State<UiState>,
    UrlPath(idrev): UrlPath<String>,
    Query(q): Query<LiveParams>,
) -> Response {
    let (id, rev) = match target(&idrev) {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };
    let ctx = reviewer_ctx(&s.db_path, &s.reviewer);
    let r = blocking(&s.db_path, move |c| {
        let Some(viewing) = viewing_rev(c, &id, rev)? else {
            return Ok(None);
        };
        if q.m.as_deref() == Some(comments::change_marker(c, &id)?.as_str()) {
            return Ok(Some(None));
        }
        Ok(load_page(&ctx, c, &id, Some(viewing))?.map(|p| Some(live_markup(&p))))
    })
    .await;
    match r {
        Ok(Some(m)) => html_response(m),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => e.into_response(),
    }
}

/// Writes need a header a cross-site form cannot send (htmx sets `HX-Request`),
/// and an `Origin`, when the browser sends one, naming this host.
fn check_post(headers: &HeaderMap) -> Result<(), Failure> {
    let origin_ok = match headers.get(header::ORIGIN) {
        None => true,
        Some(o) => {
            let host = headers.get(header::HOST).and_then(|h| h.to_str().ok());
            let origin = o
                .to_str()
                .ok()
                .and_then(|o| o.split_once("://"))
                .map(|x| x.1);
            matches!((origin, host), (Some(o), Some(h)) if o == h)
        }
    };
    if origin_ok && headers.contains_key("hx-request") {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "forbidden"))
    }
}

/// Run a write on the viewed Revision of an Artifact, then answer with the
/// redrawn live region. `act` gets the connection, the Reviewer's context, the
/// Artifact id and the viewed Revision.
async fn mutate<F>(s: &UiState, headers: &HeaderMap, idrev: &str, act: F) -> Response
where
    F: FnOnce(&mut Connection, &Context, &str, i64) -> Result<(), Error> + Send + 'static,
{
    if let Err(e) = check_post(headers) {
        return e.into_response();
    }
    let (id, rev) = match target(idrev) {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };
    let ctx = reviewer_ctx(&s.db_path, &s.reviewer);
    let r = blocking(&s.db_path, move |c| {
        let Some(viewing) = viewing_rev(c, &id, rev)? else {
            return Ok(None);
        };
        act(c, &ctx, &id, viewing)?;
        Ok(load_page(&ctx, c, &id, Some(viewing))?.map(|p| live_markup(&p)))
    })
    .await;
    match r {
        Ok(m) => html_response(m),
        Err(e) => e.into_response(),
    }
}

#[derive(Deserialize, Default)]
struct ComposeParams {
    line: Option<i64>,
    section: Option<String>,
}

/// The inline form that starts a Thread at a line or Section.
async fn compose(UrlPath(idrev): UrlPath<String>, Query(q): Query<ComposeParams>) -> Response {
    let (id, rev) = match target(&idrev) {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };
    if q.line.is_none() == q.section.is_none() {
        return plain(StatusCode::BAD_REQUEST, "bad request");
    }
    let base = match rev {
        Some(n) => format!("/a/{id}@{n}"),
        None => format!("/a/{id}"),
    };
    html_response(html! {
        form.compose hx-post={ (base) "/threads" } hx-target="#live" hx-swap="outerHTML" {
            @if let Some(l) = q.line {
                input type="hidden" name="line" value=(l);
                div.mut.small { "Comment on line " (l) }
            }
            @if let Some(sec) = &q.section {
                input type="hidden" name="section" value=(sec);
                div.mut.small { "Comment on Section " (sec) }
            }
            textarea name="body" rows="3" required placeholder="Comment" autofocus {}
            div.row {
                button type="submit" { "Comment" }
                button type="button" data-cancel { "Cancel" }
            }
        }
    })
}

#[derive(Deserialize)]
struct NewThread {
    body: String,
    line: Option<i64>,
    section: Option<String>,
}

async fn new_thread(
    State(s): State<UiState>,
    UrlPath(idrev): UrlPath<String>,
    headers: HeaderMap,
    Form(f): Form<NewThread>,
) -> Response {
    mutate(&s, &headers, &idrev, move |c, ctx, id, viewing| {
        let loc = comments::Locator {
            lines: f.line.map(|l| (l, l)),
            section: f.section,
        };
        comments::add(c, ctx, &format!("{id}@{viewing}"), &f.body, &loc, None).map(drop)
    })
    .await
}

#[derive(Deserialize)]
struct ReplyForm {
    body: String,
}

async fn reply(
    State(s): State<UiState>,
    UrlPath((idrev, thread)): UrlPath<(String, i64)>,
    headers: HeaderMap,
    Form(f): Form<ReplyForm>,
) -> Response {
    mutate(&s, &headers, &idrev, move |c, ctx, id, _| {
        // A reply stays on its Thread's Revision, so name the Artifact only.
        comments::add(
            c,
            ctx,
            id,
            &f.body,
            &comments::Locator::default(),
            Some(thread),
        )
        .map(drop)
    })
    .await
}

#[derive(Deserialize)]
struct StatusForm {
    status: String,
}

async fn set_status(
    State(s): State<UiState>,
    UrlPath((idrev, thread)): UrlPath<(String, i64)>,
    headers: HeaderMap,
    Form(f): Form<StatusForm>,
) -> Response {
    mutate(&s, &headers, &idrev, move |c, _, id, _| {
        if !matches!(f.status.as_str(), "open" | "resolved") {
            return Err(Error::Usage("bad status".into()));
        }
        // set_status takes any root id; make sure it is this Artifact's.
        comments::check_addressable(c, id, &[thread])?;
        comments::set_status(c, thread, &f.status).map(drop)
    })
    .await
}

// --- page markup ------------------------------------------------------------------

/// The part of the Artifact page that refreshes: the banner, outline, text and
/// Threads. Markdown pages poll with their change marker; a 204 means no change.
fn live_markup(p: &Page) -> Markup {
    let here = format!("/a/{}", p.id);
    let polling = matches!(p.body, Body::Markdown { .. });
    html! {
        div #live data-poll[polling]
            hx-get=[polling.then(|| format!("{here}@{}/live?m={}", p.viewing, p.marker))]
            hx-trigger=[polling.then_some("every 2s")]
            hx-swap=[polling.then_some("outerHTML")] {
            @if p.viewing != p.latest {
                div.banner {
                    "Viewing r" (p.viewing) ", not the latest (r" (p.latest) "). "
                    a.latest href={ (here) "@" (p.latest) } { "Go to latest" }
                }
            }
            @match &p.body {
                Body::Markdown { doc, outline } => {
                    div.lay {
                        nav.side aria-label="Outline" {
                            @for (i, e) in outline.iter().enumerate() {
                                a class={ "lvl" (e.level) } href={ "#s" (i + 1) } {
                                    span { (e.text) }
                                    @if e.open_threads > 0 { span.pill.open { (e.open_threads) } }
                                }
                            }
                        }
                        article.doc { (doc) }
                    }
                }
                Body::Other => {
                    div.placeholder {
                        p { "This " (p.kind.as_str()) " Artifact is not shown in the UI yet." }
                        p.mut.small { "Use `cairn get " (p.id) "@" (p.viewing) "` to read it." }
                    }
                }
            }
        }
    }
}

fn page_markup(p: &Page) -> Markup {
    let kind_label = match &p.format {
        Some(f) => format!("{}/{f}", p.kind.as_str()),
        None => p.kind.as_str().to_string(),
    };
    let here = format!("/a/{}", p.id);
    html! {
        div.wrap #page {
            div.titlebar {
                h1 { (p.name) }
                span.pill { (kind_label) }
                @if let Some(proj) = &p.project {
                    span.pill {
                        (proj)
                        @if let Some(w) = &p.worktree { " @ " (w) }
                    }
                }
                form.revpick method="get" action=(here)
                    hx-get=(here) hx-trigger="change" hx-target="#page"
                    hx-select="#page" hx-swap="outerHTML" hx-push-url="true" {
                    label.mut.small for="rev" { "Revision" }
                    select #rev name="rev" {
                        @for r in &p.revisions {
                            option value=(r.number) selected[r.number == p.viewing] {
                                "r" (r.number) " - " (r.author) " - " (short_time(&r.created_at))
                            }
                        }
                    }
                    button type="submit" { "Go" }
                }
            }
            (live_markup(p))
        }
    }
}

// --- the document with its Threads --------------------------------------------------

/// The text of one Revision as blocks (gutter, rendered markdown, Threads
/// anchored to it, an empty slot for the compose form). A Thread goes under
/// the heading of the Section it names, or under the block holding its first
/// line when it was made on this Revision. Anything else, such as a line
/// Thread from an earlier Revision whose lines may have moved, is listed first.
fn doc_markup(text: &str, base: &str, viewing: i64, threads: &[comments::Thread]) -> Markup {
    let blocks = render_blocks(text);
    let paths: Vec<String> = sections::parse(text)
        .iter()
        .map(|h| h.display_path())
        .collect();
    let mut under: Vec<Vec<&comments::Thread>> = vec![Vec::new(); blocks.len()];
    let mut floating: Vec<&comments::Thread> = Vec::new();
    for t in threads {
        let at = match (&t.locator.section_path, t.locator.line_start) {
            (Some(p), _) => paths
                .iter()
                .position(|x| x == p)
                .and_then(|h| blocks.iter().position(|b| b.heading == Some(h))),
            (None, Some(l)) if t.revision == viewing => blocks
                .iter()
                .rposition(|b| b.first <= l)
                .or((!blocks.is_empty()).then_some(0)),
            _ => None,
        };
        match at {
            Some(i) => under[i].push(t),
            None => floating.push(t),
        }
    }
    html! {
        @if !floating.is_empty() {
            section.floating {
                h3 { "Threads without a place in this Revision" }
                @for t in &floating { (thread_card(t, base, viewing)) }
            }
        }
        @for (i, b) in blocks.iter().enumerate() {
            div.blk {
                div.gut {
                    @for l in b.first..=b.last {
                        button.ln type="button" title="Comment on this line"
                            hx-get={ (base) "/compose?line=" (l) }
                            hx-target={ "#slot-" (i) } hx-swap="innerHTML" { (l) }
                    }
                }
                div.bc {
                    @if let Some(h) = b.heading {
                        button.addsec type="button" hx-get={ (base) "/compose" }
                            hx-vals=(serde_json::json!({ "section": paths[h] }).to_string())
                            hx-target={ "#slot-" (i) } hx-swap="innerHTML" { "+ section" }
                    }
                    (PreEscaped(&b.html))
                }
                @if !under[i].is_empty() {
                    div.thr { @for t in &under[i] { (thread_card(t, base, viewing)) } }
                }
                div.slot id={ "slot-" (i) } {}
            }
        }
    }
}

fn thread_card(t: &comments::Thread, base: &str, viewing: i64) -> Markup {
    let earlier = t.revision != viewing;
    let resolved = t.status == "resolved";
    let (to, verb) = if resolved {
        ("open", "Reopen")
    } else {
        ("resolved", "Resolve")
    };
    let loc = &t.locator;
    html! {
        div.thread.earlier[earlier].resolved[resolved] id={ "t" (t.id) } {
            div.th {
                b { (t.author) } span.pill { (t.role) }
                @if earlier { span.pill.label { "earlier revision" } span.mut.small { "r" (t.revision) } }
                @if let Some(p) = &loc.section_path {
                    span.mut.small { "on " (p) }
                } @else if let Some(a) = loc.line_start {
                    span.mut.small {
                        @if loc.line_end.is_some_and(|b| b != a) {
                            "lines " (a) "-" (loc.line_end.unwrap_or(a))
                        } @else { "line " (a) }
                    }
                }
                @if resolved { span.pill { "resolved" } }
                span.mut.small { (short_time(&t.created_at)) }
            }
            @if earlier && let Some(q) = &loc.quoted_text { pre.quote { (q) } }
            div.msg { (t.body) }
            @for r in &t.replies {
                div.reply {
                    b { (r.author) } span.pill { (r.role) }
                    span.mut.small { (short_time(&r.created_at)) }
                    div.msg { (r.body) }
                }
            }
            form.replyf hx-post={ (base) "/threads/" (t.id) "/reply" }
                hx-target="#live" hx-swap="outerHTML" {
                textarea name="body" rows="2" required placeholder="Reply" {}
                div.row {
                    button type="submit" { "Reply" }
                    button type="button" hx-post={ (base) "/threads/" (t.id) "/status" }
                        hx-vals=(serde_json::json!({ "status": to }).to_string())
                        hx-target="#live" hx-swap="outerHTML" { (verb) }
                }
            }
        }
    }
}

// --- markdown -----------------------------------------------------------------------

/// A link target the UI will emit: relative, or http, https or mailto.
/// Whitespace and control characters are removed first because browsers
/// ignore them inside a scheme (`java\tscript:`).
fn safe_href(dest: &str) -> bool {
    let cleaned: String = dest
        .chars()
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .collect();
    match cleaned.find([':', '/', '?', '#']) {
        Some(i) if cleaned.as_bytes()[i] == b':' => {
            let scheme = cleaned[..i].to_ascii_lowercase();
            matches!(scheme.as_str(), "http" | "https" | "mailto")
        }
        _ => true,
    }
}

/// One top-level block of the document.
struct Block {
    html: String,
    /// Inclusive 1-based source line range.
    first: i64,
    last: i64,
    /// Index into `sections::parse`'s headings when this block is one.
    heading: Option<usize>,
}

/// Markdown to HTML, safe to embed, split into top-level blocks: raw HTML is
/// dropped, images become their alt text (Artifact embeds are rendered
/// elsewhere), links with other than http, https, mailto or relative targets
/// lose their `href`, and every link gets `rel="noopener noreferrer"`.
/// Top-level headings get `id="s<n>"` (1-based, in the order
/// `sections::parse` lists them) for the outline.
fn render_blocks(src: &str) -> Vec<Block> {
    let opts = Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH;
    let mut line_starts = vec![0usize];
    line_starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
    let line = |off: usize| line_starts.partition_point(|&s| s <= off) as i64;

    // Same container rule as `sections::parse`.
    let mut depth = 0usize;
    let mut headings = 0usize;
    let mut in_meta = false;
    // Nesting of every tag, to find where a top-level block ends.
    let mut nest = 0usize;
    let mut span = (0usize, 0usize);
    let mut heading = None;
    let mut cur: Vec<Event> = Vec::new();
    let mut blocks = Vec::new();
    for (ev, range) in Parser::new_ext(src, opts).into_offset_iter() {
        if nest == 0 {
            span = (range.start, range.end);
            heading = None;
        }
        let (starts, ends) = (matches!(ev, Event::Start(_)), matches!(ev, Event::End(_)));
        let out = match ev {
            Event::Html(_) | Event::InlineHtml(_) => None,
            Event::Start(Tag::MetadataBlock(_)) => {
                in_meta = true;
                None
            }
            Event::End(TagEnd::MetadataBlock(_)) => {
                in_meta = false;
                None
            }
            Event::Text(_) if in_meta => None,
            Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => None,
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                let esc = |s: &str| html! { (s) }.into_string();
                let mut a = String::from("<a");
                if safe_href(&dest_url) {
                    a.push_str(&format!(" href=\"{}\"", esc(&dest_url)));
                }
                if !title.is_empty() {
                    a.push_str(&format!(" title=\"{}\"", esc(&title)));
                }
                a.push_str(" rel=\"noopener noreferrer\">");
                Some(Event::Html(a.into()))
            }
            Event::Start(Tag::Heading { level, .. }) if depth == 0 => {
                headings += 1;
                heading = Some(headings - 1);
                Some(Event::Html(
                    format!("<h{} id=\"s{headings}\">", level as usize).into(),
                ))
            }
            ev @ Event::Start(
                Tag::BlockQuote(_) | Tag::List(_) | Tag::Item | Tag::FootnoteDefinition(_),
            ) => {
                depth += 1;
                Some(ev)
            }
            ev @ Event::End(
                TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item | TagEnd::FootnoteDefinition,
            ) => {
                depth = depth.saturating_sub(1);
                Some(ev)
            }
            ev => Some(ev),
        };
        cur.extend(out);
        nest += starts as usize;
        nest = nest.saturating_sub(ends as usize);
        if nest == 0 {
            let mut html = String::new();
            pulldown_cmark::html::push_html(&mut html, cur.drain(..));
            if !html.trim().is_empty() {
                blocks.push(Block {
                    html,
                    first: line(span.0),
                    last: line(span.1.saturating_sub(1).max(span.0)),
                    heading,
                });
            }
        }
    }
    blocks
}

/// The markdown of `src` as one HTML string (see [`render_blocks`]).
#[cfg(test)]
fn render_markdown(src: &str) -> String {
    render_blocks(src).into_iter().map(|b| b.html).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_html_is_dropped() {
        let out = render_markdown(
            "# T\n\n<script>alert(1)</script>\n\nhi <img src=x onerror=alert(1)> there\n\n<div onclick=\"x()\">\nblock\n</div>\n",
        );
        for bad in ["<script", "<img", "onerror", "<div", "onclick"] {
            assert!(!out.contains(bad), "{bad} in {out}");
        }
        assert!(out.contains("hi") && out.contains("there"));
    }

    #[test]
    fn unsafe_links_lose_their_href() {
        let out = render_markdown(
            "[a](javascript:alert(1)) [b](data:text/html,x) [c](JaVa\tScRiPt:x) [d](vbscript:x) \
             [e](java&#x73;cript:x) <javascript:alert(1)>",
        );
        // Whatever is left of them is escaped text, never an href.
        assert!(!out.to_lowercase().contains("href=\"javascript"), "{out}");
        assert!(!out.contains("href=\"data:"), "{out}");
        assert!(!out.contains("href=\"vbscript"), "{out}");
        assert_eq!(out.matches("href=").count(), 0, "{out}");
        assert!(
            out.contains("<a rel=\"noopener noreferrer\">a</a>"),
            "{out}"
        );
    }

    #[test]
    fn safe_links_keep_href_and_get_rel() {
        let out = render_markdown(
            "[x](https://example.com/?a=1&b=\"2\") [y](#s) [z](/a/b) [m](mailto:a@b.c)",
        );
        assert!(
            out.contains("<a href=\"https://example.com/?a=1&amp;b=&quot;2&quot;\" rel=\"noopener noreferrer\">x</a>"),
            "{out}"
        );
        for href in ["#s", "/a/b", "mailto:a@b.c"] {
            assert!(
                out.contains(&format!("href=\"{href}\" rel=\"noopener noreferrer\"")),
                "{out}"
            );
        }
    }

    #[test]
    fn images_show_alt_text_only() {
        let out = render_markdown(
            "![an <b>alt</b>](https://evil.example/x.png) ![](data:image/png;base64,AA)",
        );
        assert!(!out.contains("<img") && !out.contains("src="), "{out}");
        assert!(out.contains("an "), "{out}");
    }

    #[test]
    fn text_is_escaped_and_headings_get_ids() {
        let out = render_markdown("---\ntitle: x\n---\n\n# A & <B>\n\n> ## quoted\n\n## C\n");
        assert!(out.contains("<h1 id=\"s1\">A &amp; "), "{out}");
        assert!(out.contains("<h2 id=\"s2\">C</h2>"), "{out}");
        // Not a top-level heading: no id, matching `sections::parse`.
        assert!(out.contains("<h2>quoted</h2>"), "{out}");
        assert!(!out.contains("title: x"), "{out}");
    }

    #[test]
    fn section_ids_match_outline_order() {
        let src = "# A\n\n- ## in list\n\n## B\n\n### C\n";
        let out = render_markdown(src);
        let n = sections::parse(src).len();
        assert_eq!(n, 3);
        assert_eq!(out.matches(" id=\"s").count(), n, "{out}");
    }

    #[test]
    fn blocks_carry_line_spans_and_heading_indexes() {
        let src = "---\ntitle: x\n---\n\n# A\n\npara one\nline two\n\n```\ncode\n```\n\n## B\n";
        let b = render_blocks(src);
        let spans: Vec<_> = b.iter().map(|b| (b.first, b.last, b.heading)).collect();
        assert_eq!(
            spans,
            vec![
                (5, 5, Some(0)),
                (7, 8, None),
                (10, 12, None),
                (14, 14, Some(1))
            ],
            "{:?}",
            b.iter().map(|b| &b.html).collect::<Vec<_>>()
        );
    }

    #[test]
    fn app_js_pauses_polling_while_hidden() {
        assert!(APP_JS.contains("document.hidden") && APP_JS.contains("htmx:beforeRequest"));
    }

    #[test]
    fn short_times_and_sizes() {
        assert_eq!(short_time("2026-11-10T01:42:00.123Z"), "2026-11-10 01:42");
        assert_eq!(human_bytes(10), "10 B");
        assert_eq!(human_bytes(2048), "2.0 KiB");
    }

    #[test]
    fn stylesheet_collapses_below_800px() {
        assert!(APP_CSS.contains("@media(max-width:800px)"));
    }
}
