---
title: Web UI build and embedding
type: grilling
status: closed
assignee: jeffutter
blocked_by: [research-svelte-nix-embed, json-api-shape]
---

## Question

Which framework builds the web UI, and how is it built in nix and embedded in the single binary? Decide the framework (htmx with server-rendered HTML, Svelte, SvelteKit static, or Preact), the nix build approach (npm dependency hashing only if there is an npm build), embedding versus serving from disk, and the dev loop inside the nix shell. Also decide the flake outputs (package, devShell, checks) as the one open nix question.

Standing lean from the user: htmx, since the review UI is not expected to be very interactive. If htmx wins, the UI is server-rendered HTML routes in axum alongside the JSON API, so also decide how those two surfaces share the core operations and whether an HTML templating crate is needed. Artifact content (including any single-page app an Agent submits) is separate and shown in a sandboxed frame; it does not constrain this choice.

## Resolution

Decided with the user (grilling), 2026-11-09. Recorded in ADR 0006 (`docs/adr/0006-server-rendered-web-ui-with-htmx-and-maud.md`).

- **Framework**: server-rendered HTML with htmx; no npm, no JS build, no `npmDepsHash`. Svelte, SvelteKit and Preact dropped.
- **Shared operations**: HTML routes call the core library directly in-process, like the CLI and JSON API; no generated client for the UI. The OpenAPI schema serves outside tools only.
- **Templating**: `maud` (escaped by default, no template files that crane would filter out). Markdown stays pulldown-cmark with raw HTML dropped.
- **Static assets**: htmx pinned via `fetchurl` in the flake; hand-written CSS in the repo; both embedded with `include_str!`/`include_bytes!`, served from the UI port with a versioned URL; no `rust-embed`.
- **Flake**: copy the notectl layout: `packages.default` (crane), `devShells.default` (rust-overlay, clang+mold on Linux, cargo-watch, sqlite), `formatter` nixpkgs-fmt, `checks` for build, clippy, fmt, nextest (including the OpenAPI and skill drift tests). Dev loop: `cargo watch -x 'run -- serve'`.
