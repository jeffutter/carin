---
id: TASK-8
title: Project and worktree scoping
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T00:46:00Z'
labels:
  - planned
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
- [x] #1 Tests build real repos and linked worktrees in temp dirs and check the stored name, root and worktree
- [x] #2 Default scope excludes sibling worktrees; both widening flags work
- [x] #3 A name that maps to several roots is narrowed by `--project-root`
- [x] #4 A name on a write exits 2 (rejected); a path replaces the detected root
- [x] #5 No-repo and git-missing cases succeed with no scope
- [x] #6 `list` shows project and worktree columns
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by f66a89e. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added src/scope.rs (git rev-parse detection, --project forms, ScopeFilter SQL fragment shared by list and comment list). submit stores project_name/root/worktree; a name on submit exits 2. list and comment list gained --all-projects, --all-worktrees, --project-root. Submodule root: common dir not named .git means toplevel is the root. --all-projects keeps the strict worktree rule (documented in docs/cli.md). ListRow JSON gained project_root. Existing test helpers now run from the temp dir with CAIRN_PROJECT and GIT_* cleared. Tests in tests/cli.rs mod scoping.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Project and worktree scoping implemented: detection, storage on submit, strict default scope with widening flags in list and comment list, name-on-write rejection. cargo test, clippy and fmt pass.
<!-- SECTION:FINAL_SUMMARY:END -->
