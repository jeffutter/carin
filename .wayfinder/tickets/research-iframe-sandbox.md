---
title: Research iframe sandboxing and CSP for untrusted agent HTML
type: research
status: closed
assignee: jeffutter
blocked_by: []
---

## Question

What is current best practice for showing untrusted HTML (single page, with images) inside a web app? Cover the iframe `sandbox` attribute combinations, `srcdoc` versus a URL, CSP (including `sandbox` and `default-src` via header), serving from a separate origin or port on localhost, and which of these an axum server can do. Surface facts only; the decision is the Web safety model ticket.

## Resolution

Findings are on branch `research/iframe-sandbox` (commit 52bda31, local only, never merged) in `docs/research/iframe-sandbox.md`. It covers the iframe `sandbox` tokens, `srcdoc` versus a URL, CSP, serving from a separate origin or port on localhost, and what axum can do, cited to MDN, WHATWG HTML, W3C CSP3, docs.rs and the axum repo.

The file marks these points "Not verified", so the Web safety model decision should not rest on them without a check:
- the origin of an `about:srcdoc` document, and whether it inherits the parent's CSP (CSP3 section 7.8 not retrieved)
- whether cookies are shared across ports on one host (RFC 6265 not fetched)
- how images referenced by relative URL behave in an opaque-origin framed document, and whether `'self'` matches there
- whether `axum::serve` can run twice in one process (the only example found uses hyper-util with `tokio::join!`)
- whether a header array works inside an axum response tuple
- browser support for the iframe `csp` attribute

The agent read pages through a summarizing fetch tool and ran no code, so quotes are second-hand. Its suggested follow-up is a second pass on RFC 6265 and CSP3 7.8, plus a quick browser test of images in a sandboxed frame.
