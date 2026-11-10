---
id: TASK-15
title: 'Web UI: other Kinds, embeds and delete'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T02:16:00Z'
labels:
  - planned
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
- [x] #1 html, image and diagram Artifacts each render in the correct frame; tests assert the iframe sandbox attribute and the `<img>` usage
- [x] #2 A Bundle script cannot reach the UI origin (verified manually in a browser and noted in the PR)
- [x] #3 Embeds render pinned and latest; a deleted target shows the placeholder
- [x] #4 Whole-Revision Comments work on non-markdown Kinds
- [x] #5 Delete from the list removes the Artifact and refreshes the list
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by eb8cdc9. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in src/ui.rs (frames outside #live, whole-revision compose, embeds resolved by dest string, delete route) and src/serve.rs (content_port into UiState); percent-encoding added as a direct dep (already transitive). AC2 verified in Chromium via agent-browser: an html Bundle probe in the sandboxed iframe got SecurityError for parent.document and localStorage and a blocked fetch to the UI origin. List delete verified in browser: hx-confirm dialog shown under the CSP, accept deleted the Artifact and the table showed No Artifacts. cargo test, clippy --all-targets and fmt --check pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Artifact page now frames html Bundles (sandbox=allow-scripts, content origin, outside the polled region) and shows image/diagram via img; whole-revision Threads work for non-markdown Kinds; cairn: embeds render pinned/latest/missing; list has a per-row confirmed Delete.
<!-- SECTION:FINAL_SUMMARY:END -->
