---
title: Revisions, comments and attribution
type: grilling
status: closed
assignee: jeffutter
blocked_by: []
---

## Question

How do edits keep history, and what do comments attach to? Decide: whether revisions are immutable full snapshots or deltas; how a Revision is identified; what a Comment anchors to (a Revision, plus optionally a markdown section or line, and what happens to the anchor when a later Revision changes it); who authored a comment or revision (agent name, reviewer) and how that is recorded; and whether comments are threaded or resolvable. Sharpens the Revision and Comment glossary terms.

## Resolution

- **Storage**: every Revision is an immutable full snapshot, never rewritten. An Artifact is a stable identity whose content is its latest Revision.
- **Identity**: Artifact id is a short random id (about 8 chars, typeable); a Revision is a per-artifact integer from 1, written `abc123@3`. "Latest" is the default when no revision is given. No global revision ids or content hashes in the user-facing id.
- **Attribution**: Revisions and Comments record `author` (free-text name) and role (`agent` | `reviewer`). Agents supply the name via a flag or `CAIRN_AGENT`, falling back to `agent`. The reviewer is just `reviewer` (no accounts). Names are unverified labels.
- **Session**: Revisions and Comments also record a nullable opaque `session`, taken from `--session`, then `CAIRN_SESSION`, then `CLAUDE_CODE_SESSION_ID` (observed set in a Claude Code Bash shell; stability across resume and subagents not verified). It is for filtering (for example "my artifacts", "comments on my artifacts"), never access control. Artifacts are not owned: any session may add a Revision. Exact flags belong to the CLI surface ticket.
- **Anchoring**: every Comment belongs to exactly one Revision, with an optional locator. Text kinds: optional line range, with the quoted text stored on the comment. Markdown: optionally a heading section (address format deferred to the markdown section addressing ticket). Image and diagram: whole revision only for now.
- **Drift**: anchors are never rewritten when later Revisions change the text. The UI may map old Comments onto newer Revisions by diff at display time. Unresolved Comments on older Revisions must still reach the agent; how is for the agent-reviewer-loop ticket.
- **Threads**: one level (root plus flat replies), replies from either role, a resolved/open flag on the thread root toggled by either party. Comments are immutable once posted (no edit or delete in v1).
- **Deletion**: whole-Artifact hard delete only (all Revisions and Comments). No per-Revision pruning; cleanup is the storage-limits fog.
