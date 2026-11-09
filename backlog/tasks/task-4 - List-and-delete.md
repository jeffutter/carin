---
id: TASK-4
title: List and delete
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
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
- [ ] #1 `list` ordering, row shape and each filter behave as documented
- [ ] #2 `--older-than` accepts a duration and filters on last-updated
- [ ] #3 `delete` without `--yes` exits 2 and deletes nothing
- [ ] #4 Deleting several ids is atomic; an unknown id exits 3
- [ ] #5 Blobs still referenced by another Artifact survive; orphaned blobs are removed and the file shrinks
- [ ] #6 A write to a deleted Artifact exits 3
<!-- AC:END -->
