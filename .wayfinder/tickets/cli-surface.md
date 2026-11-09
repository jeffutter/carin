---
title: CLI surface and output contract
type: grilling
status: closed
assignee: jeffutter
blocked_by: [revisions-and-comment-anchoring, kinds-and-storage, markdown-section-addressing, agent-reviewer-loop]
---

## Question

What are the commands and arguments for the agent and reviewer functions (submit, get, edit, comments, delete, list, view, comment, serve, skill)? Decide naming, how artifact content is passed (path, stdin), the output contract (JSON versus human text, flags), exit codes, and how the single binary separates the server subcommand from the database-manipulating ones. Reviewer functions available through the CLI too, as the brief requires.

## Carried in from Artifact kinds and storage

Submit takes a file or a directory (a directory is an html Bundle needing `index.html`); `--kind` is explicit or inferred from extension, `.svg` needs `--kind diagram --format svg`. Markdown embeds images as `cairn:<id>[@<rev>]`; decide whether the CLI offers to upload local relative images and rewrite them. Size limits: 10 MiB per file, 25 MiB per Revision, 200 files per bundle.

## Resolution

Decided with the user (grilling), 2026-11-09.

- **Shape**: flat verbs for Artifacts: `submit`, `get`, `edit`, `outline`, `list`, `delete`. A `comment` group: `add`, `list`, `resolve`, `reopen`. Plus `serve`, `skill`, `view`. One binary; `serve` runs axum, every other command opens sqlite directly through the shared core.
- **Global flags**: `--role agent|reviewer` (default `agent`, env `CAIRN_ROLE`), `--author` (env `CAIRN_AGENT`, then `agent`/`reviewer`), `--session` (then `CAIRN_SESSION`, `CLAUDE_CODE_SESSION_ID`), `--db <path>` (env `CAIRN_DB`). Role is attribution only: both roles may run every command, including `delete`. The default database location stays in the map's fog.
- **Ids**: `abc123` (latest) or `abc123@3` wherever an Artifact is named.
- **Content in**: `submit <path|dir|->` and `edit <id> <path|->`. A directory is an html Bundle needing `index.html`. Stdin (`-`) needs `--kind` and `--name`. `--kind` is otherwise inferred per ADR 0003. No inline-string flag. No local-image upload or rewrite in v1; images and diagrams are submitted as their own Artifacts and embedded as `cairn:<id>`.
- **Edit**: `edit <id> <path|->` replaces the whole document; `edit <id> --section "A > B" <path|->` replaces a Section. Both make a new Revision of the latest. `--addresses 12,14` on either posts the auto-reply. No base-Revision check. Prints the new `abc123@4` as JSON.
- **Read**: `get <id>[@rev]` prints markdown or diagram source to stdout. `image` and `html` need `--out <dir>` and print a file list as JSON. `--section` is markdown only. `outline <id>[@rev]` prints headings, paths and sizes as JSON.
- **List**: newest activity first, with id, Kind, title, latest Revision, open Thread count, author. Filters `--kind`, `--session`, `--mine`, `--open`, `--limit` (default 50). No cursor in v1.
- **Comments**: `comment add <id>[@rev] <body|->` with `--lines a-b` and (markdown) `--section`; `--reply <comment-id>` takes no locator. `comment list [<id>]` defaults to open Threads on the caller's session Artifacts; `--all`, `--since <id>`, `--wait`, `--timeout <secs>` (default 60). Output includes `highest_comment_id`. `comment resolve|reopen <thread-id>`.
- **Output**: JSON on stdout for every command, errors as JSON on stderr. Content-returning `get` prints raw content. `--text` gives a human table for `list` and `comment list`.
- **Exit codes**: 0 success; 1 unexpected error; 2 usage error (including `delete` without `--yes`); 3 not found; 4 rejected input (limits, bad Kind, missing embed target, invalid Mermaid, ambiguous or missing section); 5 `--wait` timeout with nothing new.
- **Delete**: `delete <id> --yes`, no prompt, no TTY detection.
- **View**: `view <id>[@rev]` prints `<base>/a/<id>[@rev]`; base from `--url`, `CAIRN_URL`, then `http://127.0.0.1:7878`, the same default as `serve --listen`. It does not open a browser.
- **Handed on**: JSON API shape (operations to mirror, the `/a/<id>` route and default port); Skill generation (the clap tree it walks).

- **Amended** by [Concurrency and conflicts](concurrency-and-conflicts.md): `edit` gains optional `--base <rev>`, and exit code 6 means conflict (stale base). The "No base-Revision check" line above no longer holds.
