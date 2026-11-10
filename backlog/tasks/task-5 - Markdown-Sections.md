---
id: TASK-5
title: Markdown Sections
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T00:24:00Z'
labels:
  - planned
dependencies:
  - TASK-3
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
An Agent reads and edits a markdown Artifact by heading Section to save tokens. `outline <id>[@rev]` prints every heading with its path and size. `get --section "A > B"` prints one Section (including children), optionally from a given Revision. `edit --section "A > B"` replaces a Section (replacement must start with a heading line) by splicing into the latest Revision and storing a new full Revision. Heading paths match exactly (case-sensitive, trimmed, inline markup stripped) with unique-suffix matching; duplicates are addressed `Name#2`; an ambiguous or missing path exits 4 listing candidates; an edit never guesses. Parsing uses pulldown-cmark, top-level headings only; front matter and the text before the first heading is an unaddressable preamble. Settle the CRLF build-time point with a test first.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `outline` lists paths and sizes, with no ids
- [x] #2 Suffix matching, `#n` disambiguation and the ambiguity error listing candidates with occurrence numbers
- [x] #3 Headings inside blockquotes or list items are not boundaries; setext headings and front matter handled
- [x] #4 `edit --section` splices correctly, makes a new full Revision, and fails if the path no longer matches
- [x] #5 `--section` on a non-markdown Artifact exits 4
- [x] #6 CRLF behaviour decided and covered by a test
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit titled 'Markdown Sections' (see git log, Task-Id: TASK-5). This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added src/sections.rs (pure: parse/outline/resolve/extract/splice over pulldown-cmark offsets; ENABLE_YAML_STYLE_METADATA_BLOCKS for front matter; container depth tracking so quote/list headings are not boundaries). artifact.rs: edit() now shares append_revision() with new section_edit(), so splice, base check and insert are one write tx; markdown_text() rejects non-markdown / non-UTF-8 (exit 4). CLI: outline, get --section, edit --section. Decisions: (1) CRLF parses like LF with exact byte ranges (tested first); splice normalizes a LF replacement to CRLF when the replaced Section is CRLF, never touching other bytes. (2) Deviation from plan: resolution is pure suffix matching with no exact-path priority, so a top-level 'Notes' and 'Top > Notes' are ambiguous for 'Notes' (never guesses); outline prints '#n' on any path not unique as a suffix so printed paths always resolve. Candidates listed as 'Notes#1 = Top > Notes'. Verified: fmt, clippy -D warnings, cargo test (40 unit + 29 integration), nix build. Docs updated: data-model.md, cli.md.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Markdown Sections shipped: outline, get --section[@rev], edit --section with atomic splice into a new full Revision, suffix/#n resolution, exit 4 on ambiguity/missing/non-markdown, CRLF decision recorded and tested.
<!-- SECTION:FINAL_SUMMARY:END -->
