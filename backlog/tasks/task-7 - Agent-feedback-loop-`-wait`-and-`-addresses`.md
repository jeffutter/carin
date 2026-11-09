---
id: TASK-7
title: 'Agent feedback loop: `--wait` and `--addresses`'
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
dependencies:
  - TASK-6
ordinal: 7000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The closed loop between Reviewer and Agent. `comment list --wait` blocks until something newer than `--since` exists, polling the database every 500 ms (a constant in core, no server needed), with `--timeout <secs>` (default 60); a timeout with nothing new exits 5. `edit --addresses 12,14` posts a reply "addressed in abc123@4" in each named Thread as the editing Revision's author and role, and never resolves the Thread.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `--wait` returns promptly when a Comment arrives from another process during the wait
- [ ] #2 Timeout exits 5 with an empty result and the same `highest_comment_id`
- [ ] #3 `--addresses` posts one reply per named Thread, attributed to the Revision's author and role
- [ ] #4 The Thread stays open after `--addresses`
- [ ] #5 An unknown Comment id in `--addresses` exits 3 and creates no Revision
<!-- AC:END -->
