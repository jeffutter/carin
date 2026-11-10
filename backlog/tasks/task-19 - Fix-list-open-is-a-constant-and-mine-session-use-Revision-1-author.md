---
id: TASK-19
title: 'Fix: list --open is a constant and --mine/--session use Revision 1 author'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T00:26:00Z'
updated_date: '2026-11-10T00:27:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-4
priority: high
ordinal: 19000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In src/artifact.rs list filters: --open filters on a hard-coded constant (0 > 0) so it always returns nothing; --mine and --session match Revision 1 session/author while the AUTHOR column shows the latest Revision author, so list --mine hides rows whose AUTHOR column shows the caller. Make the filters consistent with the displayed data and implement --open for real (or reject it), with tests.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-19 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
list --session/--mine now match the latest Revision (matching the AUTHOR column). --open is rejected with Error::Usage (exit 2) until Threads exist; TASK-6 note added to implement it for real. Tests and docs/cli.md updated; cargo test and clippy pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed list filters to use the latest Revision and made --open a usage error instead of a silent constant-false filter; tests and docs updated.
<!-- SECTION:FINAL_SUMMARY:END -->
