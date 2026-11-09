---
title: Config and data locations
type: grilling
status: closed
assignee: jeffutter
blocked_by: []
---

## Question

Where do the sqlite database and any config live (XDG data and config dirs, env var overrides), and how does the CLI find the database? Also decide how the server shares it, the serve flags for the UI port and the untrusted-content port (ADR 0005), and the web UI's author flag name and default (left open by the web UI prototype ticket). Consider one database per user (project and worktree scoping ticket) and the single-binary topology (ADR 0001).

## Resolution

Decided with the user (grilling), 2026-11-09. No ADR: every point is easy to change.

- **Database path**: `--db`, then `CAIRN_DB`, then `$XDG_DATA_HOME/cairn/cairn.db` (fallback `~/.local/share/cairn/cairn.db`, also on macOS). Directory and schema are created on first use by any subcommand; there is no `init`.
- **No config file in v1**: everything is a flag with an env var; the database is the only state on disk.
- **Serve**: `--port` (UI, default 7878, env `CAIRN_PORT`) and `--content-port` (untrusted origin, default UI port + 1, env `CAIRN_CONTENT_PORT`). Both listeners are always `127.0.0.1`, no `--bind`. A taken port is a clear non-zero exit, never a random port (the CSP `frame-ancestors` needs a predictable origin).
- **`view`**: prints `http://127.0.0.1:<port>/a/<id>` from `CAIRN_PORT` or 7878 without checking a server is running (ADR 0001); the skill tells Agents to ask the human to run `cairn serve`.
- **UI author**: `serve --reviewer-name` (env `CAIRN_REVIEWER`, default `reviewer`), role always reviewer, separate from the global `--author`.
- **Polling**: fixed 2 s htmx poll on the artifact page, paused while the tab is hidden, no flag. Uses the same latest-comment check as `--wait`.
