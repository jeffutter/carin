//! The review UI on the UI port: server-rendered with maud, enhanced by htmx.
//!
//! Handlers call core in-process. SQLite connections are not `Sync`, so each
//! request opens its own inside `spawn_blocking`. Everything an Agent supplied
//! (names, authors, Section titles, markdown) is untrusted: text goes through
//! maud's escaping, and markdown is rendered by [`render_markdown`], which
//! drops raw HTML and unsafe link schemes. The page CSP ([`csp`]) is the
//! second line of defence; it forbids inline script and style.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use axum::Router;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
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
/// More rows than any one reviewer scrolls; the list has no paging yet.
const LIST_LIMIT: i64 = 10_000;

/// Short content hash: the asset URL changes exactly when the bytes do, so
/// the assets can be cached forever.
fn short_hash(s: &str) -> String {
    artifact::hex(&Sha256::digest(s.as_bytes()))[..12].to_string()
}

static HTMX_HASH: LazyLock<String> = LazyLock::new(|| short_hash(HTMX_JS));
static CSS_HASH: LazyLock<String> = LazyLock::new(|| short_hash(APP_CSS));

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
        .route("/static/{hash}/htmx.js", get(htmx_js))
        .route("/static/{hash}/app.css", get(app_css))
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

/// Run `f` against a fresh connection off the async threads. `None` is a 404;
/// an error is a fixed 500 because its text could echo stored content.
async fn blocking<T, F>(db_path: &Path, f: F) -> Result<T, (StatusCode, &'static str)>
where
    T: Send + 'static,
    F: FnOnce(&Connection) -> Result<Option<T>, Error> + Send + 'static,
{
    let db_path = db_path.to_path_buf();
    let r = tokio::task::spawn_blocking(move || f(&db::open(&db_path)?)).await;
    match r {
        Ok(Ok(Some(v))) => Ok(v),
        Ok(Ok(None)) => Err((StatusCode::NOT_FOUND, "not found")),
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
        div #artifacts {
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
        html: String,
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
    body: Body,
}

fn reviewer_ctx(db_path: &Path) -> Context {
    Context {
        role: Role::Reviewer,
        author: "reviewer".into(),
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

fn load_page(
    db_path: &Path,
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
    let body = if kind == Kind::Markdown {
        let text = artifact::markdown_text(conn, id, Some(viewing))?;
        let list = comments::list(
            conn,
            &reviewer_ctx(db_path),
            &comments::ListQuery {
                artifact: Some(id.to_string()),
                all: false,
                since: None,
                scope: ScopeFilter::unscoped(),
            },
        )?;
        let counts = thread_counts(&text, viewing, &list.threads);
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
            html: render_markdown(&text),
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
    let db_path = s.db_path.clone();
    let page = match blocking(&s.db_path, move |c| load_page(&db_path, c, &id, rev)).await {
        Ok(p) => p,
        Err(r) => return r.into_response(),
    };
    html_response(layout(&s, &page.name, page_markup(&page)))
}

fn page_markup(p: &Page) -> Markup {
    let kind_label = match &p.format {
        Some(f) => format!("{}/{f}", p.kind.as_str()),
        None => p.kind.as_str().to_string(),
    };
    let here = format!("/a/{}", p.id);
    html! {
        div.wrap #page {
            @if p.viewing != p.latest {
                div.banner {
                    "Viewing r" (p.viewing) ", not the latest (r" (p.latest) "). "
                    a.latest href={ (here) "@" (p.latest) } { "Go to latest" }
                }
            }
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
            @match &p.body {
                Body::Markdown { html: body, outline } => {
                    div.lay {
                        nav.side aria-label="Outline" {
                            @for (i, e) in outline.iter().enumerate() {
                                a class={ "lvl" (e.level) } href={ "#s" (i + 1) } {
                                    span { (e.text) }
                                    @if e.open_threads > 0 { span.pill.open { (e.open_threads) } }
                                }
                            }
                        }
                        article.doc { (PreEscaped(body)) }
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

/// Markdown to HTML, safe to embed: raw HTML is dropped, images become their
/// alt text (Artifact embeds are rendered elsewhere), links with other than
/// http, https, mailto or relative targets lose their `href`, and every link
/// gets `rel="noopener noreferrer"`. Top-level headings get `id="s<n>"`
/// (1-based, in the order `sections::parse` lists them) for the outline.
pub(crate) fn render_markdown(src: &str) -> String {
    let opts = Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH;
    // Same container rule as `sections::parse`.
    let mut depth = 0usize;
    let mut headings = 0usize;
    let mut in_meta = false;
    let events = Parser::new_ext(src, opts).filter_map(|ev| match ev {
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
    });
    let mut out = String::new();
    pulldown_cmark::html::push_html(&mut out, events);
    out
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
