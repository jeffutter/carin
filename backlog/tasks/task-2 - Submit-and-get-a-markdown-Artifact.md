---
id: TASK-2
title: Submit and get a markdown Artifact
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
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
- [ ] #1 `submit` of a `.md` file prints a ref and exit code 0; the database is created on first use
- [ ] #2 `get` prints the stored content; unknown id exits 3
- [ ] #3 Stdin submit requires `--kind` and `--name`, otherwise exit 2
- [ ] #4 Attribution (author, role, session) falls back as documented and is stored on the Revision
- [ ] #5 A file over the per-file limit is rejected with exit 4 naming the limit
- [ ] #6 Connection pragmas and `BEGIN IMMEDIATE` are applied in one place in core; tested
- [ ] #7 Round-trip integration tests via the real binary
<!-- AC:END -->
