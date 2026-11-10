# JSON API

A REST API for tools other than the CLI and the web UI. It mirrors the CLI and calls the same core. Served by `cairn serve` on the UI port (default 7878), `127.0.0.1` only, no auth, no CORS headers in v1.

## Schema-driven

The API is defined by Rust types from which an OpenAPI 3.1 document is generated with `utoipa` ([ADR 0008](adr/0008-generate-openapi-from-rust-types-with-utoipa.md)). The document is served at `/api/v1/openapi.json`, and a snapshot is committed at [`openapi.json`](openapi.json) with a test that fails on drift (regenerate with `UPDATE_OPENAPI=1 cargo test openapi_snapshot`). Clients for outside tools are generated from that schema. The CLI's JSON output is the same serialized core types, so its shapes are covered too. The web UI does not use a generated client; it calls core directly ([ADR 0006](adr/0006-server-rendered-web-ui-with-htmx-and-maud.md)).

## Routes

All under `/api/v1`, separate from the web routes (`/a/<id>`). Ids in paths are bare (`abc123`); a Revision is its own path segment, and an id containing `@` is a 400.

| Route | Purpose |
| --- | --- |
| `GET /artifacts`, `POST /artifacts` | List; create |
| `GET /artifacts/{id}`, `PUT /artifacts/{id}`, `DELETE /artifacts/{id}` | Read; edit (new Revision); hard delete |
| `GET /artifacts/{id}/revisions`, `GET /artifacts/{id}/revisions/{n}` | Revision list; one Revision as a JSON file manifest (path, media type, size, sha256) |
| `GET /artifacts/{id}/revisions/{n}/files/{path}` | Raw bytes with the stored media type; `?section=` for a markdown Section |
| `GET /artifacts/{id}/outline` | Headings, paths, sizes; `?revision=` for a past Revision |
| `GET /artifacts/{id}/embeds` | A markdown Revision's `cairn:` embeds resolved now (as `cairn embeds`); `?revision=` for a past Revision |
| `GET /openapi.json` | The OpenAPI 3.1 document |
| `GET /artifacts/{id}/comments`, `POST /artifacts/{id}/comments` | List (with `since` and `wait`); add or reply |
| `POST /threads/{id}/resolve`, `POST /threads/{id}/reopen` | Thread state |

Both roles may call everything.

## Attribution

`role`, `author` and `session` are body fields on writes, defaulted as in the CLI. No headers and no env vars.

## Writes

Create (`POST /artifacts`, 201) and edit (`PUT /artifacts/{id}`, 200) take `multipart/form-data`: a JSON `meta` part plus one part per file. The part name is the file's stored path, so a file cannot be named `meta`. Both return `{ "id", "revision", "ref" }`.

| `meta` field | Applies to | Meaning |
| --- | --- | --- |
| `kind` | create, required | `markdown`, `html`, `image` or `diagram`; any other value is a 400 |
| `format` | create | `mermaid` or `svg`; required for a diagram and refused otherwise (422); any other value is a 400 |
| `name` | create | Display name and stored path of a single-file Artifact; defaults to the file part's name, required with several files |
| `project_root`, `project_name`, `worktree` | create | Scope as the CLI stores it: `project_root` and `worktree` are absolute paths; `project_name` alone is a 400 |
| `base_revision` | edit | Refuse with a 409 unless this is still the latest Revision; the detail names the latest Revision and its author |
| `addresses` | edit | Root Comment ids the new Revision answers |
| `role`, `author`, `session` | both | Attribution: `agent` (default) or `reviewer`; author defaults to the role name; no session unless given |

`meta` is optional on edit and refused fields are a 400, as are unknown fields. One file part replaces a single-file Artifact (its stored path is kept); several parts replace an html Bundle whole. A Section edit is a `PUT` with `?section=` and exactly one markdown file part. `DELETE /artifacts/{id}` answers `{ "deleted": [id] }`.

Comments are JSON: `POST /artifacts/{id}/comments` takes `{ body, lines?, section?, reply?, revision?, role?, author?, session? }` (`lines` is `a-b` or `a`, as the CLI's `--lines`) and returns the CLI's `comment add` JSON with a 201. `POST /threads/{id}/resolve` and `/reopen` take no body, are idempotent and return the CLI's Thread JSON. The limits and embed validation of [data-model.md](data-model.md) apply to every write, with the same stable error codes: an over-limit file, bad Kind or bad embed is a 422.

A write carrying an `Origin` header other than the request's own `Host` is a 403 with code `forbidden`: a web page on another site can send a simple cross-site POST to a loopback listener, and browsers always attach `Origin` to it. curl and agents send no `Origin` and are unaffected.

## Reads and lists

- Lists use the same filters as the CLI list (`kind`, `session`, `author`, `open`, `older_than`, `limit`) plus `project` (a name), `project_root` and `worktree` (absolute paths; `worktree` shows the root's Artifacts plus that Worktree's, as the CLI does from inside one; `worktree` without `project` or `project_root` is a 400, since a Worktree belongs to a Project). An unknown `kind` is a 400. Unscoped by default, since there is no cwd to detect from. There is no `mine`; use `session`.
- Response shape `{ "items": [...] }`. Default limit 50, maximum 200 (a limit outside 1 to 200 is a 422, not clamped), no cursor in v1. A single Artifact (`GET /artifacts/{id}`) is its list row plus `format` and `created_at`; the CLI `list` prints the same rows under `{ "artifacts": [...] }`.
- Each Artifact in a list carries stored bytes and last-updated time.
- File bytes are served from the UI port only as downloads, with `Content-Disposition: attachment` and `X-Content-Type-Options: nosniff`. Rendering untrusted html and svg inline is the content origin's job ([web-ui.md](web-ui.md#untrusted-content)), not this route's.

## Long poll

`GET /artifacts/{id}/comments?since=<id>&wait=<secs>` returns when there are Comments past the cursor or the wait ends. `wait` is seconds, clamped to 300; `wait=true` (or empty) waits 60 and absent, `false` or `0` answers at once. Without `since` the cursor is the highest Comment id when the request arrived. A timeout is a plain 200 with an empty list and the same `highest_comment_id`. The server polls the database every 500 ms without holding a thread or connection between polls, and abandons the wait when the client disconnects. The API has no caller identity, so a Comment by any author, including one named `api`, ends a wait (the CLI's `--wait` ignores the caller's own Comments). No SSE or websocket in v1.

## Errors

RFC 9457 `application/problem+json` with a stable `code` shared with the CLI's JSON stderr (one error enum in core).

| CLI exit | HTTP |
| --- | --- |
| 2 usage | 400 |
| 3 not found | 404 |
| 4 rejected input | 422 |
| 6 conflict | 409 |
| (none) cross-origin write | 403 `forbidden` |
| 1 unexpected | 500 |

A malformed query string or path segment is a 400, an unknown `/api/v1` path a 404 and a wrong method a 405 with an `Allow` header, all problem+json. `HEAD` works on every `GET` route. A well-formed value the server refuses (a limit out of range, a bad embed, a missing format) is a 422; an unknown enum value (`kind`, `format`) is malformed, so a 400. Exit 5 has no error: a wait timeout is a 200.
