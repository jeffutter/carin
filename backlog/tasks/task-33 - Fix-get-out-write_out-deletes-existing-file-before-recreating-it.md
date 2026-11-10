---
id: TASK-33
title: 'Fix: get --out write_out deletes existing file before recreating it'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T01:13:00Z'
updated_date: '2026-11-10T01:21:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-28
priority: high
ordinal: 33000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/lib.rs ~line 346: write_out removes the existing file then create_new/write_all. If the later step fails (ENOSPC, EACCES, raced symlink) the user's previous file is lost and the output dir is left half-written. Also create_dir_all can follow a symlink raced in after the phase-1 check. Write to a temp file and atomically rename.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-33 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
write_out phase 2 now creates dirs component-wise with create_dir (refusing symlinks/non-dirs) and writes via .<name>.cairn-tmp-<random> temp + rename. Added tests for rerun refresh with no temp files and failed write keeping old file. cargo test, clippy, fmt clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
get --out now writes atomically via temp file and rename, closing the delete-then-recreate data-loss window and the create_dir_all symlink race.
<!-- SECTION:FINAL_SUMMARY:END -->
