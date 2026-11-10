---
id: TASK-21
title: 'Fix: edit --section splice does not validate replacement structure'
status: Done
assignee: []
created_date: '2026-11-10T00:26:00Z'
updated_date: '2026-11-10T00:30:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-5
priority: high
ordinal: 21000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In src/sections.rs splice (around line 230): replacement only must start with a heading. A replacement with a different heading level changes the structure of following sections, and one ending in an unclosed code fence swallows the unchanged tail of the document. Validate that the replacement's leading heading level matches (or is explicitly allowed) and that fences are balanced, erroring otherwise; add tests.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-21. This plan is superseded; the final summary describes what landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added level check, shallower-heading check, and tail-structure check (re-parse result, compare headings after splice) in splice(); tests and docs updated. cargo test/clippy/fmt pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
edit --section now rejects replacements that change heading level, contain shallower headings, or alter the structure of the following document (unclosed fences).
<!-- SECTION:FINAL_SUMMARY:END -->
