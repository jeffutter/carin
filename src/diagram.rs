//! Diagram Artifacts: the `mermaid` and `svg` formats.
//!
//! Mermaid source is validated, and rendered to SVG on demand, by Pele
//! (a JavaScript renderer) inside an embedded QuickJS engine. The engine is
//! created lazily, once per thread, so commands that never render never pay
//! its start-up cost. It runs under a memory limit, a stack limit and a
//! per-render time limit. After any failure that is not Pele's own error the
//! engine is thrown away and rebuilt for the next render, because a runtime
//! that hit a limit is not trusted to be consistent.
//!
//! Raw SVG only gets a well-formedness check (roxmltree, which rejects a
//! DOCTYPE by default, so entity expansion is not possible). It is stored
//! unsanitised: containment is the untrusted origin, not this module.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use rquickjs::{CatchResultExt, Context, Ctx, Runtime, Value};
use rusqlite::Connection;

use crate::artifact;
use crate::error::Error;

const PELE_SOURCE: &str = include_str!(env!("PELE_JS"));

/// QuickJS heap limit.
pub const MEMORY_LIMIT_BYTES: usize = 64 * 1024 * 1024;
/// QuickJS native stack limit.
pub const STACK_LIMIT_BYTES: usize = 1024 * 1024;
/// Wall-clock limit for one render.
pub const RENDER_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Mermaid,
    Svg,
}

impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Mermaid => "mermaid",
            Format::Svg => "svg",
        }
    }

    /// The Format named by its stored `as_str` form.
    pub fn parse(s: &str) -> Result<Format, Error> {
        match s {
            "mermaid" => Ok(Format::Mermaid),
            "svg" => Ok(Format::Svg),
            other => Err(Error::Unexpected(format!(
                "unknown stored format {other:?}"
            ))),
        }
    }

    /// Media type of the stored file.
    pub fn media_type(self) -> &'static str {
        match self {
            Format::Mermaid => "text/plain",
            Format::Svg => "image/svg+xml",
        }
    }

    /// Format implied by a file extension, when there is exactly one.
    pub fn infer(path: &str) -> Option<Format> {
        match std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("mmd") => Some(Format::Mermaid),
            _ => None,
        }
    }
}

/// Check a diagram's stored bytes: Mermaid must render, SVG must be
/// well-formed XML with an `svg` root.
pub fn validate(format: Format, bytes: &[u8]) -> Result<(), Error> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Error::Rejected("diagram is not valid UTF-8".into()))?;
    match format {
        Format::Mermaid => render_mermaid(text).map(|_| ()),
        Format::Svg => check_svg(text),
    }
}

/// Render a stored diagram Revision (latest by default) to SVG. Mermaid is
/// rendered fresh each time; SVG is returned as stored.
pub fn render_revision(
    conn: &Connection,
    id: &str,
    revision: Option<i64>,
) -> Result<String, Error> {
    let format = artifact::diagram_format(conn, id)?;
    let bytes = artifact::get(conn, id, revision)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| Error::Unexpected("stored diagram is not valid UTF-8".into()))?;
    match format {
        Format::Mermaid => render_mermaid(&text),
        Format::Svg => Ok(text),
    }
}

fn check_svg(text: &str) -> Result<(), Error> {
    let doc = roxmltree::Document::parse(text)
        .map_err(|e| Error::Rejected(format!("svg is not well-formed XML: {e}")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err(Error::Rejected(format!(
            "svg root element is <{}>, not <svg>",
            root.tag_name().name()
        )));
    }
    Ok(())
}

// --- engine ---------------------------------------------------------------

static ENGINE_STARTS: AtomicUsize = AtomicUsize::new(0);

/// How many engines this process has started. Exposed so tests can assert
/// that start-up is lazy and paid once.
pub fn engine_starts() -> usize {
    ENGINE_STARTS.load(Ordering::SeqCst)
}

thread_local! {
    static THREAD_STARTS: Cell<usize> = const { Cell::new(0) };
}

/// Engines started by the calling thread (for the server's one-engine test).
#[cfg(test)]
pub(crate) fn engine_starts_on_thread() -> usize {
    THREAD_STARTS.with(Cell::get)
}

struct Engine {
    // Kept alive for the context.
    _rt: Runtime,
    ctx: Context,
    /// Set before each render; the interrupt handler aborts past it.
    deadline: Rc<Cell<Option<Instant>>>,
    timed_out: Rc<Cell<bool>>,
}

impl Engine {
    fn start() -> Result<Engine, Error> {
        let fail = |e: rquickjs::Error| Error::Unexpected(format!("mermaid engine: {e}"));
        let rt = Runtime::new().map_err(fail)?;
        rt.set_memory_limit(MEMORY_LIMIT_BYTES);
        rt.set_max_stack_size(STACK_LIMIT_BYTES);
        let deadline: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));
        let timed_out = Rc::new(Cell::new(false));
        {
            let deadline = deadline.clone();
            let timed_out = timed_out.clone();
            rt.set_interrupt_handler(Some(Box::new(move || {
                let over = deadline.get().is_some_and(|d| Instant::now() >= d);
                if over {
                    timed_out.set(true);
                }
                over
            })));
        }
        let ctx = Context::full(&rt).map_err(fail)?;
        ctx.with(|c| {
            c.eval::<(), _>(PELE_SOURCE)
                .catch(&c)
                .map_err(|e| e.to_string())
        })
        .map_err(|e| Error::Unexpected(format!("mermaid engine: cannot load Pele: {e}")))?;
        ENGINE_STARTS.fetch_add(1, Ordering::SeqCst);
        THREAD_STARTS.with(|c| c.set(c.get() + 1));
        Ok(Engine {
            _rt: rt,
            ctx,
            deadline,
            timed_out,
        })
    }
}

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

/// Outcome of running JS: a value, an error Pele raised on purpose, or an
/// abort (limit hit or engine fault) after which the engine is discarded.
enum JsOutcome {
    Ok(String),
    Pele { code: String, message: String },
    Abort(String),
}

/// Run `body` against the lazily started engine under the render deadline.
fn with_engine(body: impl FnOnce(Ctx<'_>) -> JsOutcome) -> Result<String, Error> {
    ENGINE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(Engine::start()?);
        }
        let engine = slot.as_ref().expect("engine just started");
        engine.timed_out.set(false);
        engine.deadline.set(Some(Instant::now() + RENDER_TIMEOUT));
        let outcome = engine.ctx.with(body);
        engine.deadline.set(None);
        let timed_out = engine.timed_out.get();
        match outcome {
            JsOutcome::Ok(s) => Ok(s),
            JsOutcome::Pele { code, message } => {
                Err(Error::Rejected(format!("mermaid: {code}: {message}")))
            }
            JsOutcome::Abort(msg) => {
                *slot = None;
                if timed_out {
                    Err(Error::Rejected(format!(
                        "mermaid: render exceeded the {} s time limit",
                        RENDER_TIMEOUT.as_secs()
                    )))
                } else {
                    Err(Error::Rejected(format!(
                        "mermaid: render aborted by the engine limits: {msg}"
                    )))
                }
            }
        }
    })
}

/// Turn an eval result into an outcome, reading Pele's `code` and `message`
/// from the thrown exception object.
fn outcome(ctx: &Ctx<'_>, r: rquickjs::Result<String>) -> JsOutcome {
    match r.catch(ctx) {
        Ok(s) => JsOutcome::Ok(s),
        Err(caught) => {
            let exc: Option<Value> = match &caught {
                rquickjs::CaughtError::Exception(e) => Some(e.clone().into_value()),
                rquickjs::CaughtError::Value(v) => Some(v.clone()),
                rquickjs::CaughtError::Error(_) => None,
            };
            if let Some(o) = exc.as_ref().and_then(|v| v.as_object()) {
                let code: Option<String> = o.get("code").ok();
                let message: Option<String> = o.get("message").ok();
                if let (Some(code), Some(message)) = (code, message)
                    && !code.is_empty()
                {
                    return JsOutcome::Pele { code, message };
                }
            }
            JsOutcome::Abort(caught.to_string().lines().next().unwrap_or("").to_string())
        }
    }
}

/// Render Mermaid source to SVG. Options are fixed here and never come from
/// the author: links and images off, Pele's default light-neutral variables.
pub(crate) fn render_mermaid(src: &str) -> Result<String, Error> {
    with_engine(|ctx| {
        let set: rquickjs::Result<()> = ctx.globals().set("__cairn_src", src);
        if let Err(e) = set {
            return JsOutcome::Abort(e.to_string());
        }
        let r =
            ctx.eval::<String, _>("pele.render(__cairn_src, { links: false, images: false }).svg");
        outcome(&ctx, r)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(cairn_pele_stub))]
    const FLOW: &str = "flowchart LR\n A[Start] --> B{Ok?}\n B -->|yes| C[Done]\n B -->|no| A\n";

    #[cfg(not(cairn_pele_stub))]
    fn run_js(code: &str) -> Result<String, Error> {
        with_engine(|ctx| {
            let r = ctx.eval::<String, _>(code);
            outcome(&ctx, r)
        })
    }

    #[test]
    #[cfg(not(cairn_pele_stub))]
    fn valid_mermaid_renders_svg() {
        let svg = render_mermaid(FLOW).unwrap();
        assert!(svg.trim_start().starts_with("<svg"), "{svg}");
        assert!(ENGINE.with(|c| c.borrow().is_some()));
    }

    #[test]
    #[cfg(not(cairn_pele_stub))]
    fn invalid_mermaid_reports_pele_code_and_message() {
        let e = validate(Format::Mermaid, b"flowchart LR\n A -->\n").unwrap_err();
        assert_eq!(e.exit_code(), 4);
        let m = e.to_string();
        assert!(m.starts_with("mermaid: "), "{m}");
        let rest = m.trim_start_matches("mermaid: ");
        let (code, msg) = rest.split_once(": ").expect("code: message");
        assert!(!code.is_empty() && !code.contains(' '), "{m}");
        assert!(!msg.is_empty(), "{m}");
        // The engine survives a Pele error.
        assert!(render_mermaid(FLOW).is_ok());
    }

    #[test]
    fn click_javascript_links_are_not_emitted() {
        let src = "flowchart LR\n A --> B\n click A \"javascript:alert(1)\"\n";
        if let Ok(svg) = render_mermaid(src) {
            assert!(!svg.contains("javascript:"), "{svg}");
        }
    }

    #[test]
    #[cfg(not(cairn_pele_stub))]
    fn infinite_loop_hits_the_time_limit_and_engine_recovers() {
        let e = run_js("while (true) {}; ''").unwrap_err();
        assert_eq!(e.exit_code(), 4);
        assert!(e.to_string().contains("time limit"), "{e}");
        assert!(render_mermaid(FLOW).is_ok());
    }

    #[test]
    #[cfg(not(cairn_pele_stub))]
    fn runaway_allocation_hits_the_memory_limit_and_engine_recovers() {
        let e = run_js("let a = []; while (true) { a.push(new Array(100000).fill(1)); } ''")
            .unwrap_err();
        assert_eq!(e.exit_code(), 4);
        assert!(render_mermaid(FLOW).is_ok());
    }

    #[test]
    #[cfg(not(cairn_pele_stub))]
    fn deep_recursion_hits_the_stack_limit() {
        let e = run_js("function f(){ return f() + 1 } String(f())").unwrap_err();
        assert_eq!(e.exit_code(), 4);
        assert!(render_mermaid(FLOW).is_ok());
    }

    #[test]
    fn svg_well_formedness() {
        assert!(
            validate(
                Format::Svg,
                b"<svg xmlns='http://www.w3.org/2000/svg'><g/></svg>"
            )
            .is_ok()
        );
        // Stored as-is, not sanitised.
        assert!(
            validate(
                Format::Svg,
                b"<svg xmlns='http://www.w3.org/2000/svg'><script>alert(1)</script></svg>"
            )
            .is_ok()
        );
        for bad in [
            &b"<svg><g></svg>"[..],
            b"not xml",
            b"<html/>",
            b"<!DOCTYPE svg [<!ENTITY a 'b'>]><svg/>",
            b"\xff\xfe",
        ] {
            assert_eq!(validate(Format::Svg, bad).unwrap_err().exit_code(), 4);
        }
    }

    #[test]
    fn svg_validation_does_not_start_the_engine() {
        // Runs on its own thread: the engine is thread-local.
        std::thread::spawn(|| {
            assert!(ENGINE.with(|c| c.borrow().is_none()));
            validate(Format::Svg, b"<svg/>").unwrap();
            assert!(ENGINE.with(|c| c.borrow().is_none()));
        })
        .join()
        .unwrap();
    }

    #[test]
    #[cfg(not(cairn_pele_stub))]
    fn engine_is_started_once_per_thread() {
        std::thread::spawn(|| {
            let before = engine_starts();
            render_mermaid(FLOW).unwrap();
            render_mermaid(FLOW).unwrap();
            // Other test threads may start engines concurrently, so assert
            // on this thread's slot rather than the global counter alone.
            assert!(engine_starts() > before);
            assert!(ENGINE.with(|c| c.borrow().is_some()));
        })
        .join()
        .unwrap();
    }

    #[test]
    #[cfg(cairn_pele_stub)]
    fn stubbed_build_reports_mermaid_unavailable() {
        let e = validate(Format::Mermaid, b"flowchart LR\n A --> B\n").unwrap_err();
        assert!(e.to_string().contains("built without PELE_JS"), "{e}");
    }
}
