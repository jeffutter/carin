---
id: TASK-11
title: Markdown embeds (`cairn:` references)
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
dependencies:
  - TASK-9
  - TASK-10
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A markdown Artifact can show an image or diagram Artifact by reference, latest or pinned to a Revision. At submit and edit, each reference is checked: a missing Artifact or Revision, or a target that is not an image or diagram, is rejected with exit 4. Each unpinned reference is resolved to the target's current Revision and recorded in the embeds side table; the stored markdown text is never rewritten. Other image URLs (http, data, relative) are not fetched and will show as alt text. Deleting an embedded Artifact is allowed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Latest and pinned references both validate and are recorded with the resolved Revision
- [ ] #2 Missing target, missing Revision, and a markdown or html target each exit 4 with a clear message
- [ ] #3 Embeds are re-resolved on each new markdown Revision
- [ ] #4 Deleting an embedded Artifact succeeds
- [ ] #5 Core exposes the embeds of a Revision for later use by the UI
<!-- AC:END -->
