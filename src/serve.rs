//! `cairn serve`: two loopback listeners in one process.
//!
//! The UI port serves the review UI (`ui`) and never any Artifact bytes. The
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
use axum::extract::Request;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::{Next, from_fn, map_response};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tokio::sync::oneshot;

use crate::artifact;
use crate::db;
use crate::diagram::{self, Format};
use crate::error::Error;
use crate::kind::Kind;
use crate::ui;

const NOSNIFF: HeaderName = HeaderName::from_static("x-content-type-options");
const SCRIPTLESS_CSP: &str = "sandbox; default-src 'none'; style-src 'unsafe-inline'";
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

/// Run both listeners until ctrl-c. Both ports are bound before anything
/// runs, so a taken port fails fast with a non-zero exit.
pub fn serve(db_path: &Path, ui_port: u16, content_port: u16, reviewer: &str) -> Result<u8, Error> {
    // Create and migrate the database up front so a bad path fails here.
    db::open(db_path)?;
    let ui_listener = bind_loopback(ui_port)?;
    let content_listener = bind_loopback(content_port)?;
    let ui = ui_router(db_path.to_path_buf(), reviewer, content_port, ui_port);
    let content = content_router(
        db_path.to_path_buf(),
        ui_port,
        content_port,
        Renderer::start(),
    );
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
fn harden(mut res: Response, default_csp: HeaderValue) -> Response {
    let h = res.headers_mut();
    h.insert(NOSNIFF, HeaderValue::from_static("nosniff"));
    if !h.contains_key(header::CONTENT_SECURITY_POLICY) {
        h.insert(header::CONTENT_SECURITY_POLICY, default_csp);
    }
    res
}

async fn harden_content(res: Response) -> Response {
    harden(res, HeaderValue::from_static(SCRIPTLESS_CSP))
}

use crate::ui::not_found;

// --- UI port ----------------------------------------------------------------

/// True when `host` is exactly a loopback name for this listener's own port.
/// Exact match only: a rebound attacker name (`evil.example:port`) or a
/// lookalike (`localhost.evil.example:port`) never passes.
fn host_allowed(host: &str, port: u16) -> bool {
    let host = host.to_ascii_lowercase();
    [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
    ]
    .contains(&host)
}

/// Outermost layer on both listeners. Defeats DNS rebinding: a page on
/// `evil.example` re-resolved to 127.0.0.1 still sends `Host: evil.example`,
/// so it is refused before routing, reads included. Missing Host is refused.
async fn require_loopback_host(port: u16, req: Request, next: Next) -> Response {
    let ok = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| host_allowed(h, port));
    if ok {
        next.run(req).await
    } else {
        harden(
            (StatusCode::FORBIDDEN, "forbidden").into_response(),
            HeaderValue::from_static(SCRIPTLESS_CSP),
        )
    }
}

pub(crate) fn ui_router(
    db_path: PathBuf,
    reviewer: &str,
    content_port: u16,
    ui_port: u16,
) -> Router {
    let csp = ui::csp(content_port);
    ui::router(ui::UiState {
        db_path,
        reviewer: reviewer.to_string(),
    })
    .fallback(|| async { not_found() })
    .layer(map_response(move |res: Response| {
        let csp = csp.clone();
        async move { harden(res, csp) }
    }))
    .layer(from_fn(move |req, next| {
        require_loopback_host(ui_port, req, next)
    }))
}

// --- content port -------------------------------------------------------------

pub(crate) fn content_router(
    db_path: PathBuf,
    ui_port: u16,
    content_port: u16,
    renderer: Renderer,
) -> Router {
    Router::new()
        .route("/a/{id}/{rev}/{*path}", get(content))
        .fallback(|| async { not_found() })
        .layer(map_response(harden_content))
        .layer(from_fn(move |req, next| {
            require_loopback_host(content_port, req, next)
        }))
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
                content: content_router(db.clone(), 7879, 7878, Renderer::start()),
                ui: ui_router(db.clone(), "rev", 7878, 7879),
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

    const UI_HOST: &str = "127.0.0.1:7879";
    const CONTENT_HOST: &str = "127.0.0.1:7878";

    async fn get(
        router: &Router,
        host: &str,
        uri: &str,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let res = router
            .clone()
            .oneshot(
                Request::get(uri)
                    .header("host", host)
                    .body(Body::empty())
                    .unwrap(),
            )
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
            connect-src 'none'; frame-ancestors http://127.0.0.1:7879";
        for (path, ct) in [
            ("index.html", "text/html"),
            ("style.css", "text/css"),
            ("app.js", "text/javascript"),
            ("p.png", "image/png"),
        ] {
            let (st, hd, _) = get(&f.content, CONTENT_HOST, &format!("/a/{id}/1/{path}")).await;
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
        let (_, _, b1) = get(&f.content, CONTENT_HOST, &format!("/a/{id}/1/index.html")).await;
        let (_, _, b2) = get(&f.content, CONTENT_HOST, &format!("/a/{id}/2/index.html")).await;
        assert_eq!(b1, b"<h1>one</h1>");
        assert_eq!(b2, b"<h1>two</h1>");
        let (st, _, _) = get(&f.content, CONTENT_HOST, &format!("/a/{id}/2/p.png")).await;
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
        let (st, hd, body) = get(&f.content, CONTENT_HOST, &format!("/a/{img}/1/p.png")).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(h(&hd, "content-type"), "image/png");
        assert_eq!(h(&hd, "content-security-policy"), csp);
        assert_eq!(h(&hd, "x-content-type-options"), "nosniff");
        assert_eq!(body, PNG);
        let (st, hd, _) = get(&f.content, CONTENT_HOST, &format!("/a/{svg}/1/d.svg")).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(h(&hd, "content-type"), "image/svg+xml");
        assert_eq!(h(&hd, "content-security-policy"), csp);
    }

    #[tokio::test]
    #[cfg(not(cairn_pele_stub))]
    async fn mermaid_renders_on_demand_with_one_engine() {
        let f = Fixture::new();
        let renderer = Renderer::start();
        let content = content_router(f.db.clone(), 7879, 7878, renderer.clone());
        let id = f.submit(
            "m",
            Kind::Diagram,
            Some(Format::Mermaid),
            files(&[("m.mmd", b"flowchart LR\n A --> B\n")]),
        );
        for _ in 0..3 {
            let (st, hd, body) =
                get(&content, CONTENT_HOST, &format!("/a/{id}/1/diagram.svg")).await;
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
        let (st, _, body) = get(&f.content, CONTENT_HOST, &format!("/a/{id}/1/m.mmd")).await;
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
            let (st, hd, body) = get(&f.content, CONTENT_HOST, &uri).await;
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
            .oneshot(
                Request::post("/a/x/1/y")
                    .header("host", CONTENT_HOST)
                    .body(Body::empty())
                    .unwrap(),
            )
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
        let (st, hd, body) = get(&f.ui, UI_HOST, &format!("/a/{id}/1/index.html")).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
        assert!(!String::from_utf8_lossy(&body).contains("SECRET"));
        assert_eq!(h(&hd, "x-content-type-options"), "nosniff");
        let (st, hd, body) = get(&f.ui, UI_HOST, "/").await;
        assert_eq!(st, StatusCode::OK);
        assert!(h(&hd, "content-security-policy").contains("frame-ancestors 'self'"));
        assert!(String::from_utf8(body).unwrap().contains("rev"));
    }

    fn text(b: &[u8]) -> String {
        String::from_utf8(b.to_vec()).unwrap()
    }

    fn md(f: &Fixture, name: &str, body: &str) -> String {
        f.submit(
            name,
            Kind::Markdown,
            None,
            files(&[("a.md", body.as_bytes())]),
        )
    }

    fn comment(f: &Fixture, id: &str, body: &str, loc: crate::comments::Locator) {
        let mut conn = db::open(&f.db).unwrap();
        crate::comments::add(&mut conn, &ctx(&f.db), id, body, &loc, None).unwrap();
    }

    #[tokio::test]
    async fn list_shows_rows_and_filters() {
        let f = Fixture::new();
        let a = md(&f, "plan <b>&\"'", "# T\n");
        let img = f.submit("pic", Kind::Image, None, files(&[("p.png", PNG)]));
        // Newest activity first.
        let (st, _, body) = get(&f.ui, UI_HOST, "/").await;
        assert_eq!(st, StatusCode::OK);
        let body = text(&body);
        assert!(body.contains(&format!("href=\"/a/{a}\"")), "{body}");
        assert!(body.contains(&format!("href=\"/a/{img}\"")));
        assert!(body.find(&img).unwrap() < body.find(&a).unwrap());
        // Names are escaped, never raw.
        assert!(
            body.contains("plan &lt;b&gt;&amp;&quot;'")
                || body.contains("plan &lt;b&gt;&amp;&quot;&#39;"),
            "{body}"
        );
        assert!(!body.contains("<b>&"));
        let (_, _, body) = get(&f.ui, UI_HOST, "/?kind=image").await;
        let body = text(&body);
        assert!(body.contains(&img) && !body.contains(&format!("/a/{a}\"")));
        let (_, _, body) = get(&f.ui, UI_HOST, "/?kind=nope").await;
        assert_eq!(text(&body), "bad kind");
        // htmx asks for the table only.
        let res =
            f.ui.clone()
                .oneshot(
                    Request::get("/?kind=image")
                        .header("host", UI_HOST)
                        .header("hx-target", "artifacts")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
        let frag = text(&res.into_body().collect().await.unwrap().to_bytes());
        assert!(
            frag.starts_with("<div id=\"artifacts\"") && !frag.contains("<html"),
            "{frag}"
        );
    }

    #[tokio::test]
    async fn list_filters_by_project() {
        let f = Fixture::new();
        let mut conn = db::open(&f.db).unwrap();
        for (name, project) in [("one", "alpha"), ("two", "beta")] {
            artifact::submit(
                &mut conn,
                &ctx(&f.db),
                artifact::SubmitInput {
                    name: name.into(),
                    kind: Kind::Markdown,
                    format: None,
                    input: files(&[("a.md", b"# x\n")]),
                    scope: Some(crate::scope::Scope {
                        name: project.into(),
                        root: format!("/r/{project}"),
                        worktree: None,
                    }),
                },
            )
            .unwrap();
        }
        let (_, _, body) = get(&f.ui, UI_HOST, "/?project=alpha").await;
        let body = text(&body);
        assert!(
            body.contains(">one</a>") && !body.contains(">two</a>"),
            "{body}"
        );
        // The dropdown still offers every project.
        assert!(body.contains("value=\"beta\""));
        let (_, _, body) = get(&f.ui, UI_HOST, "/").await;
        let body = text(&body);
        assert!(body.contains(">one</a>") && body.contains(">two</a>"));
    }

    #[tokio::test]
    async fn markdown_page_revisions_banner_and_outline() {
        let f = Fixture::new();
        let id = md(&f, "doc", "# Goal\n\none\n\n## Design\n\ntwo\n");
        f.edit(
            &id,
            files(&[("a.md", b"# Goal\n\nONE\n\n## Design\n\ntwo\n")]),
        );
        comment(
            &f,
            &id,
            "on design",
            crate::comments::Locator {
                lines: None,
                section: Some("Goal > Design".into()),
            },
        );
        comment(
            &f,
            &id,
            "on line",
            crate::comments::Locator {
                lines: Some((3, 3)),
                section: None,
            },
        );
        let (st, _, latest) = get(&f.ui, UI_HOST, &format!("/a/{id}")).await;
        assert_eq!(st, StatusCode::OK);
        let latest = text(&latest);
        assert!(
            latest.contains("ONE") && !latest.contains("Go to latest"),
            "{latest}"
        );
        // Dropdown lists every Revision.
        assert!(
            latest.contains("value=\"1\"") && latest.contains("value=\"2\" selected"),
            "{latest}"
        );
        // Outline with per-Section open Thread counts.
        assert!(latest.contains("href=\"#s1\"") && latest.contains("href=\"#s2\""));
        assert_eq!(latest.matches("pill open").count(), 2, "{latest}");
        let (_, _, same) = get(&f.ui, UI_HOST, &format!("/a/{id}@2")).await;
        assert!(text(&same).contains("ONE"));
        let (_, _, old) = get(&f.ui, UI_HOST, &format!("/a/{id}@1")).await;
        let old = text(&old);
        assert!(old.contains("<p>one</p>") && !old.contains("<p>ONE</p>"));
        assert!(
            old.contains(&format!("href=\"/a/{id}@2\"")) && old.contains("not the latest"),
            "{old}"
        );
        // The no-JS dropdown form redirects to the canonical URL.
        let (st, hd, _) = get(&f.ui, UI_HOST, &format!("/a/{id}?rev=1")).await;
        assert_eq!(st, StatusCode::SEE_OTHER);
        assert_eq!(h(&hd, "location"), format!("/a/{id}@1"));
        for bad in [
            format!("/a/{id}@3"),
            format!("/a/{id}@0"),
            "/a/nope".into(),
            format!("/a/{id}?rev=9x"),
        ] {
            let (st, _, body) = get(&f.ui, UI_HOST, &bad).await;
            assert_eq!(st, StatusCode::NOT_FOUND, "{bad}");
            assert_eq!(body, b"not found");
        }
    }

    #[tokio::test]
    async fn hostile_markdown_renders_inert_and_names_are_escaped() {
        let f = Fixture::new();
        let id = md(
            &f,
            "<script>alert('n')</script>",
            "# <b>h</b>\n\n<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n\
             [j](javascript:alert(1)) [d](data:text/html,x) [e](https://example.com)\n",
        );
        let mut conn = db::open(&f.db).unwrap();
        let mut c = ctx(&f.db);
        c.author = "<i>eve</i>".into();
        artifact::edit(&mut conn, &c, &id, files(&[("a.md", b"# x\n")]), None, &[]).unwrap();
        for uri in [format!("/a/{id}@1"), format!("/a/{id}@2"), "/".to_string()] {
            let (_, _, body) = get(&f.ui, UI_HOST, &uri).await;
            let body = text(&body);
            assert!(!body.contains("<script>alert"), "{uri}: {body}");
            assert!(!body.contains("<img") && !body.contains("onerror"), "{uri}");
            assert!(
                !body.contains("javascript:") && !body.contains("data:text"),
                "{uri}"
            );
            assert!(!body.contains("<i>eve"), "{uri}: {body}");
            assert!(!body.contains("<b>h"), "{uri}");
        }
        let (_, _, body) = get(&f.ui, UI_HOST, &format!("/a/{id}@1")).await;
        let body = text(&body);
        assert!(
            body.contains("&lt;script&gt;alert(&#39;n&#39;)&lt;/script&gt;")
                || body.contains("&lt;script&gt;alert('n')&lt;/script&gt;"),
            "{body}"
        );
        assert!(body.contains("href=\"https://example.com\" rel=\"noopener noreferrer\""));
    }

    #[tokio::test]
    async fn ui_csp_is_strict_on_every_page() {
        let f = Fixture::new();
        let id = md(&f, "d", "# x\n");
        for uri in ["/", &format!("/a/{id}"), "/nope", "/static/zzz/app.css"] {
            let (_, hd, _) = get(&f.ui, UI_HOST, uri).await;
            let csp = h(&hd, "content-security-policy");
            assert!(csp.contains("frame-ancestors 'self'"), "{uri}: {csp}");
            assert!(
                csp.contains("script-src 'self';") && csp.contains("style-src 'self';"),
                "{csp}"
            );
            assert!(
                !csp.contains("unsafe-inline") && !csp.contains("unsafe-eval"),
                "{csp}"
            );
            assert_eq!(h(&hd, "x-content-type-options"), "nosniff");
        }
    }

    #[tokio::test]
    async fn static_assets_are_versioned_and_cached() {
        let f = Fixture::new();
        let (_, _, page) = get(&f.ui, UI_HOST, "/").await;
        let page = text(&page);
        for (ext, ct) in [
            ("htmx.js", "text/javascript"),
            ("app.js", "text/javascript"),
            ("app.css", "text/css"),
        ] {
            let start = page.find("/static/").unwrap_or_else(|| panic!("{page}"));
            let _ = start;
            let href = page
                .split('"')
                .find(|p| p.starts_with("/static/") && p.ends_with(ext))
                .unwrap_or_else(|| panic!("no {ext} in {page}"))
                .to_string();
            let (st, hd, body) = get(&f.ui, UI_HOST, &href).await;
            assert_eq!(st, StatusCode::OK, "{href}");
            assert!(h(&hd, "content-type").starts_with(ct));
            assert!(h(&hd, "cache-control").contains("immutable"));
            assert!(!body.is_empty());
            let stale = href.replace("/static/", "/static/x");
            assert_eq!(get(&f.ui, UI_HOST, &stale).await.0, StatusCode::NOT_FOUND);
        }
    }

    async fn post(
        router: &Router,
        host: &str,
        uri: &str,
        form: &str,
        htmx: bool,
    ) -> (StatusCode, String) {
        let mut req = Request::post(uri)
            .header("host", host)
            .header("content-type", "application/x-www-form-urlencoded");
        if htmx {
            req = req.header("hx-request", "true");
        }
        let res = router
            .clone()
            .oneshot(req.body(Body::from(form.to_string())).unwrap())
            .await
            .unwrap();
        let (parts, body) = res.into_parts();
        (
            parts.status,
            text(&body.collect().await.unwrap().to_bytes()),
        )
    }

    fn threads(f: &Fixture, id: &str) -> Vec<crate::comments::Thread> {
        let conn = db::open(&f.db).unwrap();
        crate::comments::list(
            &conn,
            &ctx(&f.db),
            &crate::comments::ListQuery {
                artifact: Some(id.to_string()),
                all: true,
                since: None,
                scope: crate::scope::ScopeFilter::unscoped(),
            },
        )
        .unwrap()
        .threads
    }

    #[tokio::test]
    async fn reviewer_posts_replies_resolves_and_reopens() {
        let f = Fixture::new();
        let id = md(&f, "doc", "# Goal\n\none\n\n## Design\n\ntwo\n");
        // Gutter buttons and the per-heading button are on the page.
        let (_, _, page) = get(&f.ui, UI_HOST, &format!("/a/{id}")).await;
        let page = text(&page);
        assert!(
            page.contains(&format!("hx-get=\"/a/{id}@1/compose?line=3\"")),
            "{page}"
        );
        assert!(
            page.contains("+ section") && page.contains("Goal &gt; Design"),
            "{page}"
        );
        let (st, _, form) = get(&f.ui, UI_HOST, &format!("/a/{id}@1/compose?line=3")).await;
        assert_eq!(st, StatusCode::OK);
        assert!(text(&form).contains("name=\"line\" value=\"3\""));
        let (st, _, _) = get(&f.ui, UI_HOST, &format!("/a/{id}@1/compose")).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);

        // Writes need htmx's header, and a same-origin Origin if one is sent.
        let new = format!("/a/{id}@1/threads");
        assert_eq!(
            post(&f.ui, UI_HOST, &new, "body=x&line=3", false).await.0,
            StatusCode::FORBIDDEN
        );
        let res =
            f.ui.clone()
                .oneshot(
                    Request::post(&new)
                        .header("content-type", "application/x-www-form-urlencoded")
                        .header("hx-request", "true")
                        .header("host", UI_HOST)
                        .header("origin", "http://evil.example")
                        .body(Body::from("body=x&line=3"))
                        .unwrap(),
                )
                .await
                .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(threads(&f, &id).is_empty());

        // New Thread on a line: shown under its block, attributed to the reviewer.
        let (st, frag) = post(
            &f.ui,
            UI_HOST,
            &new,
            "body=%3Cb%3Ehello%3C%2Fb%3E&line=3",
            true,
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{frag}");
        assert!(frag.starts_with("<div id=\"live\""), "{frag}");
        assert!(frag.contains("&lt;b&gt;hello&lt;/b&gt;") && !frag.contains("<b>hello"));
        let t = threads(&f, &id);
        assert_eq!(t.len(), 1);
        assert_eq!(
            (t[0].author.as_str(), t[0].role.as_str()),
            ("rev", "reviewer")
        );
        assert_eq!(t[0].locator.line_start, Some(3));
        let tid = t[0].id;
        // Section Thread.
        let (st, _) = post(
            &f.ui,
            UI_HOST,
            &new,
            "body=sec&section=Goal+%3E+Design",
            true,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(
            threads(&f, &id)[1].locator.section_path.as_deref(),
            Some("Goal > Design")
        );
        // Reply, resolve, reopen.
        let base = format!("/a/{id}@1/threads/{tid}");
        let (st, _) = post(&f.ui, UI_HOST, &format!("{base}/reply"), "body=again", true).await;
        assert_eq!(st, StatusCode::OK);
        let t = threads(&f, &id);
        assert_eq!(t[0].replies.len(), 1);
        assert_eq!(t[0].replies[0].author, "rev");
        assert_eq!(t[0].replies[0].role, "reviewer");
        let (st, frag) = post(
            &f.ui,
            UI_HOST,
            &format!("{base}/status"),
            "status=resolved",
            true,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(threads(&f, &id)[0].status, "resolved");
        assert!(frag.contains("Reopen"), "{frag}");
        let (_, _) = post(
            &f.ui,
            UI_HOST,
            &format!("{base}/status"),
            "status=open",
            true,
        )
        .await;
        assert_eq!(threads(&f, &id)[0].status, "open");
        // Bad input is a fixed 4xx and writes nothing.
        for (uri, form, want) in [
            (
                format!("{base}/status"),
                "status=bogus",
                StatusCode::BAD_REQUEST,
            ),
            (new.clone(), "body=+&line=3", StatusCode::BAD_REQUEST),
            (new.clone(), "body=x&line=99", StatusCode::BAD_REQUEST),
            (
                format!("/a/{id}@1/threads/999/status"),
                "status=open",
                StatusCode::NOT_FOUND,
            ),
            (
                format!("/a/{id}@7/threads"),
                "body=x",
                StatusCode::NOT_FOUND,
            ),
        ] {
            let (st, body) = post(&f.ui, UI_HOST, &uri, form, true).await;
            assert_eq!(st, want, "{uri} {body}");
            assert!(body == "bad request" || body == "not found", "{body}");
        }
        assert_eq!(threads(&f, &id).len(), 2);
        // A Thread of another Artifact cannot be resolved through this one.
        let other = md(&f, "other", "# O\n");
        let (st, _) = post(
            &f.ui,
            UI_HOST,
            &format!("/a/{other}/threads/{tid}/status"),
            "status=resolved",
            true,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert_eq!(threads(&f, &id)[0].status, "open");
    }

    #[tokio::test]
    async fn live_poll_sees_cli_comments_and_resolves() {
        let f = Fixture::new();
        let id = md(&f, "doc", "# Goal\n\none\n");
        let (_, _, page) = get(&f.ui, UI_HOST, &format!("/a/{id}")).await;
        let page = text(&page);
        assert!(page.contains("data-poll") && page.contains("hx-trigger=\"every 2s\""));
        let marker = page
            .split("/live?m=")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .unwrap()
            .to_string();
        let live = format!("/a/{id}@1/live?m={marker}");
        assert_eq!(get(&f.ui, UI_HOST, &live).await.0, StatusCode::NO_CONTENT);
        // A Comment posted from elsewhere changes the marker and shows up.
        comment(
            &f,
            &id,
            "from the CLI",
            crate::comments::Locator {
                lines: Some((3, 3)),
                section: None,
            },
        );
        let (st, _, frag) = get(&f.ui, UI_HOST, &live).await;
        assert_eq!(st, StatusCode::OK);
        let frag = text(&frag);
        assert!(frag.starts_with("<div id=\"live\"") && frag.contains("from the CLI"));
        assert_eq!(frag.matches("pill open").count(), 1, "{frag}");
        // A resolve adds no Comment but still changes the marker.
        let marker = frag
            .split("/live?m=")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .unwrap()
            .to_string();
        let live = format!("/a/{id}@1/live?m={marker}");
        assert_eq!(get(&f.ui, UI_HOST, &live).await.0, StatusCode::NO_CONTENT);
        let tid = threads(&f, &id)[0].id;
        crate::comments::set_status(&mut db::open(&f.db).unwrap(), tid, "resolved").unwrap();
        let (st, _, frag) = get(&f.ui, UI_HOST, &live).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(text(&frag).matches("pill open").count(), 0);
        assert_eq!(
            get(&f.ui, UI_HOST, &format!("/a/{id}@9/live")).await.0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn earlier_revision_threads_are_dashed_and_resolved_ones_hidden() {
        let f = Fixture::new();
        let id = md(&f, "doc", "# Goal\n\none\n\n## Design\n\ntwo\n");
        let loc = |s: &str| crate::comments::Locator {
            lines: None,
            section: Some(s.into()),
        };
        comment(&f, &id, "open-on-r1", loc("Goal > Design"));
        comment(&f, &id, "resolved-on-r1", loc("Goal"));
        comment(
            &f,
            &id,
            "line-on-r1",
            crate::comments::Locator {
                lines: Some((3, 3)),
                section: None,
            },
        );
        let resolved = threads(&f, &id)[1].id;
        crate::comments::set_status(&mut db::open(&f.db).unwrap(), resolved, "resolved").unwrap();
        f.edit(
            &id,
            files(&[("a.md", b"# Goal\n\nONE\n\n## Design\n\ntwo\n")]),
        );
        let (_, _, r2) = get(&f.ui, UI_HOST, &format!("/a/{id}")).await;
        let r2 = text(&r2);
        assert!(
            r2.contains("open-on-r1") && r2.contains("line-on-r1"),
            "{r2}"
        );
        assert!(!r2.contains("resolved-on-r1"), "{r2}");
        assert_eq!(r2.matches("earlier revision").count(), 2, "{r2}");
        assert_eq!(r2.matches("class=\"thread earlier\"").count(), 2, "{r2}");
        // Section Threads sit in the Section; the outline counts them.
        assert!(r2.find("Design</h2>").unwrap() < r2.find("open-on-r1").unwrap());
        assert_eq!(r2.matches("pill open").count(), 1, "{r2}");
        // On its own Revision the resolved Thread shows, not dashed.
        let (_, _, r1) = get(&f.ui, UI_HOST, &format!("/a/{id}@1")).await;
        let r1 = text(&r1);
        assert!(r1.contains("resolved-on-r1"), "{r1}");
        assert!(!r1.contains("earlier revision"), "{r1}");
        assert!(r1.contains("class=\"thread resolved\""), "{r1}");
        // The list counts open Threads only.
        let (_, _, list) = get(&f.ui, UI_HOST, "/").await;
        assert!(text(&list).contains("<span class=\"pill open\">2</span>"));
    }

    #[tokio::test]
    async fn non_markdown_kind_gets_a_placeholder_page() {
        let f = Fixture::new();
        let id = f.submit("pic", Kind::Image, None, files(&[("p.png", PNG)]));
        let (st, _, body) = get(&f.ui, UI_HOST, &format!("/a/{id}")).await;
        assert_eq!(st, StatusCode::OK);
        assert!(text(&body).contains("not shown in the UI yet"));
    }
    #[tokio::test]
    async fn host_allowlist_on_both_routers() {
        let f = Fixture::new();
        for (router, port, path) in [(&f.ui, 7879, "/"), (&f.content, 7878, "/a/x/1/y")] {
            for ok in [
                format!("127.0.0.1:{port}"),
                format!("localhost:{port}"),
                format!("LocalHost:{port}"),
                format!("[::1]:{port}"),
            ] {
                let (st, _, _) = get(router, &ok, path).await;
                assert_ne!(st, StatusCode::FORBIDDEN, "{ok}");
            }
            for bad in [
                format!("evil.example:{port}"),
                "evil.example".to_string(),
                "127.0.0.1:1".to_string(),
                format!("localhost.evil.example:{port}"),
                format!("127.0.0.1:{port}.evil.example"),
            ] {
                let (st, hd, _) = get(router, &bad, path).await;
                assert_eq!(st, StatusCode::FORBIDDEN, "{bad}");
                assert_eq!(h(&hd, "x-content-type-options"), "nosniff");
            }
            let res = router
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::FORBIDDEN, "missing host");
        }
    }

    #[tokio::test]
    async fn dns_rebinding_post_is_refused_and_writes_nothing() {
        let f = Fixture::new();
        let id = md(&f, "doc", "one\n\ntwo\n");
        let res =
            f.ui.clone()
                .oneshot(
                    Request::post(format!("/a/{id}@1/threads"))
                        .header("content-type", "application/x-www-form-urlencoded")
                        .header("hx-request", "true")
                        .header("host", "evil.example:7879")
                        .header("origin", "http://evil.example:7879")
                        .body(Body::from("body=x&line=1"))
                        .unwrap(),
                )
                .await
                .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(threads(&f, &id).is_empty());
    }
}
