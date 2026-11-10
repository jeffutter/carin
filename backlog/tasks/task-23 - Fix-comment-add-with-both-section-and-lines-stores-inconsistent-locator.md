---
id: TASK-23
title: 'Fix: comment add with both --section and --lines stores inconsistent locator'
status: To Do
assignee: []
created_date: '2026-11-10T00:35:00Z'
labels:
  - review-fix
dependencies:
  - TASK-6
priority: high
ordinal: 23000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/comments.rs ~278: when both --section and --lines are given, section_path is kept but line_start/line_end/quoted_text are overwritten by the line range, so the comment claims one location and quotes another. Reject mixed locators with a usage error (or define one precedence for the whole locator) and add a test.
<!-- SECTION:DESCRIPTION:END -->
