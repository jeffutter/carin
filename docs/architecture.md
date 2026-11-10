# Architecture

## Shape

One binary, `cairn`. Every subcommand except `serve` and `skill` opens the sqlite database directly through a shared core library. `serve` starts the axum server, a second frontend over the same core. Agents and the server run on the same machine; nothing reaches the database from another host. See [ADR 0001](adr/0001-single-binary-direct-database-access.md).

```
 cairn submit/get/edit/...      cairn serve
          |                    /          \
          |           review UI (:7878)   untrusted content (:7879)
          |           + /api/v1            |
          v                  v             v
        +---------------- core library ----------------+
        |  artifacts, revisions, comments, sectioning, |
        |  mermaid rendering, embeds, scoping, limits  |
        +----------------------+-----------------------+
                               |
                       sqlite (WAL), one file
```

The core is the only code that opens the database, so CLI and server cannot configure connections differently ([ADR 0007](adr/0007-immediate-write-transactions-and-optional-base-check.md)). The CLI, the JSON API and the web UI routes all call core functions in-process; the web UI does not go through `/api/v1` ([ADR 0006](adr/0006-server-rendered-web-ui-with-htmx-and-maud.md)).

## Core responsibilities

- Artifact, Revision and Comment operations with the transaction rules in [data-model.md](data-model.md).
- Kind detection and validation: extension inference, limits, media type allowlist, html Bundle shape.
- Markdown work: heading parse with pulldown-cmark, Section resolve and splice, outline, `cairn:` embed resolution at submit.
- Mermaid validation and rendering with Pele in an embedded QuickJS engine (`rquickjs`) ([ADR 0002](adr/0002-render-mermaid-server-side-with-pele.md)). `serve` keeps one long-lived engine; a CLI submit pays the roughly 75 ms start-up once per invocation. The engine runs with a memory limit and an interrupt handler.
- Git project detection ([data-model.md](data-model.md#project-and-worktree-scope)).
- One error enum with stable codes, mapped to exit codes ([cli.md](cli.md#exit-codes)) and HTTP statuses ([api.md](api.md#errors)).
- The shared serializable types that the CLI prints, the API returns and the OpenAPI schema describes.

## Processes and ports

| Process | What it does |
| --- | --- |
| `cairn <command>` | Opens the database, does one thing, prints JSON, exits. `comment list --wait` polls the database every 500 ms (a constant in core). |
| `cairn serve` | Two axum listeners in one process (`tokio::try_join!`), both on `127.0.0.1`: the UI and `/api/v1` on `--port` (default 7878), untrusted content on `--content-port` (default port + 1). No `--bind`. A taken port is a clear non-zero exit, never a random port, because the CSP `frame-ancestors` needs a predictable origin. |

CLI and server write the same database concurrently. WAL mode makes that safe; writes wait up to a 5 s busy timeout ([ADR 0007](adr/0007-immediate-write-transactions-and-optional-base-check.md)).

## Configuration and state

- The database is the only state on disk. Path: `--db`, then `CAIRN_DB`, then `$XDG_DATA_HOME/cairn/cairn.db` (fallback `~/.local/share/cairn/cairn.db`, also on macOS). The directory and schema are created on first use by any subcommand; there is no `init`. One database per user.
- No config file in v1. Everything is a flag with an env var; the full list is in [cli.md](cli.md#global-flags).
- The database is created with `auto_vacuum=FULL` so deleting shrinks the file.

## Build and layout

Rust with clap, rusqlite, axum, maud, pulldown-cmark and rquickjs. The nix flake copies the layout of `github.com/jeffutter/notectl` and `github.com/jeffutter/graphql-document-utils`: nixpkgs-unstable, flake-utils, rust-overlay, crane, clang plus mold on Linux, nixpkgs-fmt.

- `packages.default`: the crane build. rquickjs needs only `clang` added to `nativeBuildInputs`. Pele's `pele.global.js` is pinned with `fetchzip` and embedded with `include_str!`. htmx is pinned with `fetchurl`. There is no npm and no JavaScript build. When `PELE_JS` is unset (plain `cargo build` outside nix), `build.rs` embeds a stub script instead and sets `cfg(cairn_pele_stub)`: everything builds, and Mermaid submit/render fails at runtime with "built without PELE_JS". Build with `nix develop` or `nix build` for Mermaid.
- `devShells.default`: rust-overlay toolchain, clang and mold on Linux, `cargo-watch`, sqlite. Dev loop: `cargo watch -x 'run -- serve'`.
- `formatter`: nixpkgs-fmt.
- `checks`: build, clippy, fmt, and nextest. The nextest run includes the OpenAPI snapshot test and the skill drift tests, so nix CI catches both kinds of drift.

Every dependency comes from nix. Hand-written CSS and the pinned htmx are embedded with `include_str!` / `include_bytes!` and served from the UI port at a versioned URL; there is no `rust-embed`.

## Suggested build order

1. Core: schema, connection setup, Artifact and Revision operations, limits, error enum.
2. CLI for markdown: `submit`, `get`, `edit`, `outline`, `list`, `delete`, then the `comment` group and `--wait`.
3. Other Kinds: html Bundles, images, svg and Mermaid diagrams (engine spike code is in `.wayfinder/assets/mermaid-engine-spike/`), markdown embeds.
4. Scoping: git detection and the `--project` family of flags.
5. `serve`: the content port first (`.wayfinder/assets/web-safety-spike/`), then the UI (`.wayfinder/assets/web-ui-prototype/index.html` is the look to match), then `/api/v1` and the OpenAPI snapshot.
6. `cairn skill` and its drift tests, last, because it walks the finished clap tree.

The spike and prototype assets are throwaway reference material, not code to promote into `src/`.

## Out of scope

The `herdr desktop` extension and other tools built on the API (the API only has to be sufficient for them); multi-user accounts, hosted deployment and auth beyond a single-user local server; agents on other hosts or in containers without access to the local database.
