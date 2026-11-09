---
id: TASK-9
title: Image Artifacts and html Bundles
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
dependencies:
  - TASK-3
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Agents can submit images (png, jpeg, gif, webp; never svg) and html Artifacts, which are Bundles: a required entry page plus assets at relative paths, from a single `.html` file or a directory. Kind comes from `--kind` or the extension, never content sniffing; ambiguous input is an error. Media types are checked against an allowlist and stored per file. Bundle limits: 200 files and the per-Revision cap. `get` for these Kinds needs `--out <dir>` and prints a JSON file list. Editing a Bundle resubmits the whole Bundle; unchanged files share blobs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A directory without an entry page is rejected with exit 4
- [ ] #2 Over 200 files or over the Revision cap is rejected, naming the limit
- [ ] #3 `.svg` without `--kind diagram --format svg` is an error, not an image
- [ ] #4 Media types outside the allowlist are rejected
- [ ] #5 `get --out` writes files and prints the list; without `--out` it exits 2
- [ ] #6 Editing a Bundle with one changed file stores one new blob
<!-- AC:END -->
