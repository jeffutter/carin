---
id: TASK-2
title: Submit and get a markdown Artifact
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T00:15:00Z'
labels:
  - planned
dependencies:
  - TASK-1
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
An Agent runs `submit` on a markdown file (or stdin with `--kind` and `--name`) and gets back `abc123@1`; `get abc123` prints the content. This lays the core: the schema for Artifacts, Revisions, revision files and content-addressed blobs, connection setup per ADR 0007 (WAL, immediate write transactions, foreign keys, busy timeout, `auto_vacuum=FULL`), the database and its directory created on first use, short random typeable ids, Author/role/Session attribution, Kind inference from extension, and the per-file size limit naming the limit when exceeded. Only the core opens the database. Output is JSON per the output contract, except `get`, which prints raw content.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `submit` of a `.md` file prints a ref and exit code 0; the database is created on first use
- [x] #2 `get` prints the stored content; unknown id exits 3
- [x] #3 Stdin submit requires `--kind` and `--name`, otherwise exit 2
- [x] #4 Attribution (author, role, session) falls back as documented and is stored on the Revision
- [x] #5 A file over the per-file limit is rejected with exit 4 naming the limit
- [x] #6 Connection pragmas and `BEGIN IMMEDIATE` are applied in one place in core; tested
- [x] #7 Round-trip integration tests via the real binary
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by b6c1301a5d41540b037f3d426c15733d046daba0. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added src/db.rs (only Connection::open; pragmas, user_version migration v1, write_tx with BEGIN IMMEDIATE), ids.rs, kind.rs, limits.rs, artifact.rs (submit/get), submit and get CLI verbs. Verified in nix develop: cargo fmt --check, clippy -D warnings, cargo test (23 unit, 9 integration) all pass. Plain cargo outside nix develop lacks cc. nix build not run.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
submit/get for markdown Artifacts with sqlite core, attribution, size limits, and round-trip tests via the real binary.
<!-- SECTION:FINAL_SUMMARY:END -->
