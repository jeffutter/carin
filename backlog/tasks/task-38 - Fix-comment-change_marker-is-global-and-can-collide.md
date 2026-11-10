---
id: TASK-38
title: 'Fix: comment change_marker is global and can collide'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T02:01:00Z'
updated_date: '2026-11-10T02:09:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-14
priority: high
ordinal: 38000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/comments.rs change_marker combines the table-wide highest comment id with a sum of resolved root ids. Not scoped to the Artifact, so any comment elsewhere redraws every open page; and the sum can collide (reopen threads 1,2 while resolving 3), leaving the page stale. Scope to the artifact and use a collision-free marker. Also in src/ui/app.js the htmx:responseError handler alerts on failed background polls; skip elements with data-poll.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-38 commit. This plan is superseded; the final summary describes what landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
change_marker now scoped to the artifact (MAX(id) for its comments) with an exact comma-joined list of resolved root ids. highest_comment_id kept for --wait. app.js responseError skips data-poll elements. Added 3 unit tests in comments.rs. Comma in m= query param is a legal query character; ui.rs uses unchanged. cargo test and clippy pass. Manual check of stopped-server polling not done.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Scoped change_marker per Artifact and made it collision-free; failed background polls no longer alert.
<!-- SECTION:FINAL_SUMMARY:END -->
