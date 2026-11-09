# Server-rendered web UI with htmx and maud, calling core directly

The review UI is HTML rendered by axum routes with `maud`, enhanced by htmx fragments. There is no npm, no JavaScript build and no frontend dependency hash. The HTML routes call the shared core library in the same process, exactly as the CLI and the JSON API do; they do not go through `/api/v1`. The generated OpenAPI schema exists for outside tools, not for the UI.

We chose this over Svelte (or another SPA) because reviewing artifacts is mostly reading with a few forms, and an SPA would add an npm build to the nix flake, a second type boundary, and a client to keep in sync. We chose `maud` over `askama` or `minijinja` because it escapes by default (matching ADR 0005's "escape everything") and needs no template files, which crane's source filter would otherwise drop. htmx is pinned with `fetchurl` in the flake, and the small hand-written CSS and htmx are embedded with `include_*`.

The cost is that markup lives in Rust and rich client-side interactions (inline editors, drag and drop) are awkward; moving to an SPA later would mean rewriting the UI layer, though not the core or the API.
