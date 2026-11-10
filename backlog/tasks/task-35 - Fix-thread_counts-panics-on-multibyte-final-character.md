---
id: TASK-35
title: 'Fix: thread_counts panics on multibyte final character'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T02:01:00Z'
updated_date: '2026-11-10T02:03:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-14
priority: high
ordinal: 35000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/ui.rs thread_counts calls line_of(text, h.end - 1), slicing at a byte offset that can fall inside a multibyte char. A markdown Revision whose last heading Section ends at EOF with no trailing newline and a multibyte final char (e.g. '# T\nhello 世') panics inside spawn_blocking; the Artifact page and every poll return 500 until a new Revision is stored. Align to a char boundary or use the line_starts table as render_blocks does. Add a regression test.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-35 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
line_of in src/ui.rs now counts newline bytes instead of slicing the str, so mid-char offsets no longer panic. Added tests line_of_mid_char_offset_does_not_panic and thread_counts_multibyte_final_char_without_newline. cargo test --lib and clippy pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed thread_counts panic on multibyte final char by making line_of byte-based; regression tests added.
<!-- SECTION:FINAL_SUMMARY:END -->
