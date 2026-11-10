---
id: TASK-31
title: 'Fix: --format forces any submitted file to be a diagram'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T01:13:00Z'
updated_date: '2026-11-10T01:16:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-10
priority: high
ordinal: 31000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/lib.rs ~line 100: arm (None, Some(_), false) => Kind::Diagram matches before extension inference, so 'cairn submit notes.md --format mermaid' or 'logo.png --format svg' feeds the file to the diagram validator (confusing error or wrongly stored diagram). Run extension inference first and reject --format for non-diagram kinds with a clear error.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-31 commit. This plan is superseded; the final summary describes what landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Submit now resolves Kind by extension first (x.svg with --format still falls back to Diagram) and rejects --format for non-diagram Kinds with exit 4. Tests added in tests/cli.rs. cargo test, clippy, fmt pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
--format no longer forces a diagram; non-diagram kinds are rejected with a clear error, svg/mmd with --format still submit.
<!-- SECTION:FINAL_SUMMARY:END -->
