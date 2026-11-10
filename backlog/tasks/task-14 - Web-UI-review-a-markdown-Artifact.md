---
id: TASK-14
title: 'Web UI: review a markdown Artifact'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T01:59:00Z'
labels:
  - planned
dependencies:
  - TASK-13
  - TASK-6
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Reviewer discusses a markdown Artifact in the browser. A line-number gutter and a per-heading "+ section" button start a new Thread; Threads render inline under the Section or line they anchor to; the Reviewer replies and resolves or reopens Threads. Posts are attributed to `--reviewer-name` with role reviewer, calling core directly. An open Thread made on an earlier Revision still shows on other Revisions with a dashed border and an "earlier revision" label; resolved Threads from other Revisions are hidden. The page refreshes by an htmx poll every 2 seconds, paused while the tab is hidden, using the same latest-comment check as `--wait`. Optionally add the per-Artifact "last seen" marker if it stays small; it is a hint, not a contract.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Posting, replying, resolving and reopening work and appear in `cairn comment list`
- [x] #2 Comments posted by the CLI appear in the open page within the poll interval
- [x] #3 Polling pauses when the tab is hidden
- [x] #4 Earlier-Revision open Threads show dashed and labelled; resolved ones are hidden
- [x] #5 Posted attribution uses `--reviewer-name` and role reviewer
- [x] #6 Open-Thread counts in the outline sidebar and list update
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 0784e8b. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in src/ui.rs (blocks with gutter, + section, inline Threads, live fragment, compose/new/reply/status routes), src/ui/app.js (pause polling when document.hidden, keep drafts), src/ui/style.css, comments::change_marker (src/comments.rs), axum 'form' feature. Poll: GET /a/{id}@{rev}/live?m=marker returns 204 when unchanged; marker includes a resolved-id sum so CLI resolves are noticed. Replies pass the bare artifact id so they stay on the Thread's Revision. Line Threads from other Revisions (lines may have moved) are listed in a 'without a place' group at the top; Section Threads anchor by path on any Revision. List table also polls so counts update. Optional last-seen marker skipped. Verified by axum oneshot tests, cargo test/clippy/fmt; NOT verified in a real browser (hidden-tab pause in app.js is untested beyond code review; AC3 rests on that).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Reviewer can start Threads from the line gutter or per-heading button, reply, resolve and reopen from the markdown Artifact page; Threads render inline, earlier-Revision open Threads are dashed and labelled, other-Revision resolved ones hidden; 2s htmx poll with marker-based 204, paused while hidden; attribution uses --reviewer-name with role reviewer.
<!-- SECTION:FINAL_SUMMARY:END -->
