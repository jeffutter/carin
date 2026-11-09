---
title: JSON API shape
type: grilling
status: closed
assignee: jeffutter
blocked_by: [cli-surface]
---

## Question

How does the axum JSON API map onto the same core operations as the CLI? Decide resource layout, the error format, pagination and filtering for lists, how binary artifact content and HTML bundles are served, and how the API is documented (for example generated from the same types). The aim is that other tools can build on it without Cairn building them.

## Resolution

Decided with the user (grilling), 2026-11-09.

- **Schema-driven**: the API is defined by Rust types from which an OpenAPI 3.1 document is generated (`utoipa` or `aide`; pick one when writing the spec, after checking the dependency tree). Served at `/api/v1/openapi.json`; a snapshot is committed with a test that fails on drift. The aim is that clients (the web UI included, and any future tool) are generated from the schema and share its types. The CLI's JSON output is the same serialized core types, so its shapes are covered too.
- **Layout**: `/api/v1`, separate from web routes (`/a/<id>`). `GET/POST /artifacts`; `GET/PUT/DELETE /artifacts/{id}`; `GET /artifacts/{id}/revisions[/{n}]`; `GET /artifacts/{id}/outline`; `GET/POST /artifacts/{id}/comments`; `POST /threads/{id}/resolve|reopen`. Refs are `abc123` or `abc123@3`. Both roles may call everything.
- **Attribution**: `role`, `author`, `session` are body fields on writes, defaulted as in the CLI; no auth, no env vars, no headers.
- **Errors**: RFC 9457 `application/problem+json` plus a stable `code` shared with the CLI's JSON stderr (one error enum in core). 2 -> 400, 3 -> 404, 4 -> 422, 1 -> 500.
- **Lists**: same filters as the CLI (`kind`, `session`, `open`, `limit`); `{ "items": [...] }`; default limit 50, max 200; no cursor in v1; no `mine`.
- **Waiting**: `GET /artifacts/{id}/comments?since=<id>&wait=<secs>` long poll (default 60, capped); timeout is a plain 200 with an empty list and the same `highest_comment_id`. No SSE or websocket in v1.
- **Content**: reads via `GET /artifacts/{id}/revisions/{n}/files/{path}` returning raw bytes with the stored media type, `?section=` for a markdown Section; the revision resource returns a JSON file manifest (path, media type, size, sha256). Writes are `multipart/form-data`: a JSON `meta` part (kind, format, name, role, author, session, `addresses`) plus one part per file; Section edit is a PUT with `?section=` and one markdown part. ADR 0003 limits and embed validation apply. The API serves bytes with `Content-Disposition: attachment` and `X-Content-Type-Options: nosniff`; rendering untrusted html and svg inline is the Web safety model ticket's decision.
- **Binding**: `serve --listen` defaults to `127.0.0.1:7878`; no CORS headers in v1.
- **Handed on**: Web safety model (how and from which origin untrusted files are rendered, given the attachment-only API route); Web UI build (generated client types from the OpenAPI schema).
