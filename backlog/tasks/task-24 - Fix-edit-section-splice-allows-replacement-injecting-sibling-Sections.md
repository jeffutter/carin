---
id: TASK-24
title: 'Fix: edit --section splice allows replacement injecting sibling Sections'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T00:35:00Z'
updated_date: '2026-11-10T00:40:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-21
priority: high
ordinal: 24000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/sections.rs ~316: splice validation rejects only shallower headings or a mismatched first heading. A replacement containing '## A' followed by '## B' for 'edit --section A' passes and silently creates a new same-level Section, changing section paths and occurrence counts. Reject additional headings at or above the replaced Section's level within the replacement (after the first), with tests.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 splice rejects a replacement with a second heading at or above the replaced Section's level
- [x] #2 Tests cover same-level sibling injection and deeper headings still allowed
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-24. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Tightened splice() in src/sections.rs: first heading must match level; any later heading at or above the replaced level is rejected. Added tests for same-level sibling injection, nested target, deeper headings allowed. cargo test and clippy pass (via nix develop).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
splice now rejects replacement headings after the first at or above the replaced Section's level; tests added.
<!-- SECTION:FINAL_SUMMARY:END -->
