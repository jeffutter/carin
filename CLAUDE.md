# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

## What this is

`cairn` is a single Rust binary: a CLI for agents to submit artifacts (markdown, html bundles, images, mermaid/svg diagrams) that a human reviews and comments on, plus `cairn serve` (review web UI and `/api/v1` JSON API). Terms (Artifact, Revision, Kind, Section, Thread, ...) are defined in `GLOSSARY.md`; use them exactly. `docs/` is the spec (start at `docs/README.md`); ADRs in `docs/adr/` hold the reasoning. When a doc and a `.wayfinder/tickets/` file disagree, the doc wins. `.wayfinder/assets/` is throwaway reference material, not code to promote into `src/`.

## Commands

Build and test inside `nix develop` (or via direnv). It sets `PELE_JS` (Mermaid renderer) and `HTMX_JS`, which `build.rs` embeds with `include_str!`. Plain `cargo build` outside nix still compiles but embeds stubs: Mermaid fails at runtime and `cfg(cairn_pele_stub)` is set, so Mermaid tests are skipped.

- Build / run: `cargo build`, `cargo run -- <subcommand>`; dev server loop: `cargo watch -x 'run -- serve'`
- Tests: `cargo nextest run` (or `cargo test`); single test: `cargo nextest run <name>` / `cargo test <name>`. Integration tests are `tests/cli.rs` and `tests/embeds.rs`; scoping tests shell out to `git`.
- Lint: `cargo clippy --all-targets -- -D warnings`, `cargo fmt`
- Full CI equivalent: `nix flake check` (build, clippy, fmt, nextest), then `nix build`
- Release: `cargo release patch|minor|major --execute` (dry run without `--execute`); tags trigger `cd.yml`

### Generated artifacts that tests guard

- OpenAPI: `docs/openapi.json` is a snapshot of the utoipa-generated doc (`src/api.rs`). Regenerate with `UPDATE_OPENAPI=1 cargo test openapi_snapshot`.
- Skill: `cairn skill` output is an `insta` snapshot (`src/snapshots/`); review with `cargo insta review`. Prose in `skill/*.md` is embedded via `include_str!`, holds no flag details, and every `cairn ...` line in a code fence must parse against the clap tree. Reference sections are generated from the clap command tree, so every non-hidden command/arg needs help text.

## Architecture

- **Core library, two frontends.** All sqlite access goes through the core (`db.rs`, `artifact.rs`, `comments.rs`, ...), so CLI and server share identical connection setup (WAL, immediate write transactions, 5 s busy timeout, optional `--base` revision check; ADR 0007). The CLI opens the database directly; there is no daemon. `serve` (`serve.rs`, `api.rs`, `ui.rs`) calls the same core functions in-process, and the web UI does not go through `/api/v1`.
- **Revisions are immutable file sets stored in sqlite** (ADR 0003). Kind is fixed at creation. Markdown sections are addressed by heading path (`sections.rs`); markdown embeds other Artifacts with `cairn:<id>` (`embeds.rs`), resolved at submit.
- **Two origins.** `serve` runs two axum listeners: UI + API on `--port` (7878) and untrusted agent content on `--content-port` (default port+1), so agent-supplied html/svg can never act as the UI (ADR 0005). A non-loopback `--bind` drops the DNS-rebinding guard and has no auth.
- **Mermaid** is rendered server-side by Pele in embedded QuickJS (`rquickjs`, `diagram.rs`); `serve` keeps one long-lived engine.
- **Errors** are one enum with stable codes (`error.rs`) mapped to both CLI exit codes (`error::EXIT_CODES`, also used by the generated skill) and HTTP statuses.
- **Output** is JSON inside agent sessions, formatted by context (`output.rs`).
- **Scoping**: Project/Worktree come from git detection (`scope.rs`); default database is `$XDG_DATA_HOME/cairn/cairn.db` (override `--db` / `CAIRN_DB`). No config file; every option is a flag with an env var.
- UI is server-rendered with maud + htmx; CSS and JS live in `src/ui/` and are embedded, with no npm or JS build.
