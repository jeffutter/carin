# Generate the OpenAPI document from Rust types with utoipa

The JSON API's OpenAPI 3.1 document is generated from the Rust types with `utoipa` (derive only: `ToSchema`, `IntoParams`, `IntoResponses`, `OpenApi`). The routes are plain axum handlers, annotated with `#[utoipa::path]`. The document is served at `/api/v1/openapi.json` and a snapshot is committed at `docs/openapi.json`; a test fails when the two differ and says to regenerate with `UPDATE_OPENAPI=1 cargo test openapi_snapshot`.

We chose `utoipa` over `aide` because it adds almost nothing to the dependency tree and does not touch routing. Measured with `cargo tree -e normal --prefix none | sort -u`, against the tree before this change (108 lines):

| | New crates |
| --- | --- |
| `utoipa` 6 | `utoipa`, `utoipa-gen` (proc macro), `indexmap` (4 lines in all, 112 total) |
| `aide` 0.15 with `axum` | `aide`, `schemars` and `schemars_derive`, `serde_derive_internals`, `ref-cast` and `ref-cast-impl`, `dyn-clone`, `once_cell`, `log`, `tracing`, `tracing-core`, `tracing-attributes`, `indexmap` (about 13 crates) |

`aide` wraps the axum `Router` and `MethodRouter` so it can see routes, which ties it to axum's version and replaces the router type used by the Host check and hardening layers that the API shares with the web UI. `utoipa` leaves routing alone; the cost is that each handler carries a `#[utoipa::path]` attribute that repeats its path and parameters, and the snapshot test is what catches a path that drifts from its route (the served-document test lists every path).

The CLI prints the same named serde types (`ArtifactList`, `Outline`, `CommentList`), so one derive covers both outputs. Writes (TASK-17) add their routes to the same `ApiDoc`.
