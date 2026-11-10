---
id: TASK-13
title: 'Web UI: list and read-only Artifact page'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T01:52:00Z'
labels:
  - planned
dependencies:
  - TASK-12
  - TASK-5
ordinal: 13000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A Reviewer opens the UI and browses. The server-rendered UI (maud, htmx fragments, no npm, calling core in-process) shows a list table (name, Kind, project and worktree, latest Revision, open Thread count, size, updated; filterable by project and Kind; unscoped by default; newest activity first) and the markdown Artifact page: header with name, Kind and a revision dropdown, a banner linking to the latest when an old Revision is viewed, and a sticky outline sidebar with open-Thread counts per Section. Markdown renders with raw HTML dropped, `javascript:` and `data:` links stripped, external links `noopener noreferrer`, and all text escaped. The UI sets its own strict CSP. htmx is pinned with `fetchurl` and embedded with the hand-written CSS, served at a versioned URL. Match the look of the throwaway prototype. `cairn view <id>[@rev]` prints the URL (from `--url`, `CAIRN_URL`, or the default) without checking a server.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 List and markdown page render real data; filters work
- [x] #2 Revision dropdown navigates Revisions; old-Revision banner links to the latest
- [x] #3 A markdown file containing raw HTML and script links renders inert (tests assert it)
- [x] #4 Titles, author names and comment text are escaped
- [x] #5 Response CSP on UI pages is strict with `frame-ancestors 'self'`
- [x] #6 The nix build embeds the pinned htmx; no network access at build time beyond the fetch
- [x] #7 Layout collapses to one column below about 800 px
- [x] #8 `view` prints the correct URL for all three base sources
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by d8ce3f9. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in src/ui.rs (maud, axum Query), UI CSP + router wiring in src/serve.rs, src/ui/style.css (prototype variant B look), htmx pinned 2.0.4 via fetchurl in flake.nix with HTMX_JS stub in build.rs, 'cairn view' in cli.rs/lib.rs. Markdown goes through pulldown-cmark events: raw HTML dropped, images become alt text, only http/https/mailto/relative hrefs kept, rel=noopener noreferrer on all anchors, top-level headings get id=sN for the outline. Open Thread counts per Section: Section locator match, else line Threads on the viewed Revision map to the innermost containing Section. Non-markdown Kinds show a placeholder (TASK-15). Verified: cargo test, clippy -D warnings, nix build (real 50 KB htmx served by the built binary, curl). AC7 is covered by a test that the CSS has the 800px media query; not checked in a real browser at both widths, and the htmx dropdown change-trigger was not exercised in a browser (the no-JS form path is tested).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Server-rendered list page with project/kind filters, markdown Artifact page with revision dropdown, old-revision banner and outline sidebar with open-Thread counts, inert markdown rendering, strict UI CSP, pinned embedded htmx and CSS at hashed URLs, and cairn view.
<!-- SECTION:FINAL_SUMMARY:END -->
