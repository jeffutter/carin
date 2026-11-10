---
id: TASK-7
title: 'Agent feedback loop: `--wait` and `--addresses`'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T00:43:00Z'
labels:
  - planned
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
- [x] #1 `--wait` returns promptly when a Comment arrives from another process during the wait
- [x] #2 Timeout exits 5 with an empty result and the same `highest_comment_id`
- [x] #3 `--addresses` posts one reply per named Thread, attributed to the Revision's author and role
- [x] #4 The Thread stays open after `--addresses`
- [x] #5 An unknown Comment id in `--addresses` exits 3 and creates no Revision
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 53987a4. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added comments::wait (500 ms poll, WAIT_POLL_MS), run() now returns the exit code (5 on timeout, JSON still printed), edit --addresses validated inside the Revision write transaction via comments::check_addressable/post_addressed, shared insert_comment. Bare --wait baseline = highest id at entry. Tests in tests/cli.rs; docs/cli.md updated.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
comment list --wait/--timeout and edit --addresses implemented with tests; fmt, clippy, test all pass.
<!-- SECTION:FINAL_SUMMARY:END -->
