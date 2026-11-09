---
id: TASK-16
title: 'JSON API: reads and OpenAPI'
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
dependencies:
  - TASK-12
  - TASK-8
ordinal: 16000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Outside tools can read Cairn over `/api/v1` on the UI port (loopback, no auth, no CORS). Read routes: list artifacts (same filters as the CLI plus project, project root and worktree; unscoped by default; default limit 50, max 200; `{ "items": [...] }` with stored bytes and updated time), get an Artifact, revisions list, one Revision as a file manifest (path, media type, size, sha256), raw file bytes with the stored media type, attachment disposition and nosniff (with `?section=` for markdown), outline, and comments list. Errors are RFC 9457 problem+json with the same stable `code` as the CLI and the documented status mapping. The shared serializable types are the ones the CLI already prints. Generate an OpenAPI 3.1 document from the Rust types, serve it at the openapi route, and commit a snapshot with a test that fails on drift. Pick `utoipa` or `aide` after checking the dependency tree, and record the choice.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each read route returns documented shapes; CLI JSON and API JSON agree for the same operation
- [ ] #2 Error statuses follow the mapping table (400, 404, 422, 409, 500)
- [ ] #3 Raw file responses carry attachment disposition and nosniff
- [ ] #4 OpenAPI document served and snapshot-tested; changing a type fails the test until the snapshot is updated
- [ ] #5 The schema crate choice is stated in the PR with the dependency-tree evidence
<!-- AC:END -->
