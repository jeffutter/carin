---
id: TASK-21
title: 'Fix: edit --section splice does not validate replacement structure'
status: To Do
assignee: []
created_date: '2026-11-10T00:26:00Z'
labels:
  - review-fix
dependencies:
  - TASK-5
priority: high
ordinal: 21000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In src/sections.rs splice (around line 230): replacement only must start with a heading. A replacement with a different heading level changes the structure of following sections, and one ending in an unclosed code fence swallows the unchanged tail of the document. Validate that the replacement's leading heading level matches (or is explicitly allowed) and that fences are balanced, erroring otherwise; add tests.
<!-- SECTION:DESCRIPTION:END -->
