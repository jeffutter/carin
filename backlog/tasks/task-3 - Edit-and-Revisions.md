---
id: TASK-3
title: Edit and Revisions
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T00:17:00Z'
labels:
  - planned
dependencies:
  - TASK-2
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
An Agent edits an Artifact with `edit <id> <path|->`, creating a new immutable full-snapshot Revision and printing `abc123@4`. `get abc123@3` reads any earlier Revision. `--base <rev>` refuses a stale base with exit 6 naming the latest Revision and its author. The next Revision number and the Artifact-exists check happen inside the write transaction, so concurrent writers are safe; a write to a deleted Artifact exits 3. The per-Revision size limit is enforced here.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `edit` creates Revision n+1 and leaves earlier Revisions unchanged
- [x] #2 `get <id>@<n>` returns that Revision; a missing Revision exits 3
- [x] #3 `--base` with a stale value exits 6 with `latest_revision` and its author in the error
- [x] #4 Unchanged content shares its blob with the previous Revision
- [x] #5 Two concurrent edits produce distinct, consecutive Revision numbers (test with two processes)
- [x] #6 A Revision over the per-Revision limit is rejected with exit 4
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit titled 'Edit and Revisions'. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added artifact::edit (src/artifact.rs), limits::check_revision, Command::Edit and dispatch. New revision reuses path/media_type from the latest revision's file. Tests in tests/cli.rs cover all ACs incl. 8 concurrent processes. cargo test/clippy/fmt clean via nix develop.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
edit <id> <path|-> [--base N] creates full-snapshot Revision n+1 inside a BEGIN IMMEDIATE transaction; stale base exits 6 naming latest_revision and author; deleted/unknown artifact exits 3; limits enforced with exit 4.
<!-- SECTION:FINAL_SUMMARY:END -->
