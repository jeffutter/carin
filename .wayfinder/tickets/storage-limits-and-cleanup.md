---
title: Storage limits and cleanup
type: grilling
status: open
assignee:
blocked_by: []
---

## Question

How do old Artifacts and Revisions get cleaned up and how is growth bounded? Decide whether Revisions can be pruned (given immutable snapshots and content-addressed BLOBs, ADR 0003), blob garbage collection after delete, any retention or archive concept, and what the CLI exposes (for example a prune verb). Fixed per-Artifact size limits already exist; this covers total growth.
