---
id: TASK-6
title: Comments and Threads (CLI)
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T00:34:00Z'
labels:
  - planned
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
- [x] #1 Add with line range stores the quoted text; with section stores the resolved path
- [x] #2 Reply attaches to the Thread's Revision and rejects a locator
- [x] #3 Resolve and reopen work for either role
- [x] #4 Default list includes open stale Threads; `--all` and `--since` behave as documented
- [x] #5 `highest_comment_id` is present on every read and is monotonic across the database
- [x] #6 A Comment on a Revision that is not the latest is accepted and reported `stale`
- [x] #7 The open Thread count in `list` (ticket 04) is now populated
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 74e3ea0. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
From TASK-19: list --open is currently rejected with a usage error (src/artifact.rs list). Implement it against the real threads table and replace the rejection; open_threads column is still constant 0.

Added src/comments.rs (add, set_status, list), schema v2 (comments table, AUTOINCREMENT, cascade from revisions), comment subcommands in cli/lib, open_threads count and list --open in artifact.rs. highest_comment_id reads sqlite_sequence rather than MAX(id) so deleting an Artifact never lowers it. --since returns whole Threads and still obeys the open/--all rule. Session scope uses ctx.session (latest Revision's session). Added MAX_COMMENT_BYTES (64 KiB). Docs updated. cargo test, clippy -D warnings, fmt --check all pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Comments and Threads CLI: comment add/resolve/reopen/list with locators, stale reporting, monotonic highest_comment_id, populated open_threads and list --open. Tests in tests/cli.rs (comments module) and src/db.rs (migration).
<!-- SECTION:FINAL_SUMMARY:END -->
