---
id: TASK-40
title: Full-width, full-height content view with header-triggered general comment
status: To Do
assignee: []
created_date: '2026-11-10T04:24:00Z'
updated_date: '2026-11-10T04:24:00Z'
labels:
  - ui
dependencies: []
references:
  - docs/adr/0005-serve-untrusted-content-from-a-separate-origin.md
ordinal: 40000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
On the Artifact page, content (at least html, possibly markdown too - unverified) appears inside a framed box that does not use the available space. Reviewers want the content to fill the page width, and ideally the height below the cairn header, so the content is the focus. The general comment box currently sits below the content, which fights a full-height layout; it should move out of the flow into a modal or bubble opened from the header.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Embedded content takes at least the full width of the page
- [ ] #2 Embedded content takes the full remaining viewport height below the cairn header, with no outer frame/padding around it (check whether markdown is affected and fix it if so)
- [ ] #3 The general comment form is removed from below the content and opens as a modal or popover from a control in the header
- [ ] #4 Submitting a general comment from the modal behaves as before (htmx, thread appears, modal closes)
- [ ] #5 Layout works for each Kind (markdown, html bundle, image, mermaid/svg) and does not break section/line commenting
- [ ] #6 Tests updated; docs/ updated if the UI spec describes the layout
<!-- AC:END -->
