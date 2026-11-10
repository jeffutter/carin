---
id: TASK-37
title: 'Fix: edit runs Mermaid prepare before cheap stale-base/exists/addresses checks'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T02:01:00Z'
updated_date: '2026-11-10T02:07:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-34
priority: high
ordinal: 37000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After TASK-34, artifact edit runs bundle::prepare (Mermaid render, up to the render time limit) before the stale --base, missing Artifact and unknown --addresses checks. A stale-base edit now pays the full render and bad input may be reported as Rejected instead of Conflict. Run the cheap checks first (then prepare, then re-verify inside the transaction). Also EditPlan::read/Latest::single_path loads blob bytes just to get the path, twice, once under the write lock; use a path-only query.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 445d42b. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added check_preconditions (exists, stale base, addresses) used by edit before prepare and by append_revision inside the tx; Latest::single_path now path-only SQL. Test cheap_checks_win_over_prepare_rejection added. cargo test, clippy, fmt clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
edit runs cheap precondition checks before Mermaid prepare and re-verifies in the write tx; single_path no longer loads blobs.
<!-- SECTION:FINAL_SUMMARY:END -->
