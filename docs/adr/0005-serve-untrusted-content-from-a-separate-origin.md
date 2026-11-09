# Serve untrusted content from a separate origin with a sandbox CSP

The server runs two axum listeners in one process: the review UI on one loopback port and all agent-supplied bytes (html Bundle files, images, rendered diagram SVGs) on a second port, so they are different origins. The UI never serves artifact bytes. Untrusted bytes live at `/a/<id>/<rev>/<path>`, always pinned to a Revision.

An html Artifact is shown in an `<iframe src=...>` with `sandbox="allow-scripts"` and never `allow-same-origin`. Every untrusted response carries `X-Content-Type-Options: nosniff` and a CSP header, and a `Content-Type` taken from an allowlisted media type checked at submit:

- html Bundle files: `sandbox allow-scripts; default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'none'; frame-ancestors <ui origin>`. A Bundle must be self-contained: no remote images, scripts, fonts or `fetch`.
- images and diagram SVGs: `sandbox; default-src 'none'; style-src 'unsafe-inline'` (no `allow-scripts`), and the UI embeds them only via `<img>`.

The UI renders markdown with raw HTML dropped, escapes comment text, author names and titles, strips `javascript:` and `data:` links, opens links with `rel="noopener noreferrer"`, and sets its own strict CSP with `frame-ancestors 'self'`.

We chose this over serving everything from one origin (a script in a Bundle could then act as the UI) and over `srcdoc` (relative asset paths would not resolve to Bundle files). The CSP `sandbox` header goes on every response, not just iframed ones, because a spike showed a script inside an SVG opened directly ran when the header allowed scripts; only html needs `allow-scripts`. The cost is a second port to configure and a Bundle that cannot use a CDN. Spike code is in `.wayfinder/assets/web-safety-spike/`.
