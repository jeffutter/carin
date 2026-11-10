---
id: TASK-29
title: 'Fix: submit default name regresses for stdin and nonexistent paths'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T00:59:00Z'
updated_date: '2026-11-10T01:03:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-9
priority: high
ordinal: 29000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/lib.rs (~line 104): default Artifact name now comes from std::fs::canonicalize(path).ok(). For stdin ('-') or a path that does not exist, canonicalize fails and the command reports a misleading usage error 'no file name in ...' instead of using the file name as before (or the real 'cannot read' error for missing files). Fix: derive the name from Path::file_name of the given path (canonicalize only for '.'/'..' style paths that lack a file name), keep a clear error for stdin without --name, and ensure missing files report the read error. Add tests for stdin, nonexistent path, and '.'.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-29. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Name derivation in src/lib.rs Submit now uses Path::file_name first, canonicalize only for '.'/'..'. Missing files reach read_input error. Tests added in tests/cli.rs roundtrip mod (stdin, missing, '.'). cargo test/clippy/fmt pass (in nix develop).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed submit default-name regression; missing files report the read error, stdin keeps its usage error, '.' uses the directory basename. Tests added.
<!-- SECTION:FINAL_SUMMARY:END -->
