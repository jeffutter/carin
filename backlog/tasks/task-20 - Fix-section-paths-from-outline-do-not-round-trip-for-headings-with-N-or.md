---
id: TASK-20
title: 'Fix: section paths from outline do not round-trip for headings with #N or >'
status: To Do
assignee: []
created_date: '2026-11-10T00:26:00Z'
labels:
  - review-fix
dependencies:
  - TASK-5
priority: high
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In src/sections.rs (around line 163): a heading like '## Issue #12' is printed by outline as 'Issue #12' but parsed by --section as name 'Issue' occurrence 12; headings containing '>' are split as path separators. get --section / edit --section cannot address them. Also heading text drops inline HTML, so '## Intro <!-- x -->' and '## Intro' collide. Add escaping in display_path and the parser (or an unambiguous id form) so every path printed by outline resolves to the same section; add tests.
<!-- SECTION:DESCRIPTION:END -->
