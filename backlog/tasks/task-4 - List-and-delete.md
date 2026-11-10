---
id: TASK-4
title: List and delete
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T00:20:00Z'
labels:
  - planned
dependencies:
  - TASK-3
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
An Agent or user can see and remove Artifacts. `list` shows newest activity first with id, Kind, name, latest Revision, open Thread count, author, stored bytes, last-updated time, and project and worktree columns (null until scoping lands). Filters: `--kind`, `--session`, `--mine`, `--open`, `--older-than <duration>`, `--limit` (default 50); `--text` gives a human table. `delete <id>... --yes` removes several Artifacts, each with all Revisions, Comments and now-unreferenced blobs, in one transaction; without `--yes` it exits 2.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `list` ordering, row shape and each filter behave as documented
- [x] #2 `--older-than` accepts a duration and filters on last-updated
- [x] #3 `delete` without `--yes` exits 2 and deletes nothing
- [x] #4 Deleting several ids is atomic; an unknown id exits 3
- [x] #5 Blobs still referenced by another Artifact survive; orphaned blobs are removed and the file shrinks
- [x] #6 A write to a deleted Artifact exits 3
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-4. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added artifact::list/parse_duration/delete, CLI List/Delete, Context.session_explicit (so list --session ignores the CLAUDE_CODE_SESSION_ID fallback). open_threads is constant 0 and --open yields nothing until TASK-6 adds comments; TASK-6 must make comments cascade on artifact delete and return NotFound for writes to a missing Artifact. Docs updated in docs/cli.md. Tests in tests/cli.rs mod list_delete and unit tests.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
list (filters, --text, JSON {artifacts}) and atomic delete --yes with orphan blob cleanup implemented; cargo test, clippy, fmt pass.
<!-- SECTION:FINAL_SUMMARY:END -->
