---
id: TASK-12
title: '`serve` with the untrusted content port'
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
dependencies:
  - TASK-9
  - TASK-10
ordinal: 12000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`cairn serve` starts two axum listeners in one process, both on loopback only: the UI port (default 7878, `--port`, `CAIRN_PORT`) with a placeholder page for now, and the content port (default UI port + 1, `--content-port`, `CAIRN_CONTENT_PORT`). A taken port is a clear non-zero exit, never a random port. The content port serves all Agent-supplied bytes at a path pinned to a Revision, never from the UI port (ADR 0005). Every response carries nosniff, the per-Kind CSP (html: sandboxed scripts and no network; images and SVG: script-less sandbox, with frame-ancestors for the UI origin where documented) and a Content-Type taken from the stored allowlisted media type. Mermaid is rendered to SVG on demand and served the same way. One long-lived render engine lives in the server. The web-safety spike is the reference for the expected behaviour. `serve` also gets the reviewer-name flag for later UI tickets.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both listeners bind loopback only; a taken port exits non-zero with a clear message
- [ ] #2 Header tests per Kind assert CSP, nosniff and Content-Type exactly as documented
- [ ] #3 Bundle assets resolve by relative path on the content port, pinned to a Revision
- [ ] #4 SVG and Mermaid are served script-less; Mermaid renders on demand
- [ ] #5 An unknown Artifact or path returns 404 with no untrusted bytes leaked
- [ ] #6 The UI port never serves Artifact bytes
<!-- AC:END -->
