---
id: TASK-30
title: 'Fix: dangling embed blocks all later edits of a markdown artifact'
status: Done
assignee: []
created_date: '2026-11-10T01:13:00Z'
updated_date: '2026-11-10T01:15:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-11
priority: high
ordinal: 30000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
src/embeds.rs record() re-validates every cairn: embed on each markdown write. If an embedded artifact is deleted (allowed, leaves a dangling row) or a pinned revision is gone, any later edit/section_edit of the markdown, even of an unrelated section, fails with 'embed cairn:ID: no such artifact'. Only validate newly added or changed references, or tolerate already-recorded embeds.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-30 commit. This plan is superseded; the final summary describes what landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in src/embeds.rs record(): takes prev_rev; refs already recorded by the previous Revision (same target, pinned-ness, @N) are carried forward when target/revision is gone. New refs still fully validated. artifact.rs passes None on create, Some(latest) on edit. Test dangling_embed_does_not_block_later_edits in tests/embeds.rs covers full edit, section edit, new-ref rejection. cargo test, clippy, fmt pass (via nix develop). Pinned-revision-removed case not tested separately (revisions cannot be removed).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Dangling cairn: embeds recorded by the previous Revision no longer block later markdown edits; new or changed refs are still rejected.
<!-- SECTION:FINAL_SUMMARY:END -->
