---
title: Storage limits and cleanup
type: grilling
status: closed
assignee: jeffutter
blocked_by: []
---

## Question

How do old Artifacts and Revisions get cleaned up and how is growth bounded? Decide whether Revisions can be pruned (given immutable snapshots and content-addressed BLOBs, ADR 0003), blob garbage collection after delete, any retention or archive concept, and what the CLI exposes (for example a prune verb). Fixed per-Artifact size limits already exist; this covers total growth.

## Resolution

- **No Revision pruning.** Revisions are never deleted individually; the only removal is whole-Artifact hard delete. Comment anchors, pinned embeds (`abc123@2`) and per-Artifact numbering all depend on Revisions staying put, and content-addressed blobs already make repeated edits cheap.
- **Blob cleanup**: `delete` removes the blobs no remaining file row references, in its own `BEGIN IMMEDIATE` transaction (ADR 0007). The database is created with `auto_vacuum=FULL` on first use so the file shrinks on commit. No `gc` or `compact` verb, no background sweep.
- **No retention, archive, expiry or total size cap.** Only the per-file, per-Revision and per-bundle limits of ADR 0003 apply. The Reviewer is the only one who deletes; a silent expiry or a refused submit near a cap would hurt an Agent more than a large file.
- **Making cleanup practical**: `list` (CLI, API and web UI) shows each Artifact's stored bytes and last-updated time; the CLI `list` takes `--older-than <duration>`; `delete` accepts several ids and still requires `--yes`; the web UI list has the same size column and a delete action. No new verb, no whole-database stats command in v1.
