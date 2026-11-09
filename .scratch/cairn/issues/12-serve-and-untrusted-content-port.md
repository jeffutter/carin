# 12: `serve` with the untrusted content port

**What to build:** `cairn serve` starts two axum listeners in one process, both on loopback only: the UI port (default 7878, `--port`, `CAIRN_PORT`) with a placeholder page for now, and the content port (default UI port + 1, `--content-port`, `CAIRN_CONTENT_PORT`). A taken port is a clear non-zero exit, never a random port. The content port serves all Agent-supplied bytes at a path pinned to a Revision, never from the UI port (ADR 0005). Every response carries nosniff, the per-Kind CSP (html: sandboxed scripts and no network; images and SVG: script-less sandbox, with frame-ancestors for the UI origin where documented) and a Content-Type taken from the stored allowlisted media type. Mermaid is rendered to SVG on demand and served the same way. One long-lived render engine lives in the server. The web-safety spike is the reference for the expected behaviour. `serve` also gets the reviewer-name flag for later UI tickets.

**Blocked by:** 09: Image Artifacts and html Bundles, 10: Diagram Artifacts (svg and Mermaid)

**Status:** ready-for-agent

- [ ] Both listeners bind loopback only; a taken port exits non-zero with a clear message
- [ ] Header tests per Kind assert CSP, nosniff and Content-Type exactly as documented
- [ ] Bundle assets resolve by relative path on the content port, pinned to a Revision
- [ ] SVG and Mermaid are served script-less; Mermaid renders on demand
- [ ] An unknown Artifact or path returns 404 with no untrusted bytes leaked
- [ ] The UI port never serves Artifact bytes
