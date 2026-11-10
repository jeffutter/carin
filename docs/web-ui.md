# Web UI

The review UI is where the Reviewer reads Artifacts and discusses them. It is server-rendered HTML from axum routes using `maud`, enhanced by htmx fragments. No npm, no JavaScript build ([ADR 0006](adr/0006-server-rendered-web-ui-with-htmx-and-maud.md)). A throwaway prototype of the layout is at `.wayfinder/assets/web-ui-prototype/index.html`; it uses fake data and is a look to match, not code to reuse.

## Routes and assets

- The UI and `/api/v1` live on the UI port; routes include `/` (list) and `/a/<id>[@rev]` (Artifact page).
- htmx is pinned in the flake with `fetchurl` (`HTMX_JS`, stubbed by `build.rs` when unset). Hand-written CSS (`src/ui/style.css`) and htmx are embedded with `include_str!` and served from the UI port at `/static/<content-hash>/{htmx.js,app.css}` with an immutable cache header.
- `/?project=&kind=` filters the list (a plain GET form; htmx swaps only the table). `/a/<id>?rev=N` redirects to `/a/<id>@N`; the revision dropdown is that form.
- Posting a Comment or resolving a Thread calls core directly. The author is `serve --reviewer-name` with role reviewer.

## Artifact list

A plain table: name, Kind, project and worktree, latest Revision, open Thread count, size, updated. Filterable by project and Kind. Unscoped by default. Each row has a delete action. Sorted by newest activity.

## Artifact page

- **Header**: name, Kind, a revision dropdown (the only revision navigation; there is no revision list in the sidebar). Viewing an old Revision shows a banner that it is not the latest, linking to the latest.
- **Markdown**: a sticky outline sidebar with open-Thread counts per Section. Threads render inline under the Section or line they anchor to. A line-number gutter and a per-heading "+ section" button start a new Thread.
- **Other Kinds** (html, image, diagram): the same page with the Kind's frame in place of the text. Comments are on the whole Revision.
- **Threads from other Revisions**: an open Thread made on an earlier Revision still shows when another Revision is viewed, with a dashed border and an "earlier revision" label. A resolved Thread from another Revision is hidden.
- **Phones**: below about 800 px the layout collapses to one column and the sidebar becomes static.
- **Live refresh**: an htmx poll every 2 s on the Artifact page, paused while the tab is hidden, using the same latest-comment check as `--wait`. Fixed, no flag. The UI does not use the long-poll endpoint.

### Reviewer attention

An optional per-Artifact "last seen" marker in sqlite can mark Artifacts with a new Revision or Agent reply since the Reviewer last looked. A hint, not a contract. A needs-attention inbox view was dropped from v1 and may return later.

## Untrusted content

Everything an Agent supplies is untrusted. The model is in [ADR 0005](adr/0005-serve-untrusted-content-from-a-separate-origin.md); the rules:

- Two listeners, one process: the UI port, and an untrusted-content port (`--content-port`, default UI port + 1). They are different origins. The UI never serves Artifact bytes.
- `cairn serve` prints one JSON line on stdout once both ports are bound: `{"ui":"http://127.0.0.1:P","content":"http://127.0.0.1:Q","reviewer":"..."}`.
- Untrusted bytes live at `/a/<id>/<rev>/<path>` on the content port, always pinned to a Revision.
- **html**: shown in `<iframe src=...>` with `sandbox="allow-scripts"` and never `allow-same-origin`. CSP: `sandbox allow-scripts; default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'none'; frame-ancestors <ui origin>`. A Bundle is self-contained: no remote images, scripts, fonts or `fetch`.
- **Images and diagram SVGs**: shown only via `<img>`, with CSP `sandbox; default-src 'none'; style-src 'unsafe-inline'` (no `allow-scripts`). Mermaid is rendered to SVG on demand and served the same way, at the fixed path `/a/<id>/<rev>/diagram.svg` of its Revision; the stored `.mmd` source is never served. A render failure is a fixed 500 with no source or renderer text. Diagrams stay fixed light-neutral inside `<img>`.
- **markdown** Artifacts are 404 on the content port; the UI renders them. An unknown Artifact, Revision or path, a malformed Revision (`latest` is not an alias) and a wrong-Kind path all return the same fixed `not found` body.
- **Every** untrusted response carries `X-Content-Type-Options: nosniff`, a CSP, and a `Content-Type` from the allowlisted media type stored at submit.
- **The UI itself** renders markdown with raw HTML dropped, escapes comment text, author names and titles, strips `javascript:` and `data:` links, opens links with `rel="noopener noreferrer"`, and sets its own strict CSP with `frame-ancestors 'self'`.
- **Markdown embeds** (`cairn:`) render as `<img>` from the content origin. `http:`, `data:` and relative image URLs show as alt text. An embed whose target is deleted shows a missing-image placeholder.

The spike in `.wayfinder/assets/web-safety-spike/` verified these in Chromium: relative, absolute-path and `data:` images load in the sandboxed frame; remote images, `fetch` and parent or storage access are blocked; a script in an SVG opened directly runs only if the CSP sandbox allows scripts. Not verified: other browsers and the iframe `csp` attribute.
