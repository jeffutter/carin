---
id: TASK-20
title: 'Fix: section paths from outline do not round-trip for headings with #N or >'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T00:26:00Z'
updated_date: '2026-11-10T00:29:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-5
priority: high
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In src/sections.rs (around line 163): a heading like '## Issue #12' is printed by outline as 'Issue #12' but parsed by --section as name 'Issue' occurrence 12; headings containing '>' are split as path separators. get --section / edit --section cannot address them. Also heading text drops inline HTML, so '## Intro <!-- x -->' and '## Intro' collide. Add escaping in display_path and the parser (or an unambiguous id form) so every path printed by outline resolves to the same section; add tests.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-20. This plan is superseded; the final summary describes what landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added escape_segment and a char scanner (split_query) in src/sections.rs; display_path and resolve errors use escapes; docs/cli.md updated; round-trip, #N, > and splice tests added. cargo test, clippy, fmt pass. HTML-comment collision already resolved via Intro#1/#2; locked in by round-trip test.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Section paths printed by outline now backslash-escape \, > and #digits, and resolve parses the same grammar, so every printed path round-trips.
<!-- SECTION:FINAL_SUMMARY:END -->
