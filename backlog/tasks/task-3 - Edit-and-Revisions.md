---
id: TASK-3
title: Edit and Revisions
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
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
- [ ] #1 `edit` creates Revision n+1 and leaves earlier Revisions unchanged
- [ ] #2 `get <id>@<n>` returns that Revision; a missing Revision exits 3
- [ ] #3 `--base` with a stale value exits 6 with `latest_revision` and its author in the error
- [ ] #4 Unchanged content shares its blob with the previous Revision
- [ ] #5 Two concurrent edits produce distinct, consecutive Revision numbers (test with two processes)
- [ ] #6 A Revision over the per-Revision limit is rejected with exit 4
<!-- AC:END -->
