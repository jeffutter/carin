---
title: Project and worktree scoping
type: grilling
status: closed
assignee: claude
blocked_by: []
---

## Question

Artifacts should be scopable by the git project they came from, as well as by Session. Requirements from the user, to be sharpened here:

- Detect whether the CLI is run inside a git project.
- The project name comes from the last path segment of the root checkout (the main working tree), not from a linked worktree's own directory name, which may differ.
- When run inside a linked worktree, scope by both the root checkout and the current worktree.

Decide:
- How the project and worktree are detected (for example from the git common dir versus the worktree top level), and what happens outside a git project, in a bare repo, or when git is missing.
- Whether the project name alone is the scope key or the root path is too (two unrelated checkouts can share a last segment), and what happens when a directory is renamed or moved.
- What is stored and where: on the Artifact (fixed at creation) or on each Revision and Comment as Session is, and how it relates to Session.
- Whether detection is automatic with an override flag and env var, and how it fits the existing global flags.
- The default filter for `list` and `comment list` (all projects, or the current project and worktree), and the new filters on the CLI, the JSON API and the web UI.
- Whether a worktree scope means "this worktree only" or "this project, with this worktree highlighted".
- Whether this adds a glossary term (for example Project, Worktree) and whether it is hard to reverse enough for an ADR.

## Carried in from closed tickets

This amends decisions already recorded, so check each against the answer:
- Revisions, comments and attribution: authorship and the nullable Session label are stored on Revisions and Comments.
- CLI surface and output contract: global flags `--role`, `--author`, `--session`, `--db`; `list` filters `--kind`, `--session`, `--mine`, `--open`, `--limit`; `comment list` defaults to open Threads on the caller's Session Artifacts.
- JSON API shape: list filters mirror the CLI.
- Agent and reviewer feedback loop: the default `comment list` view is scoped by Session.
- The map's fog item on config and data directory locations: scoping by project is a field inside one database only if the database is shared; confirm that assumption.

## Resolution

Decided with the user (grilling), 2026-11-09. All answers are cheap to reverse; no ADR.

- **Detection**: shell out to `git rev-parse --path-format=absolute --git-common-dir --show-toplevel` from the cwd. Root checkout = parent of the common dir; the project name is its last path segment. The worktree is the toplevel, and counts as linked only when it differs from the root. Not in a repo, `git` missing, or a bare repo: no project scope, never an error. A submodule is its own project.
- **Key**: the Artifact stores both `project_name` (filtering, display) and `project_root` (absolute path, disambiguates two checkouts with one last segment). `--project <name>` matches the name; if a name maps to several roots, list output shows the roots and `--project-root <path>` narrows. No move or rename handling in v1.
- **Storage**: `project_name`, `project_root` and `worktree` (null in the root checkout) are nullable columns on the Artifact, fixed at creation. Revisions and Comments carry no project. Session stays on Revisions and Comments as decided.
- **Worktree semantics**: strict. In a linked worktree the default scope is the project's Artifacts whose worktree is null (root) or this worktree; sibling worktrees are excluded. In the root checkout the default is root Artifacts only (worktree null). `--all-worktrees` widens to the whole project.
- **Override**: one global `--project <path-or-name>` with env `CAIRN_PROJECT`. `none` or empty disables detection for the call. A value starting with `/` is a root path (replaces the detected root; name from its last segment). Any other value is a name only: accepted on reads as a filter, rejected on writes. No worktree override; the worktree comes from the cwd.
- **Defaults**: CLI `list` defaults to the detected scope; `--all-projects` drops the project filter, `--all-worktrees` widens to the project. Outside a project it is unscoped. `--mine` and `--session` still combine. `comment list` with a Session is unchanged (Session wins, project ignored); with no Session it falls back to the same scope and takes the same two flags.
- **API and web UI**: the JSON API list takes `project`, `project_root` and `worktree` filter params and defaults to unscoped (no cwd to detect from); `POST` creation takes `project_name`, `project_root`, `worktree` as body fields. The web UI defaults to all projects, groups or filters by project and labels the worktree; layout is for the prototype ticket.
- **One database per user**: confirmed. `--db` stays the only override; directory paths stay in the map's fog.
- **Glossary**: added Project and Worktree.

### Amends

- Revisions, comments and attribution: unchanged for Revisions and Comments; the Artifact gains the three scope columns.
- CLI surface: new global flag `--project`; `list` and `comment list` gain `--all-projects`, `--all-worktrees`, `--project-root`.
- JSON API shape: list filters add the three scope params; creation body adds the three fields.
- Agent and reviewer feedback loop: the sessionless `comment list` default now falls back to project scope.
