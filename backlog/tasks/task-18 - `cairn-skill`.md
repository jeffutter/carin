---
id: TASK-18
title: '`cairn skill`'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T02:44:00Z'
labels:
  - planned
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
- [x] #1 Output has valid frontmatter with version and hash
- [x] #2 Every `cairn ...` line in a prose code fence parses with the clap tree (test)
- [x] #3 Every non-hidden command and argument has help text (test)
- [x] #4 Full output is an insta snapshot; changing a flag fails the test until reviewed
- [x] #5 `--check` exits 0 on a fresh copy and 1 on a stale one
- [x] #6 Exit codes in the output match the shared table
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by e29d310. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added src/skill.rs (generate, run), skill/*.md prose and template, Command::Skill, insta snapshot. Stale rule: installed text != generate(); missing file exits 3. Reference is a custom walk of the clap tree (not render_long_help, which repeats global flags per command). Added help text to positional args that lacked it. JSON output contract is prose (no shared Rust table); only exit codes come from EXIT_CODES. flake.nix needed no change: craneLib.path keeps skill/ and .snap files. Verified: cargo test, clippy, fmt, nix build checks.nextest (242 passed), skill --check 0 fresh / 1 edited.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented cairn skill and skill --check with embedded prose, runtime clap-tree reference, shared exit-code table, and tests including an insta snapshot.
<!-- SECTION:FINAL_SUMMARY:END -->
