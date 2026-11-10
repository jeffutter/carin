---
id: TASK-24
title: 'Fix: edit --section splice allows replacement injecting sibling Sections'
status: To Do
assignee: []
created_date: '2026-11-10T00:35:00Z'
labels:
  - review-fix
dependencies:
  - TASK-21
priority: high
ordinal: 24000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/sections.rs ~316: splice validation rejects only shallower headings or a mismatched first heading. A replacement containing '## A' followed by '## B' for 'edit --section A' passes and silently creates a new same-level Section, changing section paths and occurrence counts. Reject additional headings at or above the replaced Section's level within the replacement (after the first), with tests.
<!-- SECTION:DESCRIPTION:END -->
