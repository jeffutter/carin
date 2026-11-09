---
id: TASK-18
title: '`cairn skill`'
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
dependencies:
  - TASK-7
  - TASK-8
  - TASK-11
ordinal: 18000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`cairn skill` prints one complete SKILL.md to stdout: frontmatter (name, description, cairn version, hash of the generated text) plus Markdown built from hand-written prose files embedded in the binary and a runtime walk of the clap command tree for usage, long help, flags, env vars and defaults (`serve` and `skill` omitted). The exit-code table and JSON output contract come from the single Rust table shared with CLI error handling. The prose covers purpose, the submit / wait / edit-with-base-and-addresses workflow, `cairn:` embeds, Section path syntax, choosing a Kind, and asking the human to run `cairn serve`; it holds no flag details. `cairn skill --check <path>` compares an installed copy and exits 0 if current and 1 if stale. Nothing is written to disk.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Output has valid frontmatter with version and hash
- [ ] #2 Every `cairn ...` line in a prose code fence parses with the clap tree (test)
- [ ] #3 Every non-hidden command and argument has help text (test)
- [ ] #4 Full output is an insta snapshot; changing a flag fails the test until reviewed
- [ ] #5 `--check` exits 0 on a fresh copy and 1 on a stale one
- [ ] #6 Exit codes in the output match the shared table
<!-- AC:END -->
