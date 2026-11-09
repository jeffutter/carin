---
id: TASK-17
title: 'JSON API: writes and long poll'
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
dependencies:
  - TASK-16
  - TASK-7
  - TASK-11
ordinal: 17000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Outside tools can create and change things over the API. Create and edit take multipart form data: a JSON meta part (kind, format, name, role, author, session, addresses, base_revision, and on create the project fields) plus one part per file; a Section edit is a PUT with a section query and one markdown part. Delete, comment add and reply, and thread resolve and reopen complete the route table. Attribution is body fields only, defaulted as in the CLI. All core limits and embed validation apply, with errors mapped as in the reads ticket (stale base gives 409). `GET` comments with `since` and `wait` long-polls (polling the database every 500 ms, default 60 s, capped); a timeout is a plain 200 with an empty list and the same `highest_comment_id`. The OpenAPI document and snapshot are updated.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Create, edit, section edit and delete work end to end for every Kind
- [ ] #2 A stale `base_revision` returns 409 naming the latest Revision and its author
- [ ] #3 Over-limit, bad Kind and bad embed return 422 with the stable code
- [ ] #4 Long poll returns early on a new Comment and returns 200 empty on timeout
- [ ] #5 Comment add, reply, resolve and reopen match CLI behaviour
- [ ] #6 OpenAPI snapshot updated and passing
<!-- AC:END -->
