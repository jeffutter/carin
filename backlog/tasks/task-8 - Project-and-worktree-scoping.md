---
id: TASK-8
title: Project and worktree scoping
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
dependencies:
  - TASK-4
  - TASK-6
ordinal: 8000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Artifacts are scoped to the git Project (and Worktree) they were created in. Detection shells out to `git rev-parse` from the cwd; the Artifact stores a fixed project name, root path and worktree. Outside a repo, without git, or in a bare repo there is no scope and never an error; a submodule is its own Project. `--project` accepts a root path (write or read) or a name (read only, rejected on writes), `none` or empty disables detection. `list` and `comment list` default to the strict scope (root checkout: root Artifacts only; linked worktree: root plus that worktree), widened by `--all-worktrees`, `--all-projects`, and narrowed by `--project-root`. `comment list` with a Session ignores the project; with none it falls back to the same scope as `list`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tests build real repos and linked worktrees in temp dirs and check the stored name, root and worktree
- [ ] #2 Default scope excludes sibling worktrees; both widening flags work
- [ ] #3 A name that maps to several roots is narrowed by `--project-root`
- [ ] #4 A name on a write exits 2 (rejected); a path replaces the detected root
- [ ] #5 No-repo and git-missing cases succeed with no scope
- [ ] #6 `list` shows project and worktree columns
<!-- AC:END -->
