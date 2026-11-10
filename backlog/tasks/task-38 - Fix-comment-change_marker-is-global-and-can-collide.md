---
id: TASK-38
title: 'Fix: comment change_marker is global and can collide'
status: To Do
assignee: []
created_date: '2026-11-10T02:01:00Z'
labels:
  - review-fix
dependencies:
  - TASK-14
priority: high
ordinal: 38000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/comments.rs change_marker combines the table-wide highest comment id with a sum of resolved root ids. Not scoped to the Artifact, so any comment elsewhere redraws every open page; and the sum can collide (reopen threads 1,2 while resolving 3), leaving the page stale. Scope to the artifact and use a collision-free marker. Also in src/ui/app.js the htmx:responseError handler alerts on failed background polls; skip elements with data-poll.
<!-- SECTION:DESCRIPTION:END -->
