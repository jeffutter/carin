---
id: TASK-28
title: >-
  Fix: get --out follows symlinks and overwrites existing files in the output
  directory
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T00:59:00Z'
updated_date: '2026-11-10T01:01:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-9
priority: high
ordinal: 28000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/lib.rs write_out (~line 296) writes bundle files into the --out directory without checking what already exists. safe_path validates only the relative path. If --out already holds a symlink (e.g. index.html -> ~/.bashrc) the write follows it and overwrites the target. A Bundle containing both a.js and a.js/b.css fails mid-write leaving a partial tree. Fix: refuse to write through symlinks (O_NOFOLLOW / create_new or symlink_metadata check), detect file/dir clashes up front before writing anything, and decide/document overwrite behavior for existing regular files. Add tests.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by this commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
write_out is now two-phase: validate (safe_path, file/dir clash within the set, symlink_metadata walk below --out, existing dir at file path), then write via remove_file + create_new (O_EXCL, no libc needed, never follows a raced symlink). Existing regular files are overwritten; --out itself may be a symlink. Documented in docs/cli.md. Six tests added in tests/cli.rs (bundles module). Submit-side clash was not probed; clash test crafts a revision_files row directly.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
get --out refuses symlinks and file/dir clashes before writing anything and overwrites existing regular files; tests and docs added.
<!-- SECTION:FINAL_SUMMARY:END -->
