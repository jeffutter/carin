---
id: TASK-11
title: Markdown embeds (`cairn:` references)
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T01:11:00Z'
labels:
  - planned
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
- [x] #1 Latest and pinned references both validate and are recorded with the resolved Revision
- [x] #2 Missing target, missing Revision, and a markdown or html target each exit 4 with a clear message
- [x] #3 Embeds are re-resolved on each new markdown Revision
- [x] #4 Deleting an embedded Artifact succeeds
- [x] #5 Core exposes the embeds of a Revision for later use by the UI
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 5e151ee. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added src/embeds.rs (extract via pulldown_cmark, record, for_revision), SCHEMA_V3 embeds table with no FK on target, hooked into submit and append_revision (covers edit and section edit) inside the write tx. Tests: unit tests in embeds.rs and tests/embeds.rs. Docs updated in data-model.md and cli.md.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Markdown embeds validated and recorded per Revision with resolved target Revision; bad targets exit 4 with rollback; deletion of targets allowed; embeds::for_revision exposes them.
<!-- SECTION:FINAL_SUMMARY:END -->
