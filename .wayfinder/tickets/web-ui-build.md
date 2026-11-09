---
title: Web UI build and embedding
type: grilling
status: open
assignee:
blocked_by: [research-svelte-nix-embed, json-api-shape]
---

## Question

Is the web UI Svelte, and how is it built in nix and embedded in the single binary? Decide the framework (Svelte, SvelteKit static, or lighter), the nix build approach and npm dependency hashing, embedding versus serving from disk, and the dev loop inside the nix shell. Also decide the flake outputs (package, devShell, checks) as the one open nix question.
