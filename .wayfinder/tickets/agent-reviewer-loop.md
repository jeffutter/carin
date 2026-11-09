---
title: Agent and reviewer feedback loop
type: grilling
status: closed
assignee: jeffutter
blocked_by: [revisions-and-comment-anchoring]
---

## Question

How does an Agent learn a Reviewer has commented? Decide between polling, a blocking `--wait` or `--since` style read, or something else, given there is no push channel from a one-shot CLI. Also decide what "unread" means for comments, and how an agent signals it has addressed a comment, if at all.

## Resolution

- **Mechanism**: polling is the base; a blocking `--wait` is a convenience on the same read. `--wait` is the CLI polling sqlite directly (no server needed, per ADR 0001), takes an explicit timeout (default 60s), and is overridable. The plain read with no `--wait` also covers "I told the agent to go pull the comments".
- **Unread**: no stored cursor. Comments have monotonically increasing ids; `--since <id>` returns everything newer. Every read prints the highest id it saw so the Agent passes it back next time. Chosen because there are no accounts, Sessions are only labels, and an Agent may lose context and restart.
- **Two views**, both filterable by Session: (1) open Threads on the Agent's Artifacts, on any Revision, the default and the source of truth, so unresolved Comments on older Revisions still reach the Agent; (2) new since id N, the incremental view used by `--since` and `--wait`.
- **Addressing**: no new state. The Agent replies in the Thread using existing mechanics. A new Revision may name the Comment ids it responds to (for example `--addresses 12,14`); Cairn posts an automatic reply in each Thread ("addressed in abc123@4") and does not resolve it. The Reviewer resolves; an Agent never closes a Thread by its own authority.
- **`--wait` semantics**: success with new Comments printed (exit 0), timeout with nothing new (a dedicated non-zero code), error (another). Output is JSON including the highest Comment id seen. Exact flag names and exit code numbers belong to the CLI surface ticket.
- **Reviewer side**: the web UI may mark Artifacts with a new Revision or Agent reply since the Reviewer last looked, via a per-Artifact "last seen" marker in sqlite. A hint for the web UI prototype, not a contract.
