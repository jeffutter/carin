---
id: TASK-39
title: Dark mode support for the review UI and content
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T04:24:00Z'
updated_date: '2026-11-10T11:45:00Z'
labels:
  - ui
  - planned
dependencies: []
references:
  - docs/adr/0005-serve-untrusted-content-from-a-separate-origin.md
ordinal: 39000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The review UI is light-only, which is harsh for reviewers who work in dark environments. The UI should follow the viewer's color scheme. Agent content is served from the separate untrusted content origin (ADR 0005), so it cannot share UI styles and must be told the scheme some other way. Markdown is rendered by cairn itself, so it can ship its own dark style.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The cairn UI (list, Artifact page, comments) renders a dark theme when the viewer prefers dark (prefers-color-scheme), and stays light otherwise
- [x] #2 Markdown Artifacts render with a cairn-provided dark style under the same condition
- [x] #3 Embedded html/svg/image content gets the viewer color scheme where possible (for example via the iframe color-scheme / prefers-color-scheme inheritance), without weakening the two-origin isolation
- [x] #4 Where agent html cannot pick up the scheme, the content still displays legibly in dark mode (no unreadable text, no jarring white margin)
- [x] #5 Mermaid/svg diagrams remain legible in dark mode
- [x] #6 Tests or snapshots cover the added CSS/markup; docs/ is updated if the UI spec mentions theming
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-39 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Pure CSS in src/ui/style.css: dark variable override under prefers-color-scheme, color-scheme:light dark, hardcoded colours moved to variables. Markdown shares the variables. Iframe .frame gets color-scheme:light dark; images/svg/mermaid keep a light --media backing (AC4/5). serve.rs/CSP unchanged. Test in src/ui.rs guards CSS; docs/web-ui.md Theming section. Not visually checked in a browser; nextest 266 pass, clippy clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Dark theme via prefers-color-scheme in CSS, light backing for media, docs and CSS tests added.
<!-- SECTION:FINAL_SUMMARY:END -->
