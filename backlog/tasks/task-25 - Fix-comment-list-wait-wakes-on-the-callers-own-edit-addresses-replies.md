---
id: TASK-25
title: 'Fix: comment list --wait wakes on the caller''s own edit --addresses replies'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T00:47:00Z'
updated_date: '2026-11-10T00:50:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-7
priority: high
ordinal: 25000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
post_addressed inserts reply comments that pass the c.id > since filter in comments::wait (src/comments.rs ~300). An agent that runs edit --addresses then comment list --wait with its pre-edit --since returns immediately with its own reply as new feedback, so the feedback loop spins. Exclude comments authored by the waiting Session (or advance since past the replies) and add a test.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-25. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
wait() now uses list_inner(exclude_own=true): a Comment counts as newer only if not written by the caller (same author and Session). Public list() and ListQuery unchanged. Updated existing wait test to use another author. Added test wait_ignores_own_addresses_replies_but_wakes_on_others. docs/cli.md updated. cargo test and clippy pass (via nix develop).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
comment list --wait ignores the caller's own Comments (including edit --addresses replies) when deciding to wake; other authors still wake it.
<!-- SECTION:FINAL_SUMMARY:END -->
