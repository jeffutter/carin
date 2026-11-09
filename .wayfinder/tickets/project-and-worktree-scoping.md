---
title: Project and worktree scoping
type: grilling
status: open
assignee:
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
