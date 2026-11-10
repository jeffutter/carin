# `cairn skill`

`cairn skill` prints an agent skill that cannot drift from the functionality, because the reference part is read from the code.

## Output

One complete `SKILL.md` on stdout: YAML frontmatter (`name: cairn`, `description`, the cairn version, a hash of the generated text) followed by Markdown. Nothing is written to disk and there is no install subcommand; the user redirects to `~/.claude/skills/cairn/SKILL.md`. Other agent harnesses can use it as plain Markdown.

## Sources

- **Reference**, generated at runtime: `CommandFactory::command()` is walked recursively (`get_subcommands`, `get_arguments`, `render_long_help`) for usage, long help, flags, env vars and defaults. `serve` and `skill` are omitted.
- **Prose**, hand-written: a few Markdown files under `skill/`, embedded with `include_str!`. They cover purpose, the workflow (submit, `comment list --wait`, `edit --base ... --addresses ...`), `cairn:<id>` embeds, the Section path syntax, choosing a Kind, and asking the human to run `cairn serve`. One template has a `{{name}}` slot per prose file plus `{{reference}}`; slots are filled in one pass, so inserted text containing `{{...}}` is kept literally. A slot name with no value is left verbatim, and a test checks the embedded template has none. Prose holds no flag details.
- **Exit codes** come from `error::EXIT_CODES`, the one Rust table shared with CLI error handling ([cli.md](cli.md#exit-codes)). The output contract (JSON inside agent sessions, how to force it) is prose in `skill/purpose.md`; there is no shared Rust table for it.

## Drift protection

All of this runs in `cargo test`, so nix CI runs it:

- every `cairn ...` line in a prose code fence must parse with `try_get_matches_from`;
- every non-hidden command and argument must have help text;
- the full generated skill is an `insta` snapshot.

## Staleness

`cairn skill --check <path>` compares an installed file against the current output (version and hash in the frontmatter). Exit 0 if it matches, 1 if stale (any difference, including a hand edit). A missing file counts as stale and exits 1. There is no auto-update.
