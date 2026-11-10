---
id: TASK-9
title: Image Artifacts and html Bundles
status: Done
assignee: []
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T00:57:00Z'
labels:
  - planned
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
- [x] #1 A directory without an entry page is rejected with exit 4
- [x] #2 Over 200 files or over the Revision cap is rejected, naming the limit
- [x] #3 `.svg` without `--kind diagram --format svg` is an error, not an image
- [x] #4 Media types outside the allowlist are rejected
- [x] #5 `get --out` writes files and prints the list; without `--out` it exits 2
- [x] #6 Editing a Bundle with one changed file stores one new blob
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-9. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented via file sets: src/bundle.rs (read_dir walk with symlink rejection and metadata-first limits, prepare: per-Kind allowlist, index.html entry, single html stored as index.html), Kind::media_type(path) allowlist, artifact::get_files, edit accepts directories for html. Tests in tests/cli.rs mod bundles cover all six criteria; cargo test, clippy, fmt pass. nix build not run.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Image and html Bundle Artifacts: submit/edit/get --out generalised to file sets with per-file media-type allowlist, 200-file and size limits, and blob sharing for unchanged files.
<!-- SECTION:FINAL_SUMMARY:END -->
