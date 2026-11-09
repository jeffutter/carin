---
title: Skill generation and drift protection
type: grilling
status: closed
assignee: jeffutter
blocked_by: [cli-surface]
---

## Question

How does `cairn skill` produce the agent skill? Decide how clap command help is extracted (walking the clap `Command` tree at runtime), where the hand-written prose lives (embedded in the binary), the skill's output format, and how drift is caught (for example a test or CI check that the generated skill covers every command).

## Resolution

Decided with the user (grilling), 2026-11-09.

- **Output**: `cairn skill` prints one complete `SKILL.md` to stdout: YAML frontmatter (`name: cairn`, `description`, cairn version, hash of the generated text) then Markdown. Nothing is written to disk and there is no install subcommand; the user redirects to `~/.claude/skills/cairn/SKILL.md`. Usable by other agent harnesses as plain Markdown.
- **Extraction**: at runtime, `CommandFactory::command()` is walked recursively (`get_subcommands`, `get_arguments`, `render_long_help`) for usage, long help, flags, env vars and defaults. `serve` and `skill` are omitted from the reference.
- **Prose**: a few Markdown files under `skill/`, embedded with `include_str!`: purpose, workflow (submit, `comment list --wait`, `edit --addresses`), `cairn:<id>` embeds, Section path syntax, choosing a Kind. One template with a single `{{reference}}` slot for the generated reference. No flag details in prose. Exit codes (and the JSON output contract) come from one Rust table (an enum with descriptions) shared by the CLI error handling and the skill, so they cannot disagree.
- **Drift checks** (all in `cargo test`, so nix CI runs them): every `cairn ...` line in a prose code fence must parse with `try_get_matches_from`; every non-hidden command and argument must have help text; the full generated skill is an `insta` snapshot.
- **Staleness**: `cairn skill --check <path>` compares an installed file against the current output (version and hash in frontmatter); exit 0 if it matches, 1 if stale. No auto-update.
- **Note for the spec**: exit 1 is "unexpected error" in the CLI surface contract; a stale skill from `--check` also uses 1 as agreed. Reconcile when the CLI doc is written (a dedicated code is the alternative).
