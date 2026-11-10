---
id: TASK-27
title: >-
  Fix: --project with relative path is parsed as a name; comment list silently
  drops Project scope flags
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T00:48:00Z'
updated_date: '2026-11-10T00:54:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-8
priority: high
ordinal: 27000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/scope.rs ~91: --project treats any non-absolute value as a bare name, so '.' or '../repo' is misinterpreted (submit errors misleadingly, list --project . matches nothing). Resolve relative paths (values containing a separator or starting with '.') as paths. Also src/comments.rs ~466: comments::list ignores --project-root/--all-projects when --session or a target is explicit, with no error; reject the combination or apply the scope. Dedupe the root normalization used by select and ScopeFilter::build into one helper. Add tests.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-27 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added scope::normalize_root (relative paths resolved against cwd, canonicalize, lexical fallback) used by select and ScopeFilter::build (now takes cwd). --project values that are absolute, contain '/', or start with '.' are paths. comment list now rejects --all-projects/--all-worktrees/--project-root with an artifact id or explicit --session. Tests added in src/scope.rs and tests/cli.rs; cargo test, clippy, fmt pass (via nix develop).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Relative --project paths now resolve as paths; comment list errors on ignored scope flags; root normalization deduped into one helper.
<!-- SECTION:FINAL_SUMMARY:END -->
