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
- Stack: Rust, clap, sqlite, axum; a JSON API others can build on. Web UI framework is open: Svelte was the first idea, but the user now leans toward htmx (server-rendered HTML fragments, no npm build) because browsing and reviewing artifacts is not expected to be very interactive. A lean, not a decision; the Web UI build ticket settles it.
- The web UI's own framework is separate from artifact content: an Agent may submit a single-page app as an Artifact, but that is untrusted content shown in a sandboxed frame, unrelated to how the review UI is built.
- Nix flake for the project; every dependency comes from nix. Copy the layout of `github.com/jeffutter/notectl` and `github.com/jeffutter/graphql-document-utils` (nixpkgs-unstable, flake-utils, rust-overlay, crane, clang+mold on Linux, nixpkgs-fmt). The only open nix question is the web UI build (npm hashing if Svelte, none if htmx).
- Topology: one binary. A subcommand starts the axum server; the other subcommands manipulate the sqlite database directly through a shared core library. Agents and server are on the same machine; nothing on other hosts.
- History matters: edits keep history. Comments attach to a version of an artifact; for markdown, possibly to a section or line.
- Markdown artifacts should support retrieving and editing by heading section, to save agent tokens.
- `cairn skill` prints an agent skill: prose plus details extracted from the codebase (clap help), so it cannot drift from the functionality.
- Web UI should treat agent-supplied HTML as untrusted (iframes, sandboxing).
- Never use the em dash; write a plain dash.

## Decisions so far

<!-- one line per closed ticket -->

- [Research diagram formats and safe browser rendering](tickets/research-diagram-formats.md): six formats surveyed (Mermaid, SVG, Excalidraw, Graphviz, D2, PlantUML) with rendering and security notes; LLM authorability and a few renderer details unconfirmed.
- [Revisions, comments and attribution](tickets/revisions-and-comment-anchoring.md): immutable full-snapshot Revisions (`abc123@3`); Comments anchor to one Revision with optional line/section locator, never re-anchored; one-level resolvable Threads; author plus nullable session labels; whole-Artifact hard delete.
- [Research iframe sandboxing and CSP for untrusted agent HTML](tickets/research-iframe-sandbox.md): sandbox tokens, srcdoc vs URL, CSP, separate origin/port and axum surveyed; several points marked not verified (srcdoc CSP inheritance, cross-port cookies, images in opaque origin, dual axum listeners).
- [Research building Svelte in nix and embedding it in an axum binary](tickets/research-svelte-nix-embed.md): npm hashing options, crane source filtering, rust-embed/axum-embed, SPA output and dev proxy surveyed; nothing built in nix, a few gaps listed.
- [Research Rust crates for heading-based Markdown sectioning](tickets/research-markdown-sectioning.md): four parser crates probed on one document (three give byte offsets, comrak only line:column); none dedupes duplicate headings; notectl-outline lacks setext, front matter and replace.
- [Agent and reviewer feedback loop](tickets/agent-reviewer-loop.md): polling plus blocking `--wait`; `--since <comment-id>` instead of stored cursors; default view is open Threads on the Agent's Artifacts; `--addresses` posts an auto-reply but only the Reviewer resolves.
- [Diagram format](tickets/diagram-format.md): Kind `diagram` with format `mermaid` or `svg` in v1 (D2 and Excalidraw later); Mermaid rendered server-side to SVG by Pele in an embedded JS engine, invalid source rejected at submit; both shown via `<img>` from the untrusted origin. See ADR 0002.
- [Artifact kinds and storage](tickets/kinds-and-storage.md): Kinds markdown, html (a Bundle of files), image, diagram; every Revision a file set stored as content-addressed sqlite BLOBs; markdown embeds image and diagram Artifacts via `cairn:id[@rev]`; fixed size limits. See ADR 0003 and 0004.

## Not yet specified

- Concurrency and conflicts when an agent edits while a reviewer comments.
- Storage limits and cleanup of old artifacts and revisions.
- Embedding a JS engine (for example rquickjs) in the nix build, render cost and caching of derived SVG, and Pele's CSS-variable theming inside `<img>`; may need a small spike.
- Config and data directory locations; how the CLI finds the database (and how the server shares it).

## Out of scope

- The `herdr desktop` extension and any other tool built on the API (the API only has to be sufficient for them).
- Multi-user accounts, hosted deployment, auth beyond what a single-user local server needs.
- Agents on other hosts or in containers without access to the local database.
