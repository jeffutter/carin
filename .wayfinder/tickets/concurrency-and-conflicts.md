---
title: Concurrency and conflicts
type: grilling
status: closed
assignee: jeffutter
blocked_by: []
---

## Question

What happens when an Agent edits while a Reviewer comments, or two Agents edit the same Artifact? Decide sqlite locking and transaction mode across CLI processes and the server, whether edits carry a base Revision (optimistic concurrency, a conflict exit code), and what a Comment on a now-superseded Revision looks like to the Agent. Builds on the Revisions and Comments ticket and the CLI exit codes.

## Resolution

Decided with the user (grilling), 2026-11-09. See ADR 0007.

- **Transactions**: all writes use `BEGIN IMMEDIATE`; connections set WAL, `foreign_keys=on`, `synchronous=NORMAL`, `busy_timeout` 5 s, opened only by the shared core. Revision numbering and the Artifact-exists check happen inside the transaction. Busy timeout exits 1 (database busy); a write to a deleted Artifact exits 3.
- **Conflicts**: `edit --base <rev>` (API `base_revision`) refuses a stale base with new exit code 6 (HTTP 409), reporting `latest_revision` and its author. No `--base` means apply to the latest, so a section edit stays lenient unless `--base` is given. The skill tells Agents to pass `--base`. This amends the CLI surface ticket's "No base-Revision check" and adds exit code 6.
- **Superseded Revisions**: Comments are never refused for being on an older Revision and stay on it. `comment list` shows `revision`, `latest_revision` and derived `stale`, plus quoted text and locator; the default open-Thread view includes stale Threads. Replies stay on the Thread's Revision; `--addresses` may name Threads from older Revisions.
- **Waiting**: `--wait` and long-poll poll the database every 500 ms (a constant in core) for Comments past the cursor, until the timeout (exit 5).
