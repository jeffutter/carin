---
id: TASK-40
title: Full-width, full-height content view with header-triggered general comment
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T04:24:00Z'
updated_date: '2026-11-10T11:48:00Z'
labels:
  - ui
  - planned
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
- [x] #1 Embedded content takes at least the full width of the page
- [x] #2 Embedded content takes the full remaining viewport height below the cairn header, with no outer frame/padding around it (check whether markdown is affected and fix it if so)
- [x] #3 The general comment form is removed from below the content and opens as a modal or popover from a control in the header
- [x] #4 Submitting a general comment from the modal behaves as before (htmx, thread appears, modal closes)
- [x] #5 Layout works for each Kind (markdown, html bundle, image, mermaid/svg) and does not break section/line commenting
- [x] #6 Tests updated; docs/ updated if the UI spec describes the layout
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by a5c5437. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented: header.top takes header_extra (Comment on this revision button + Comments link, Whole kinds only); native dialog#whole-dialog drawn in #page outside #live so polling cannot wipe it; app.js handles data-open/data-close and closes+resets on successful htmx request; removed in-flow button, #slot-whole and compose ?whole=1 (ComposeParams.whole). .fullpage wrapper + flex body so frame/imgframe fill width and remaining height (min 60vh), no border/padding. Markdown unaffected (keeps reading layout, no whole comment). Header omits a thread count (header is outside #page, would go stale on revision change). Tests: ui.rs page_markup unit tests, serve.rs whole_revision test updated; clippy and nextest pass (268). NOT verified in a real browser (layout, modal open/submit/close, dark mode, phone width); look at these manually.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Whole-Revision kinds (html/image/mermaid) now render full-bleed, filling width and remaining height; the general comment form moved to a header-opened modal dialog posting whole=1 via htmx. Markdown layout left unchanged. Docs and tests updated; browser visual check pending.
<!-- SECTION:FINAL_SUMMARY:END -->
