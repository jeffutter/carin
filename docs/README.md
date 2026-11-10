# Cairn design

Cairn is a Rust CLI for agents to submit artifacts that a human reviews and discusses. These docs are the spec: nothing is left to decide before building starts, apart from the small build-time points listed at the end of this page.

Terms (Artifact, Kind, Revision, Comment, Agent, Reviewer and the rest) are defined in [`../GLOSSARY.md`](../GLOSSARY.md). Hard-to-reverse choices have their own records in [`adr/`](adr/); these docs state the whole design and link to the ADRs for the reasoning.

| Doc | Covers |
| --- | --- |
| [architecture.md](architecture.md) | One binary, shared core, process topology, nix build, build order |
| [data-model.md](data-model.md) | Artifacts, Revisions, files, Comments, Threads, scoping, limits, transactions |
| [cli.md](cli.md) | Every command, global flags, output, exit codes |
| [api.md](api.md) | The `/api/v1` JSON API and its OpenAPI schema |
| [web-ui.md](web-ui.md) | The review UI, the untrusted origin and rendering safety |
| [skill.md](skill.md) | How `cairn skill` is generated and kept from drifting |

## ADRs

1. [One binary; the CLI manipulates the database directly](adr/0001-single-binary-direct-database-access.md)
2. [Render Mermaid on the server with Pele](adr/0002-render-mermaid-server-side-with-pele.md)
3. [Store every Revision as a set of files in sqlite](adr/0003-store-revisions-as-file-sets-in-sqlite.md)
4. [Markdown embeds images by reference to other Artifacts](adr/0004-markdown-embeds-images-by-artifact-reference.md)
5. [Serve untrusted content from a separate origin](adr/0005-serve-untrusted-content-from-a-separate-origin.md)
6. [Server-rendered web UI with htmx and maud](adr/0006-server-rendered-web-ui-with-htmx-and-maud.md)
7. [Immediate write transactions and an optional base check](adr/0007-immediate-write-transactions-and-optional-base-check.md)
8. [Generate the OpenAPI document from Rust types with utoipa](adr/0008-generate-openapi-from-rust-types-with-utoipa.md)

## Where the decisions came from

The decision trail is in `.wayfinder/map.md` (the index) and `.wayfinder/tickets/` (one file per decision, with the reasoning and the alternatives rejected). When a doc here and a ticket disagree, the doc wins: where a later ticket amended an earlier one, the docs carry the amended result.

## Build-time points

Small choices deliberately left for the build, none of which change an interface:

- `utoipa` or `aide` for the OpenAPI schema; check the dependency tree first.
- CRLF handling in markdown sectioning (write the test, then pick).
- Reading the real `PeleError` code and message from the embedded engine with `ctx.catch()`, and running renders under a QuickJS memory limit and interrupt handler.
- Exact column types and indexes in the schema.
