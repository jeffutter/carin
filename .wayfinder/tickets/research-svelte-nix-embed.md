---
title: Research building Svelte in nix and embedding it in an axum binary
type: research
status: open
assignee: jeffutter
blocked_by: []
---

## Question

How do projects build a Svelte (or SvelteKit static / Vite) frontend inside a crane-based nix flake, and embed the result in a Rust axum binary (rust-embed, include_dir, or similar)? Cover npm dependency hashing in nix (buildNpmPackage, pnpm, bun), how dev-mode works with a nix shell, and lighter alternatives to Svelte worth noting. Surface facts only; the decision is the Web UI build ticket.
