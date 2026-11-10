---
id: TASK-12
title: '`serve` with the untrusted content port'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T01:41:00Z'
labels:
  - planned
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
- [x] #1 Both listeners bind loopback only; a taken port exits non-zero with a clear message
- [x] #2 Header tests per Kind assert CSP, nosniff and Content-Type exactly as documented
- [x] #3 Bundle assets resolve by relative path on the content port, pinned to a Revision
- [x] #4 SVG and Mermaid are served script-less; Mermaid renders on demand
- [x] #5 An unknown Artifact or path returns 404 with no untrusted bytes leaked
- [x] #6 The UI port never serves Artifact bytes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-12 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added src/serve.rs (UI router, content router, single Mermaid render thread, loopback bind helper), artifact::kind_of and artifact::get_file, Command::Serve with --port/--content-port/--reviewer-name, axum/tokio deps. Mermaid served at diagram.svg only; markdown is 404 on the content port. Per-request db::open in spawn_blocking. Tests: in-process router tests per Kind (CSP/nosniff/Content-Type exact, pinned revisions, identical 404s, UI never serves bytes, one engine on render thread) and subprocess tests (taken ports exit 1, port 0 exit 2, headers over a real socket). Verified with cargo clippy -D warnings, cargo test (in nix develop) and nix build. Docs updated in docs/cli.md and docs/web-ui.md.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
cairn serve now runs a loopback UI listener (placeholder page) and a content listener serving Revision-pinned Artifact bytes with per-Kind CSP, nosniff and stored Content-Type; Mermaid renders on demand on one dedicated engine thread; unknown paths and markdown 404 with a fixed body.
<!-- SECTION:FINAL_SUMMARY:END -->
