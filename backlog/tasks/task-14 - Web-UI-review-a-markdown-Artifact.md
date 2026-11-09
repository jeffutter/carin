---
id: TASK-14
title: 'Web UI: review a markdown Artifact'
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
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
- [ ] #1 Posting, replying, resolving and reopening work and appear in `cairn comment list`
- [ ] #2 Comments posted by the CLI appear in the open page within the poll interval
- [ ] #3 Polling pauses when the tab is hidden
- [ ] #4 Earlier-Revision open Threads show dashed and labelled; resolved ones are hidden
- [ ] #5 Posted attribution uses `--reviewer-name` and role reviewer
- [ ] #6 Open-Thread counts in the outline sidebar and list update
<!-- AC:END -->
