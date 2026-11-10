---
id: TASK-6
title: Comments and Threads (CLI)
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T00:27:00Z'
labels: []
dependencies:
  - TASK-5
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reviewer and Agent discussion from the command line. `comment add <id>[@rev] <body|->` with `--lines a-b` (quoted text stored) and, for markdown, `--section "A > B"` resolved against the Comment's own Revision. `--reply <comment-id>` replies in a one-level Thread with no locator. `comment resolve` and `reopen` toggle the Thread flag, the only mutable part of a Comment. `comment list [<id>]` shows open Threads on any Revision for the caller's Artifacts (those of the Session when one is set; otherwise all, until scoping lands in ticket 08), `--all` includes resolved, `--since <id>` returns only newer Comments, and every read returns `highest_comment_id`. Each Thread reports `revision`, `latest_revision`, a derived `stale` flag, the quoted text and the locator. A Comment on an older Revision is never refused. Comments are never edited or deleted, and anchors are never rewritten.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Add with line range stores the quoted text; with section stores the resolved path
- [ ] #2 Reply attaches to the Thread's Revision and rejects a locator
- [ ] #3 Resolve and reopen work for either role
- [ ] #4 Default list includes open stale Threads; `--all` and `--since` behave as documented
- [ ] #5 `highest_comment_id` is present on every read and is monotonic across the database
- [ ] #6 A Comment on a Revision that is not the latest is accepted and reported `stale`
- [ ] #7 The open Thread count in `list` (ticket 04) is now populated
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
From TASK-19: list --open is currently rejected with a usage error (src/artifact.rs list). Implement it against the real threads table and replace the rejection; open_threads column is still constant 0.
<!-- SECTION:NOTES:END -->
