---
id: TASK-15
title: 'Web UI: other Kinds, embeds and delete'
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
dependencies:
  - TASK-13
  - TASK-11
  - TASK-14
ordinal: 15000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Artifact page works for every Kind. html shows in an iframe from the content port with the script-only sandbox token and never same-origin; images and diagrams show only via `<img>` from the content port. These pages use the same header and revision dropdown, and Comments are on the whole Revision with the same Thread UI. Markdown `cairn:` embeds render as `<img>` from the content origin, pinned or latest as recorded; a deleted target shows a missing-image placeholder; other URL schemes show alt text. The list gets its per-row delete action.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 html, image and diagram Artifacts each render in the correct frame; tests assert the iframe sandbox attribute and the `<img>` usage
- [ ] #2 A Bundle script cannot reach the UI origin (verified manually in a browser and noted in the PR)
- [ ] #3 Embeds render pinned and latest; a deleted target shows the placeholder
- [ ] #4 Whole-Revision Comments work on non-markdown Kinds
- [ ] #5 Delete from the list removes the Artifact and refreshes the list
<!-- AC:END -->
