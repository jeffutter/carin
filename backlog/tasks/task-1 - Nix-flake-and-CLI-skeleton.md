---
id: TASK-1
title: Nix flake and CLI skeleton
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
dependencies: []
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A buildable, checkable project foundation. `nix build` produces the `cairn` binary, `nix flake check` runs build, clippy, fmt and nextest, and the dev shell supports the `cargo watch` loop. The binary has the full global flag set with env fallbacks, the single core error enum with stable codes mapped to exit codes 0-6, JSON errors on stderr, and database path resolution (flag, env var, XDG data dir, with fallback). Follow the architecture doc's flake layout (notectl and graphql-document-utils as the reference). Every dependency comes from nix.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `nix build` and `nix flake check` pass (build, clippy, fmt, nextest)
- [ ] #2 Dev shell provides the toolchain, clang and mold on Linux, cargo-watch and sqlite
- [ ] #3 Global flags `--role`, `--author`, `--session`, `--db`, `--project` parse, with their env vars and the documented defaults and fallbacks
- [ ] #4 The error enum maps each variant to the documented exit code and prints a JSON error with a stable `code` on stderr
- [ ] #5 Database path resolves as `--db`, then `CAIRN_DB`, then the XDG data dir, with the home-directory fallback; unit tested
<!-- AC:END -->
