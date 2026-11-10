---
id: TASK-22
title: 'Fix: comment list default scope hides Threads after another Session edits'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T00:35:00Z'
updated_date: '2026-11-10T00:36:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-6
priority: high
ordinal: 22000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/comments.rs ~342: comment list without an explicit artifact scopes by ctx.session (includes CLAUDE_CODE_SESSION_ID fallback) using the latest Revision's session. Once another Session makes a new Revision, the original Session no longer sees its open Threads. Inconsistent with list --session, which uses session_explicit. Scope by Artifacts the Session created/touched in any Revision, or use session_explicit, and add a test.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-22. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
comment list now scopes by ctx.session_explicit and matches any Revision by that Session (src/comments.rs list). Tests added in tests/cli.rs (other-Session edit, env fallback). docs/cli.md updated. cargo test, clippy, fmt pass via nix develop.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed comment list session scope: explicit --session only, any Revision.
<!-- SECTION:FINAL_SUMMARY:END -->
