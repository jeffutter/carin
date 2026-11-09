---
title: Web UI build and embedding
type: grilling
status: open
assignee:
blocked_by: [research-svelte-nix-embed, json-api-shape]
---

## Question

Which framework builds the web UI, and how is it built in nix and embedded in the single binary? Decide the framework (htmx with server-rendered HTML, Svelte, SvelteKit static, or Preact), the nix build approach (npm dependency hashing only if there is an npm build), embedding versus serving from disk, and the dev loop inside the nix shell. Also decide the flake outputs (package, devShell, checks) as the one open nix question.

Standing lean from the user: htmx, since the review UI is not expected to be very interactive. If htmx wins, the UI is server-rendered HTML routes in axum alongside the JSON API, so also decide how those two surfaces share the core operations and whether an HTML templating crate is needed. Artifact content (including any single-page app an Agent submits) is separate and shown in a sandboxed frame; it does not constrain this choice.
