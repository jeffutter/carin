---
label: wayfinder:map
---

# Cairn map

## Destination

A spec, handed off ready for implementation: design docs in this repo covering architecture, data model, CLI surface, JSON API, web UI approach, and how `cairn skill` is generated, plus ADRs for hard-to-reverse choices. The map is done when nothing is left to decide before someone starts building. Nothing in `src/` is built as part of this effort.

## Notes

- Cairn: a Rust CLI for agents to submit artifacts that a human reviews and discusses. See `GLOSSARY.md` for terms (Artifact, Kind, Revision, Comment, Agent, Reviewer); sharpen the loose ones as their tickets resolve.
- Planning only. Prototypes are allowed but throwaway: they live on `prototype/<name>` branches or under `.wayfinder/assets/`, never in `src/`.
- Tracker: local markdown. Tickets are `.wayfinder/tickets/<slug>.md`; frontmatter holds `type`, `status`, `assignee` (the claim) and `blocked_by`. Frontier = open, unclaimed, every `blocked_by` ticket closed. Committed to the current branch.
- Stack: Rust, clap, sqlite, axum; web UI probably Svelte (lightweight but full-featured); a JSON API others can build on.
- Nix flake for the project; every dependency comes from nix. Copy the layout of `github.com/jeffutter/notectl` and `github.com/jeffutter/graphql-document-utils` (nixpkgs-unstable, flake-utils, rust-overlay, crane, clang+mold on Linux, nixpkgs-fmt). The only open nix question is the Svelte build.
- Topology: one binary. A subcommand starts the axum server; the other subcommands manipulate the sqlite database directly through a shared core library. Agents and server are on the same machine; nothing on other hosts.
- History matters: edits keep history. Comments attach to a version of an artifact; for markdown, possibly to a section or line.
- Markdown artifacts should support retrieving and editing by heading section, to save agent tokens.
- `cairn skill` prints an agent skill: prose plus details extracted from the codebase (clap help), so it cannot drift from the functionality.
- Web UI should treat agent-supplied HTML as untrusted (iframes, sandboxing).
- Never use the em dash; write a plain dash.

## Decisions so far

<!-- one line per closed ticket -->

- [Research diagram formats and safe browser rendering](tickets/research-diagram-formats.md): six formats surveyed (Mermaid, SVG, Excalidraw, Graphviz, D2, PlantUML) with rendering and security notes; LLM authorability and a few renderer details unconfirmed.
- [Research iframe sandboxing and CSP for untrusted agent HTML](tickets/research-iframe-sandbox.md): sandbox tokens, srcdoc vs URL, CSP, separate origin/port and axum surveyed; several points marked not verified (srcdoc CSP inheritance, cross-port cookies, images in opaque origin, dual axum listeners).
- [Research building Svelte in nix and embedding it in an axum binary](tickets/research-svelte-nix-embed.md): npm hashing options, crane source filtering, rust-embed/axum-embed, SPA output and dev proxy surveyed; nothing built in nix, a few gaps listed.

## Not yet specified

- Concurrency and conflicts when an agent edits while a reviewer comments.
- Storage limits and cleanup of old artifacts and revisions.
- Config and data directory locations; how the CLI finds the database (and how the server shares it).

## Out of scope

- The `herdr desktop` extension and any other tool built on the API (the API only has to be sufficient for them).
- Multi-user accounts, hosted deployment, auth beyond what a single-user local server needs.
- Agents on other hosts or in containers without access to the local database.
