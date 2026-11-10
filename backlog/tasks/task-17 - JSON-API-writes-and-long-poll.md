---
id: TASK-17
title: 'JSON API: writes and long poll'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T02:39:00Z'
labels:
  - planned
dependencies:
  - TASK-16
  - TASK-7
  - TASK-11
ordinal: 17000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Outside tools can create and change things over the API. Create and edit take multipart form data: a JSON meta part (kind, format, name, role, author, session, addresses, base_revision, and on create the project fields) plus one part per file; a Section edit is a PUT with a section query and one markdown part. Delete, comment add and reply, and thread resolve and reopen complete the route table. Attribution is body fields only, defaulted as in the CLI. All core limits and embed validation apply, with errors mapped as in the reads ticket (stale base gives 409). `GET` comments with `since` and `wait` long-polls (polling the database every 500 ms, default 60 s, capped); a timeout is a plain 200 with an empty list and the same `highest_comment_id`. The OpenAPI document and snapshot are updated.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Create, edit, section edit and delete work end to end for every Kind
- [x] #2 A stale `base_revision` returns 409 naming the latest Revision and its author
- [x] #3 Over-limit, bad Kind and bad embed return 422 with the stable code
- [x] #4 Long poll returns early on a new Comment and returns 200 empty on timeout
- [x] #5 Comment add, reply, resolve and reopen match CLI behaviour
- [x] #6 OpenAPI snapshot updated and passing
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-17. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in src/api.rs on axum's multipart feature (multer 3.1.0, plus the json feature; Cargo.lock gains edges only, crane vendors from it). Upload extractor enforces per-file, per-Revision and file-count limits while reading; body limit is Revision max + 2 MiB so over-limit gives 422 not 413. meta is optional on edit; kind/format/name/project fields on edit and base_revision/addresses on create are 400. Bad kind/format on create map to 422 explicitly. Single file edit passes path '-' (image keeps stored path; markdown path forced; html single file becomes index.html). Comments POST is JSON (J extractor, 400 problems); lines uses cli::parse_lines (now pub(crate)). Long poll: wait=<secs> clamped to 300, true/empty = 60, absent/false/0 = no wait; docs updated. CSRF: middleware refuses any non-GET with an Origin not equal to Host (403, code forbidden), documented in docs/api.md; no ADR conflict found. run() now hands closures &mut Connection. Added/ThreadResult got ToSchema; OpenAPI snapshot regenerated. Tests: 12 new in api::tests; cargo test, clippy --all-targets, fmt --check pass. Core 409 detail already names latest revision and author, no core change needed. Not verified: nix flake build and a live curl smoke against cairn serve.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
JSON API writes and long poll: POST/PUT/DELETE /api/v1/artifacts (multipart, all Kinds, section edit), JSON comment add/reply, thread resolve/reopen, long poll via wait on GET comments, Origin check on writes, OpenAPI and docs/api.md updated, with tests for each acceptance criterion.
<!-- SECTION:FINAL_SUMMARY:END -->
