---
id: TASK-34
title: 'Fix: Mermaid render runs inside the SQLite write transaction on edit'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T01:13:00Z'
updated_date: '2026-11-10T01:35:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-10
priority: high
ordinal: 34000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/artifact.rs ~line 182: edit calls prepare (Mermaid render, up to RENDER_TIMEOUT 5s) inside the append_revision write transaction, while submit renders before write_tx. With busy_timeout 5000ms a slow render can make concurrent writers fail with SQLITE_BUSY. Validate outside the lock and re-check the format inside the transaction.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by this commit. Plan superseded; see final summary.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
edit now reads an EditPlan (kind, format, single path) and runs bundle::prepare before write_tx; the transaction re-reads the plan and returns Conflict if it changed. Added two tests in src/artifact.rs. cargo test, clippy, fmt clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Moved the Mermaid render in artifact::edit outside the SQLite write transaction, with an in-transaction re-check of the plan inputs.
<!-- SECTION:FINAL_SUMMARY:END -->
