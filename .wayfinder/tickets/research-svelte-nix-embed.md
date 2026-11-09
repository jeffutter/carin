---
title: Research building Svelte in nix and embedding it in an axum binary
type: research
status: closed
assignee: jeffutter
blocked_by: []
---

## Question

How do projects build a Svelte (or SvelteKit static / Vite) frontend inside a crane-based nix flake, and embed the result in a Rust axum binary (rust-embed, include_dir, or similar)? Cover npm dependency hashing in nix (buildNpmPackage, pnpm, bun), how dev-mode works with a nix shell, and lighter alternatives to Svelte worth noting. Surface facts only; the decision is the Web UI build ticket.

## Resolution

Findings are on branch `research/svelte-nix-embed` (commit cdd5b71, local only, never merged) in `docs/research/svelte-nix-embed.md`. It covers npm/pnpm/yarn/bun dependency hashing in nix, Svelte output shape, embedding (rust-embed, axum-embed, include_dir), crane, dev mode in a nix shell, and lighter alternatives, cited to docs.rs, crates.io, the nixpkgs manual, crane.dev, svelte.dev, vite.dev and the notectl flake.

Facts the Web UI build decision leans on, as the agent reported them (headings checked, text not re-read):
- `buildNpmPackage` uses `npmDepsHash` (computable with `prefetch-npm-deps`); `importNpmLock` needs no hash; pnpm has `fetchPnpmDeps`; the nixpkgs manual does not mention bun and `bun2nix` is third-party.
- crane's `cleanCargoSource` drops non-Rust files, so a frontend `dist/` is excluded unless added with a fileset or custom filter.
- rust-embed embeds in release builds and reads from disk in debug builds unless `debug-embed` is on; `axum-embed` adds serving.
- Svelte `adapter-static` has an SPA `fallback` option; the Vite dev server proxy can forward API calls to axum.
- Preact with HTM and htmx need no build step.

The file's "Not established" section lists what could not be verified: rust-embed's missing-folder behaviour, plain Svelte with vite-plugin-svelte, the bun2nix workflow, and size figures. Some claims are marked as inference. Nothing was built or run in nix, so a build proof belongs in the Web UI build ticket if the choice hinges on it.
