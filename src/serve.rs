//! `cairn serve`: two loopback listeners in one process.
//!
//! The UI port serves a placeholder page and never any Artifact bytes. The
//! content port serves every Agent-supplied byte at `/a/<id>/<rev>/<path>`,
//! pinned to a Revision, under a per-Kind CSP, `nosniff` and the stored
//! allowlisted Content-Type (ADR 0005). Every response from either port,
//! errors and 405s included, carries nosniff and a CSP.
//!
//! Mermaid is rendered on demand by ONE dedicated thread that owns the
//! thread-local engine (see `diagram`), fed through a channel. Pool threads
//! would each start their own engine. SQLite connections are not `Sync`, so
//! each request opens its own inside `spawn_blocking` (WAL makes this safe
//! alongside CLI writers).

use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::map_response;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tokio::sync::oneshot;

use crate::artifact;
use crate::db;
use crate::diagram::{self, Format};
use crate::error::Error;
use crate::kind::Kind;

const NOSNIFF: HeaderName = HeaderName::from_static("x-content-type-options");
const SCRIPTLESS_CSP: &str = "sandbox; default-src 'none'; style-src 'unsafe-inline'";
const UI_CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'self'";
/// Mermaid is served at this fixed path of its Revision; the source is not.
const MERMAID_PATH: &str = "diagram.svg";

/// The UI port and the content port, with defaults and clashes resolved.
pub fn resolve_ports(port: u16, content_port: Option<u16>) -> Result<(u16, u16), Error> {
    let content = match content_port {
        Some(p) => p,
        None => port.checked_add(1).ok_or_else(|| {
            Error::Usage(format!(
                "no default content port after {port}; pass --content-port"
            ))
        })?,
    };
    if content == port {
        return Err(Error::Usage("--port and --content-port must differ".into()));
    }
    Ok((port, content))
}

/// Bind 127.0.0.1 only. A bind failure names the port.
pub(crate) fn bind_loopback(port: u16) -> Result<TcpListener, Error> {
    let l = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            Error::Unexpected(format!(
                "port {port} is already in use; pass --port / --content-port"
            ))
        } else {
            Error::Unexpected(format!("cannot listen on 127.0.0.1:{port}: {e}"))
        }
    })?;
    l.set_nonblocking(true)
        .map_err(|e| Error::Unexpected(format!("cannot configure listener: {e}")))?;
    Ok(l)
}

enum RenderJob {
    Render(String, oneshot::Sender<Result<String, Error>>),
    /// Test probe: how many engines this thread has started.
    #[cfg(test)]
    Starts(oneshot::Sender<usize>),
}

/// Handle to the single Mermaid render thread.
#[derive(Clone)]
pub(crate) struct Renderer(mpsc::Sender<RenderJob>);

impl Renderer {
    /// Start the thread. It exits when every handle is dropped.
    pub(crate) fn start() -> Renderer {
        let (tx, rx) = mpsc::channel::<RenderJob>();
        std::thread::Builder::new()
            .name("cairn-render".into())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    match job {
                        RenderJob::Render(src, reply) => {
                            let _ = reply.send(diagram::render_mermaid(&src));
                        }
                        #[cfg(test)]
                        RenderJob::Starts(reply) => {
                            let _ = reply.send(diagram::engine_starts_on_thread());
                        }
                    }
                }
            })
            .expect("spawn render thread");
        Renderer(tx)
    }

    async fn render(&self, src: String) -> Result<String, Error> {
        let (tx, rx) = oneshot::channel();
        self.0
            .send(RenderJob::Render(src, tx))
            .map_err(|_| Error::Unexpected("render thread is gone".into()))?;
        rx.await
            .map_err(|_| Error::Unexpected("render thread dropped the job".into()))?
    }
}

#[cfg(test)]
impl Renderer {
    async fn starts(&self) -> usize {
        let (tx, rx) = oneshot::channel();
        self.0.send(RenderJob::Starts(tx)).unwrap();
        rx.await.unwrap()
    }
}

#[derive(Clone)]
struct ContentState {
    db_path: PathBuf,
    ui_port: u16,
    renderer: Renderer,
}

#[derive(Clone)]
struct UiState {
    reviewer: String,
}

/// Run both listeners until ctrl-c. Both ports are bound before anything
/// runs, so a taken port fails fast with a non-zero exit.
pub fn serve(db_path: &Path, ui_port: u16, content_port: u16, reviewer: &str) -> Result<u8, Error> {
    // Create and migrate the database up front so a bad path fails here.
    db::open(db_path)?;
    let ui_listener = bind_loopback(ui_port)?;
    let content_listener = bind_loopback(content_port)?;
    let ui = ui_router(reviewer);
    let content = content_router(db_path.to_path_buf(), ui_port, Renderer::start());
    println!(
        "{}",
        serde_json::json!({
            "ui": format!("http://127.0.0.1:{ui_port}"),
            "content": format!("http://127.0.0.1:{content_port}"),
            "reviewer": reviewer,
        })
    );
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Unexpected(format!("cannot start runtime: {e}")))?;
    rt.block_on(async move {
        let io = |e: std::io::Error| Error::Unexpected(format!("server error: {e}"));
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = stop_tx.send(true);
        });
        let shutdown = |mut rx: tokio::sync::watch::Receiver<bool>| async move {
            let _ = rx.wait_for(|stopped| *stopped).await;
        };
        let ui_l = tokio::net::TcpListener::from_std(ui_listener).map_err(io)?;
        let content_l = tokio::net::TcpListener::from_std(content_listener).map_err(io)?;
        let (a, b) = tokio::join!(
            axum::serve(ui_l, ui).with_graceful_shutdown(shutdown(stop_rx.clone())),
            axum::serve(content_l, content).with_graceful_shutdown(shutdown(stop_rx)),
        );
        a.map_err(io)?;
        b.map_err(io)?;
        Ok(0)
    })
}

/// Set nosniff on every response and a default CSP where none was set.
fn harden(mut res: Response, default_csp: &'static str) -> Response {
    let h = res.headers_mut();
    h.insert(NOSNIFF, HeaderValue::from_static("nosniff"));
    if !h.contains_key(header::CONTENT_SECURITY_POLICY) {
        h.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(default_csp),
        );
    }
    res
}

async fn harden_ui(res: Response) -> Response {
    harden(res, UI_CSP)
}

async fn harden_content(res: Response) -> Response {
    harden(res, SCRIPTLESS_CSP)
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "not found",
    )
        .into_response()
}

// --- UI port ----------------------------------------------------------------

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(crate) fn ui_router(reviewer: &str) -> Router {
    Router::new()
        .route("/", get(ui_index))
        .fallback(|| async { not_found() })
        .layer(map_response(harden_ui))
        .with_state(UiState {
            reviewer: reviewer.to_string(),
        })
}

async fn ui_index(State(s): State<UiState>) -> Response {
    let page = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>cairn</title>\
         <h1>cairn</h1><p>Reviewing as {}.</p>",
        escape_html(&s.reviewer)
    );
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], page).into_response()
}

// --- content port -------------------------------------------------------------

pub(crate) fn content_router(db_path: PathBuf, ui_port: u16, renderer: Renderer) -> Router {
    Router::new()
        .route("/a/{id}/{rev}/{*path}", get(content))
        .fallback(|| async { not_found() })
        .layer(map_response(harden_content))
        .with_state(ContentState {
            db_path,
            ui_port,
            renderer,
        })
}

/// What a request resolved to.
enum Found {
    /// A stored file served as is; `bundle` selects the html CSP.
    File {
        file: artifact::StoredFile,
        bundle: bool,
    },
    /// Mermaid source to render.
    Mermaid(String),
}

fn lookup(db_path: &Path, id: &str, rev: i64, path: &str) -> Result<Option<Found>, Error> {
    let conn = db::open(db_path)?;
    let Some(kind) = artifact::kind_of(&conn, id)? else {
        return Ok(None);
    };
    match kind {
        // The UI renders markdown; its bytes are never served raw.
        Kind::Markdown => Ok(None),
        Kind::Diagram if artifact::diagram_format(&conn, id)? == Format::Mermaid => {
            if path != MERMAID_PATH {
                return Ok(None);
            }
            match artifact::get(&conn, id, Some(rev)) {
                Ok(bytes) => Ok(String::from_utf8(bytes).ok().map(Found::Mermaid)),
                Err(Error::NotFound(_)) => Ok(None),
                Err(e) => Err(e),
            }
        }
        Kind::Html | Kind::Image | Kind::Diagram => Ok(artifact::get_file(&conn, id, rev, path)?
            .map(|file| Found::File {
                file,
                bundle: kind == Kind::Html,
            })),
    }
}

async fn content(
    State(s): State<ContentState>,
    UrlPath((id, rev, path)): UrlPath<(String, String, String)>,
) -> Response {
    let Some(rev) = rev.parse::<i64>().ok().filter(|r| *r >= 1) else {
        return not_found();
    };
    let db_path = s.db_path.clone();
    let found = tokio::task::spawn_blocking(move || lookup(&db_path, &id, rev, &path)).await;
    let found = match found {
        Ok(Ok(Some(f))) => f,
        Ok(Ok(None)) => return not_found(),
        // Detail stays out of the response: it could echo stored content.
        _ => return server_error(),
    };
    match found {
        Found::File { file, bundle } => {
            let csp = if bundle {
                html_csp(s.ui_port)
            } else {
                SCRIPTLESS_CSP.to_string()
            };
            typed(&file.media_type, &csp, file.bytes)
        }
        Found::Mermaid(src) => match s.renderer.render(src).await {
            Ok(svg) => typed("image/svg+xml", SCRIPTLESS_CSP, svg.into_bytes()),
            Err(_) => server_error(),
        },
    }
}

fn html_csp(ui_port: u16) -> String {
    format!(
        "sandbox allow-scripts; default-src 'none'; script-src 'self' 'unsafe-inline'; \
         style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; \
         connect-src 'none'; frame-ancestors http://127.0.0.1:{ui_port}"
    )
}

fn typed(media_type: &str, csp: &str, bytes: Vec<u8>) -> Response {
    let (Ok(ct), Ok(csp)) = (
        HeaderValue::from_str(media_type),
        HeaderValue::from_str(csp),
    ) else {
        return server_error();
    };
    let mut res = bytes.into_response();
    res.headers_mut().insert(header::CONTENT_TYPE, ct);
    res.headers_mut()
        .insert(header::CONTENT_SECURITY_POLICY, csp);
    res
}

fn server_error() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "server error",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::{Input, RawFile};
    use crate::cli::{Context, Role};
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    struct Fixture {
        _dir: tempfile::TempDir,
        db: PathBuf,
        content: Router,
        ui: Router,
    }

    fn ctx(db: &Path) -> Context {
        Context {
            role: Role::Agent,
            author: "a".into(),
            session: None,
            session_explicit: None,
            db_path: db.to_path_buf(),
            project: None,
        }
    }

    fn files(fs: &[(&str, &[u8])]) -> Input {
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
                content: content_router(db.clone(), 7878, Renderer::start()),
                ui: ui_router("rev"),
                db,
                _dir: dir,
            }
        }

        fn submit(&self, name: &str, kind: Kind, format: Option<Format>, input: Input) -> String {
            let mut conn = db::open(&self.db).unwrap();
            let r = artifact::submit(
                &mut conn,
                &ctx(&self.db),
                artifact::SubmitInput {
                    name: name.into(),
                    kind,
                    format,
                    input,
                    scope: None,
                },
            )
            .unwrap();
            r.id
        }

        fn edit(&self, id: &str, input: Input) {
            let mut conn = db::open(&self.db).unwrap();
            artifact::edit(&mut conn, &ctx(&self.db), id, input, None, &[]).unwrap();
        }
    }

    async fn get(router: &Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let res = router
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (parts, body) = res.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes().to_vec();
        (parts.status, parts.headers, bytes)
    }

    fn h<'a>(m: &'a axum::http::HeaderMap, k: &str) -> &'a str {
        m.get(k).unwrap().to_str().unwrap()
    }

    const PNG: &[u8] =
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89";
    const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"1\"/></svg>";

    #[test]
    fn ports_resolve() {
        assert_eq!(resolve_ports(7878, None).unwrap(), (7878, 7879));
        assert_eq!(resolve_ports(7878, Some(9000)).unwrap(), (7878, 9000));
        assert!(resolve_ports(65535, None).is_err());
        assert!(resolve_ports(7878, Some(7878)).is_err());
    }

    #[test]
    fn binds_loopback_only() {
        let l = bind_loopback(0).unwrap();
        assert_eq!(
            l.local_addr().unwrap().ip(),
            std::net::IpAddr::V4(Ipv4Addr::LOCALHOST)
        );
        let port = l.local_addr().unwrap().port();
        let e = bind_loopback(port).unwrap_err();
        assert!(e.to_string().contains(&port.to_string()), "{e}");
        assert_eq!(e.exit_code(), 1);
    }

    #[tokio::test]
    async fn html_bundle_headers_and_pinned_assets() {
        let f = Fixture::new();
        let id = f.submit(
            "b",
            Kind::Html,
            None,
            files(&[
                ("index.html", b"<h1>one</h1>"),
                ("style.css", b"a{}"),
                ("app.js", b"1"),
                ("p.png", PNG),
            ]),
        );
        let expect = "sandbox allow-scripts; default-src 'none'; script-src 'self' 'unsafe-inline'; \
            style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; \
            connect-src 'none'; frame-ancestors http://127.0.0.1:7878";
        for (path, ct) in [
            ("index.html", "text/html"),
            ("style.css", "text/css"),
            ("app.js", "text/javascript"),
            ("p.png", "image/png"),
        ] {
            let (st, hd, _) = get(&f.content, &format!("/a/{id}/1/{path}")).await;
            assert_eq!(st, StatusCode::OK, "{path}");
            assert_eq!(h(&hd, "content-security-policy"), expect);
            assert_eq!(h(&hd, "x-content-type-options"), "nosniff");
            assert!(h(&hd, "content-type").starts_with(ct), "{path}");
        }
        // Pinned: a new Revision does not change @1.
        f.edit(
            &id,
            files(&[("index.html", b"<h1>two</h1>"), ("style.css", b"a{}")]),
        );
        let (_, _, b1) = get(&f.content, &format!("/a/{id}/1/index.html")).await;
        let (_, _, b2) = get(&f.content, &format!("/a/{id}/2/index.html")).await;
        assert_eq!(b1, b"<h1>one</h1>");
        assert_eq!(b2, b"<h1>two</h1>");
        let (st, _, _) = get(&f.content, &format!("/a/{id}/2/p.png")).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn image_and_svg_are_scriptless() {
        let f = Fixture::new();
        let img = f.submit("i", Kind::Image, None, files(&[("p.png", PNG)]));
        let svg = f.submit(
            "d.svg",
            Kind::Diagram,
            Some(Format::Svg),
            files(&[("d.svg", SVG.as_bytes())]),
        );
        let csp = "sandbox; default-src 'none'; style-src 'unsafe-inline'";
        let (st, hd, body) = get(&f.content, &format!("/a/{img}/1/p.png")).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(h(&hd, "content-type"), "image/png");
        assert_eq!(h(&hd, "content-security-policy"), csp);
        assert_eq!(h(&hd, "x-content-type-options"), "nosniff");
        assert_eq!(body, PNG);
        let (st, hd, _) = get(&f.content, &format!("/a/{svg}/1/d.svg")).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(h(&hd, "content-type"), "image/svg+xml");
        assert_eq!(h(&hd, "content-security-policy"), csp);
    }

    #[tokio::test]
    #[cfg(not(cairn_pele_stub))]
    async fn mermaid_renders_on_demand_with_one_engine() {
        let f = Fixture::new();
        let renderer = Renderer::start();
        let content = content_router(f.db.clone(), 7878, renderer.clone());
        let id = f.submit(
            "m",
            Kind::Diagram,
            Some(Format::Mermaid),
            files(&[("m.mmd", b"flowchart LR\n A --> B\n")]),
        );
        for _ in 0..3 {
            let (st, hd, body) = get(&content, &format!("/a/{id}/1/diagram.svg")).await;
            assert_eq!(st, StatusCode::OK);
            assert_eq!(h(&hd, "content-type"), "image/svg+xml");
            assert_eq!(
                h(&hd, "content-security-policy"),
                "sandbox; default-src 'none'; style-src 'unsafe-inline'"
            );
            assert_eq!(h(&hd, "x-content-type-options"), "nosniff");
            let text = String::from_utf8(body).unwrap();
            assert!(text.trim_start().starts_with("<svg"));
            assert!(!text.contains("<script"));
        }
        assert_eq!(renderer.starts().await, 1);
        // The source is never served.
        let (st, _, body) = get(&f.content, &format!("/a/{id}/1/m.mmd")).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
        assert_eq!(body, b"not found");
    }

    #[tokio::test]
    async fn unknown_things_404_identically() {
        let f = Fixture::new();
        let md = f.submit(
            "n",
            Kind::Markdown,
            None,
            files(&[("n.md", b"# SECRET-MARKDOWN\n")]),
        );
        let id = f.submit(
            "b",
            Kind::Html,
            None,
            files(&[("index.html", b"SECRET-HTML"), ("x.js", b"1")]),
        );
        let uris = [
            "/a/nope/1/index.html".to_string(),
            format!("/a/{id}/9/index.html"),
            format!("/a/{id}/1/missing.html"),
            format!("/a/{id}/1/../index.html"),
            format!("/a/{id}/1/%2e%2e/index.html"),
            format!("/a/{id}/0/index.html"),
            format!("/a/{id}/latest/index.html"),
            format!("/a/{id}/-1/index.html"),
            format!("/a/{md}/1/n.md"),
            "/".to_string(),
            "/nothing".to_string(),
        ];
        for uri in uris {
            let (st, hd, body) = get(&f.content, &uri).await;
            assert_eq!(st, StatusCode::NOT_FOUND, "{uri}");
            assert_eq!(body, b"not found", "{uri}");
            assert_eq!(h(&hd, "x-content-type-options"), "nosniff");
            assert!(hd.contains_key("content-security-policy"));
        }
    }

    #[tokio::test]
    async fn other_methods_are_405_with_hardening_headers() {
        let f = Fixture::new();
        let res = f
            .content
            .clone()
            .oneshot(Request::post("/a/x/1/y").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(h(res.headers(), "x-content-type-options"), "nosniff");
        assert!(res.headers().contains_key("content-security-policy"));
    }

    #[tokio::test]
    async fn ui_port_never_serves_artifact_bytes() {
        let f = Fixture::new();
        let id = f.submit(
            "b",
            Kind::Html,
            None,
            files(&[("index.html", b"SECRET-HTML")]),
        );
        let (st, hd, body) = get(&f.ui, &format!("/a/{id}/1/index.html")).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
        assert!(!String::from_utf8_lossy(&body).contains("SECRET"));
        assert_eq!(h(&hd, "x-content-type-options"), "nosniff");
        let (st, hd, body) = get(&f.ui, "/").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(
            h(&hd, "content-security-policy"),
            "default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'self'"
        );
        assert!(String::from_utf8(body).unwrap().contains("rev"));
    }
}
