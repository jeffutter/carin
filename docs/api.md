# JSON API

A REST API for tools other than the CLI and the web UI. It mirrors the CLI and calls the same core. Served by `cairn serve` on the UI port (default 7878), `127.0.0.1` only, no auth, no CORS headers in v1.

## Schema-driven

The API is defined by Rust types from which an OpenAPI 3.1 document is generated (`utoipa` or `aide`; pick one at build time after checking the dependency tree). The document is served at `/api/v1/openapi.json`, and a snapshot is committed with a test that fails on drift. Clients for outside tools are generated from that schema. The CLI's JSON output is the same serialized core types, so its shapes are covered too. The web UI does not use a generated client; it calls core directly ([ADR 0006](adr/0006-server-rendered-web-ui-with-htmx-and-maud.md)).

## Routes

All under `/api/v1`, separate from the web routes (`/a/<id>`). Refs are `abc123` or `abc123@3`.

| Route | Purpose |
| --- | --- |
| `GET /artifacts`, `POST /artifacts` | List; create |
| `GET /artifacts/{id}`, `PUT /artifacts/{id}`, `DELETE /artifacts/{id}` | Read; edit (new Revision); hard delete |
| `GET /artifacts/{id}/revisions`, `GET /artifacts/{id}/revisions/{n}` | Revision list; one Revision as a JSON file manifest (path, media type, size, sha256) |
| `GET /artifacts/{id}/revisions/{n}/files/{path}` | Raw bytes with the stored media type; `?section=` for a markdown Section |
| `GET /artifacts/{id}/outline` | Headings, paths, sizes |
| `GET /artifacts/{id}/comments`, `POST /artifacts/{id}/comments` | List (with `since` and `wait`); add or reply |
| `POST /threads/{id}/resolve`, `POST /threads/{id}/reopen` | Thread state |

Both roles may call everything.

## Attribution

`role`, `author` and `session` are body fields on writes, defaulted as in the CLI. No headers and no env vars.

## Writes

`multipart/form-data`: a JSON `meta` part (`kind`, `format`, `name`, `role`, `author`, `session`, `addresses`, `base_revision`, and on create `project_name`, `project_root`, `worktree`) plus one part per file. A Section edit is a `PUT` with `?section=` and one markdown part. The limits and embed validation of [data-model.md](data-model.md) apply.

## Reads and lists

- Lists use the same filters as the CLI list (`kind`, `session`, `open`, `limit`) plus `project`, `project_root` and `worktree`. Unscoped by default, since there is no cwd to detect from. There is no `mine`; use `session`.
- Response shape `{ "items": [...] }`. Default limit 50, maximum 200, no cursor in v1.
- Each Artifact in a list carries stored bytes and last-updated time.
- Content bytes are served with `Content-Disposition: attachment` and `X-Content-Type-Options: nosniff`. Rendering untrusted html and svg inline is the content origin's job ([web-ui.md](web-ui.md#untrusted-content)), not this route's.

## Long poll

`GET /artifacts/{id}/comments?since=<id>&wait=<secs>` returns when there are Comments past the cursor or the wait ends (default 60, capped). A timeout is a plain 200 with an empty list and the same `highest_comment_id`. The server polls the database every 500 ms. No SSE or websocket in v1.

## Errors

RFC 9457 `application/problem+json` with a stable `code` shared with the CLI's JSON stderr (one error enum in core).

| CLI exit | HTTP |
| --- | --- |
| 2 usage | 400 |
| 3 not found | 404 |
| 4 rejected input | 422 |
| 6 conflict | 409 |
| 1 unexpected | 500 |

Exit 5 has no error: a wait timeout is a 200.
