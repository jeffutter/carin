---
id: TASK-26
title: 'Fix: comment list --wait --timeout overflow panics'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T00:48:00Z'
updated_date: '2026-11-10T00:52:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-7
priority: high
ordinal: 26000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/comments.rs ~305 computes Instant::now() + Duration::from_secs(timeout) with an unbounded u64. --timeout 18446744073709551615 panics (exit 101, no JSON error, outside the documented exit-code table). Use checked_add or clamp/validate in the CLI value_parser in src/cli.rs and add a test.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 6a06852. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Replaced Instant::now()+timeout in comments::wait with checked_add -> Option<Instant>; None means no deadline (poll forever). Added integration test wait_with_max_u64_timeout_does_not_panic in tests/cli.rs. cargo test and clippy pass (via nix develop).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
comment list --wait no longer panics on huge --timeout; overflow means wait without deadline. Test added.
<!-- SECTION:FINAL_SUMMARY:END -->
