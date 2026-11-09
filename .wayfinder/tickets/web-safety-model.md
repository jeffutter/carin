---
title: Web safety model for untrusted artifacts
type: grilling
status: closed
assignee: claude
blocked_by: [research-iframe-sandbox, kinds-and-storage]
---

## Question

How does the web UI show agent-supplied HTML, SVG and images safely? Decide the iframe sandbox flags, the CSP, whether artifacts are served from a separate origin or port, how HTML-with-images resolves its images, and what the web UI itself must never trust. Record an ADR if the choice is hard to reverse.

## Carried in from Artifact kinds and storage

Blobs live in sqlite with a stored media type; decide how they are served from the untrusted origin (Content-Type from an allowlist, nosniff). Html Bundles serve assets by relative path. Markdown `cairn:` embeds (image and diagram Artifacts only) render as `<img>` from the untrusted origin; `http:`, `data:` and relative image URLs in markdown are not rendered.

## Resolution

Decided with the user; recorded in ADR 0005 (`docs/adr/0005-serve-untrusted-content-from-a-separate-origin.md`).

- Two axum listeners, one process: UI port and an untrusted-content port (a single flag, loopback). UI never serves artifact bytes.
- Html Bundles load in `<iframe src>` at `/a/<id>/<rev>/<path>` with `sandbox="allow-scripts"`, no `allow-same-origin`. CSP blocks all network (`connect-src 'none'`, no remote img/script/font); Bundles must be self-contained.
- Images and SVGs get a script-less `sandbox` CSP and are shown only via `<img>`; nosniff and allowlisted Content-Type everywhere.
- UI rules: markdown raw HTML dropped, all text escaped, `javascript:`/`data:` links stripped, `noopener noreferrer`, strict UI CSP, `frame-ancestors 'self'`.

Spike (`.wayfinder/assets/web-safety-spike/`, run in headless Chromium via agent-browser; one browser, one run each) verified the earlier unknowns:
- Relative and absolute-path images and `data:` images load in the sandboxed frame; `'self'` matches; a remote-origin image, `fetch` and cookie/localStorage/parent access are blocked.
- A script SVG opened directly ran when the CSP sandbox allowed scripts, and did not run with the script-less sandbox.
- `tokio::try_join!(axum::serve(a, app()), axum::serve(b, app()))` serves two ports in one process (axum 0.8), and a header array in a response tuple works.
- Not checked: other browsers, the iframe `csp` attribute, and whether `location.origin` reporting the real URL (seen in the frame) matters (storage and parent access were still blocked).
