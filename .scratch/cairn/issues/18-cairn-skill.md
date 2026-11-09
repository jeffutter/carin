# 18: `cairn skill`

**What to build:** `cairn skill` prints one complete SKILL.md to stdout: frontmatter (name, description, cairn version, hash of the generated text) plus Markdown built from hand-written prose files embedded in the binary and a runtime walk of the clap command tree for usage, long help, flags, env vars and defaults (`serve` and `skill` omitted). The exit-code table and JSON output contract come from the single Rust table shared with CLI error handling. The prose covers purpose, the submit / wait / edit-with-base-and-addresses workflow, `cairn:` embeds, Section path syntax, choosing a Kind, and asking the human to run `cairn serve`; it holds no flag details. `cairn skill --check <path>` compares an installed copy and exits 0 if current and 1 if stale. Nothing is written to disk.

**Blocked by:** 07: Agent feedback loop: `--wait` and `--addresses`, 08: Project and worktree scoping, 11: Markdown embeds (`cairn:` references)

**Status:** ready-for-agent

- [ ] Output has valid frontmatter with version and hash
- [ ] Every `cairn ...` line in a prose code fence parses with the clap tree (test)
- [ ] Every non-hidden command and argument has help text (test)
- [ ] Full output is an insta snapshot; changing a flag fails the test until reviewed
- [ ] `--check` exits 0 on a fresh copy and 1 on a stale one
- [ ] Exit codes in the output match the shared table
