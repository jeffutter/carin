---
id: TASK-23
title: 'Fix: comment add with both --section and --lines stores inconsistent locator'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T00:35:00Z'
updated_date: '2026-11-10T00:39:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-6
priority: high
ordinal: 23000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/comments.rs ~278: when both --section and --lines are given, section_path is kept but line_start/line_end/quoted_text are overwritten by the line range, so the comment claims one location and quotes another. Reject mixed locators with a usage error (or define one precedence for the whole locator) and add a test.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-23. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added Usage error in src/comments.rs when both --lines and --section are given; test in tests/cli.rs (lines_store_quote_and_section_stores_path). cargo test and clippy pass (via nix develop).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
comment add rejects mixed --lines/--section with exit 2.
<!-- SECTION:FINAL_SUMMARY:END -->
