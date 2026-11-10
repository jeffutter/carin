# CLI

One binary, flat verbs for Artifacts plus a `comment` group. Every command except `serve` and `skill` opens sqlite directly through the core ([architecture.md](architecture.md)). Data rules (ids, Sections, limits, scoping) are in [data-model.md](data-model.md).

```
cairn submit <path|dir|->      cairn comment add <id>[@rev] <body|->
cairn get <id>[@rev]           cairn comment list [<id>]
cairn edit <id> <path|->       cairn comment resolve <thread-id>
cairn outline <id>[@rev]       cairn comment reopen <thread-id>
cairn list                     cairn serve
cairn delete <id>... --yes     cairn skill
cairn view <id>[@rev]
```

## Global flags

| Flag | Env | Default / fallback |
| --- | --- | --- |
| `--role agent\|reviewer` | `CAIRN_ROLE` | `agent` |
| `--author <name>` | `CAIRN_AGENT` | `agent` (or `reviewer` for that role) |
| `--session <label>` | `CAIRN_SESSION`, then `CLAUDE_CODE_SESSION_ID` | none |
| `--db <path>` | `CAIRN_DB` | `$XDG_DATA_HOME/cairn/cairn.db` |
| `--project <path-or-name>` | `CAIRN_PROJECT` | detected from the cwd |

Role is attribution only: both roles may run every command, including `delete`.

### Scoping

`--project` takes a root path (value starts with `/`, replaces the detected root; the name comes from its last segment) or a name. A name is accepted on reads as a filter and rejected on writes. `none` or an empty value disables detection for the call. There is no worktree override; the worktree comes from the cwd. Detection and defaults are in [data-model.md](data-model.md#project-and-worktree-scope). `list` and `comment list` also take `--all-projects`, `--all-worktrees` and `--project-root <path>` (narrows when a name maps to several roots).

## Output contract

- JSON on stdout for every command; errors as JSON on stderr with a stable `code` shared with the API.
- Exception: `get` prints raw content (markdown or diagram source) to stdout.
- `--text` gives a human table for `list` and `comment list`.
- Command shapes are the same serialized core types the API returns.

## Exit codes

One Rust table (an enum with descriptions) drives the CLI error handling and the generated skill, so they cannot disagree.

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Unexpected error, including database busy past the 5 s timeout |
| 2 | Usage error, including `delete` without `--yes` |
| 3 | Not found, including a write to a deleted Artifact |
| 4 | Rejected input: limits, bad Kind, missing embed target, invalid Mermaid, ambiguous or missing Section |
| 5 | `comment list --wait` timed out with nothing new |
| 6 | Conflict: `edit --base` named a stale Revision |

`cairn skill --check` also exits 1 for a stale installed skill, a deliberate exception agreed when the skill was designed: its meaning is clear from the command, and a dedicated code would add a number every Agent must learn for one rarely used flag.

## Commands

### submit

`submit <path|dir|->` creates an Artifact and its Revision 1. A directory is an html Bundle and needs `index.html`. Stdin (`-`) needs `--kind` and `--name`. Otherwise `--kind` is inferred from the extension ([data-model.md](data-model.md#kinds-and-files)); `--format` for diagrams. Local images are not uploaded or rewritten: submit an image as its own Artifact and embed it as `cairn:<id>`. There is no inline-string flag. Prints the new `abc123@1`.

### edit

- `edit <id> <path|->` replaces the whole document.
- `edit <id> --section "A > B" <path|->` replaces a Section (markdown only).
- Both create a new Revision of the latest and print the new `abc123@4`.
- `--base <rev>` refuses a stale base with exit 6.
- `--addresses 12,14` posts the auto-reply in each named Thread.

### get and outline

`get <id>[@rev]` prints markdown or diagram source. `image` and `html` need `--out <dir>` and print a file list as JSON. `--section` is markdown only. `outline <id>[@rev]` prints `{"sections": [{"path": "A > B", "level": 2, "bytes": 120}]}`, one entry per heading in document order; `bytes` is the Section's size including children. A path that is not unique as a suffix carries `#n` (`Notes#2`) so it can be passed straight to `--section`. In a path, `\`, `>` and a `#` before trailing digits are backslash-escaped (`Issue \#12`, `A \> B`) so every printed path resolves to its own Section. `edit <id> --section "A > B" <path|->` replaces that Section; the new text must start with a heading line of the same level, contain no shallower heading, and not change the structure of the rest of the document (e.g. an unclosed code fence). `--base` works with it.

### list

Newest activity first. Each row: id, Kind, name, latest Revision, open Thread count, author, stored bytes, last-updated time, and project and worktree.

Output is `{"artifacts": [...]}`. Stored bytes is the sum of the distinct blobs any Revision of the Artifact references. `--session` (the global flag, not the `CLAUDE_CODE_SESSION_ID` fallback) and `--mine` match the Session or author of the latest Revision (the one the author column shows); `--mine` uses the caller's Session if it has one, else its author. `--older-than` takes `30d`, `12h`, `90m`, `45s` or `2w` and compares last-updated. `--text` columns: id, kind, name, rev, open, author, bytes, updated, project, worktree (`-` for null).

Filters: `--kind`, `--session`, `--mine`, `--open` (only Artifacts with an open Thread), `--older-than <duration>`, `--limit` (default 50), plus the scoping flags above. No cursor in v1. Defaults to the detected scope; unscoped outside a project.

### delete

`delete <id>... --yes`. Several ids; no prompt and no TTY detection. Removes everything for each Artifact in one transaction ([data-model.md](data-model.md#revisions)). Without `--yes` it exits 2; an unknown id exits 3 and deletes nothing. Prints `{"deleted": [ids]}`.

### comment

- `comment add <id>[@rev] <body|->` with `--lines a-b` and, for markdown, `--section "A > B"`. `--reply <comment-id>` replies in a Thread and takes no locator.
- `comment resolve <thread-id>` and `comment reopen <thread-id>`.
- `comment list [<id>]` shows open Threads, on any Revision, for the caller's Artifacts: with a Session it is those of that Session (project ignored); with none it falls back to the same scope as `list` and takes the same two widening flags. `--all` includes resolved Threads. `--since <id>` returns only Threads holding a Comment newer than the id (the whole Thread, replies included), the incremental view; the open/`--all` rule still applies on top, so a new reply in a resolved Thread surfaces only with `--all`. With an explicit `<id>` the Session scope is ignored. `--lines` takes `a-b` or a single line; a reply (`--reply <root-comment-id>`) conflicts with both locators. `--wait` blocks until there is something new, `--timeout <secs>` (default 60).
- Output includes `highest_comment_id`, the highest id ever issued (it does not drop when an Artifact is deleted); every read returns it so the Agent passes it as the next `--since`. Each Thread shows `revision`, `latest_revision`, `stale`, the quoted text and the locator.
- `--wait` is the CLI polling sqlite every 500 ms; it needs no server. A timeout is exit 5.

### serve

`serve` runs axum on `127.0.0.1` only: `--port` (default 7878, env `CAIRN_PORT`) for the UI and API, `--content-port` (default port + 1, env `CAIRN_CONTENT_PORT`) for untrusted content. `--reviewer-name` (env `CAIRN_REVIEWER`, default `reviewer`) is the author the UI posts under, always with role reviewer, separate from the global `--author`.

### view

`view <id>[@rev]` prints `<base>/a/<id>[@rev]` and does not open a browser or check a server is running. The base comes from `--url`, then `CAIRN_URL`, then `http://127.0.0.1:<port>` with the port from `CAIRN_PORT` or 7878. The skill tells Agents to ask the human to run `cairn serve`.

### skill

`skill` prints a SKILL.md; `skill --check <path>` compares an installed copy. See [skill.md](skill.md).
