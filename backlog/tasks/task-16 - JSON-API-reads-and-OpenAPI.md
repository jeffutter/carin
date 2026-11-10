---
id: TASK-16
title: 'JSON API: reads and OpenAPI'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T02:33:00Z'
labels:
  - planned
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
- [x] #1 Each read route returns documented shapes; CLI JSON and API JSON agree for the same operation
- [x] #2 Error statuses follow the mapping table (400, 404, 422, 409, 500)
- [x] #3 Raw file responses carry attachment disposition and nosniff
- [x] #4 OpenAPI document served and snapshot-tested; changing a type fails the test until the snapshot is updated
- [x] #5 The schema crate choice is stated in the PR with the dependency-tree evidence
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-16 (git log --grep=TASK-16). This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added src/api.rs: read-only /api/v1 router merged into serve::ui_router before the fallback and layers (inherits Host check and hardening). Routes: artifacts list, artifact, revisions, revision manifest, raw file (attachment, nosniff, ?section= for markdown, exact DB path match), outline (?revision=), comments (all, since), openapi.json. Errors are problem+json via Error::http_status(); query/path rejections, unknown /api/v1 paths and wrong methods are problem+json too. Shared types ArtifactList/ArtifactDetail/RevisionList/RevisionManifest/Outline in core; CLI list now prints {"items"} (was "artifacts") and outline prints sections::Outline, so CLI and API share one serde type (tests/cli.rs and docs/cli.md updated). Added ListFilter.id and ScopeFilter::in_worktree. Limit outside 1..200 is 422. Chose utoipa over aide: ADR docs/adr/0008 holds the cargo tree comparison (AC5; no PR exists yet, repeat it in the PR body). Snapshot docs/openapi.json, test api::tests::openapi_snapshot_has_not_drifted; regenerate with UPDATE_OPENAPI=1 cargo test openapi_snapshot. Verified drift failure by editing a doc comment, and ran cairn serve with curl. fmt, clippy -D warnings, full cargo test pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Read-only JSON API under /api/v1 on the UI port with problem+json errors, shared CLI/API serde types, and a utoipa-generated OpenAPI 3.1 document served and snapshot-tested. Schema crate choice recorded in ADR 0008. CLI list envelope key changed from artifacts to items.
<!-- SECTION:FINAL_SUMMARY:END -->
