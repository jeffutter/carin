---
id: TASK-1
title: Nix flake and CLI skeleton
status: Done
assignee: []
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T00:10:00Z'
labels:
  - planned
dependencies: []
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A buildable, checkable project foundation. `nix build` produces the `cairn` binary, `nix flake check` runs build, clippy, fmt and nextest, and the dev shell supports the `cargo watch` loop. The binary has the full global flag set with env fallbacks, the single core error enum with stable codes mapped to exit codes 0-6, JSON errors on stderr, and database path resolution (flag, env var, XDG data dir, with fallback). Follow the architecture doc's flake layout (notectl and graphql-document-utils as the reference). Every dependency comes from nix.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `nix build` and `nix flake check` pass (build, clippy, fmt, nextest)
- [x] #2 Dev shell provides the toolchain, clang and mold on Linux, cargo-watch and sqlite
- [x] #3 Global flags `--role`, `--author`, `--session`, `--db`, `--project` parse, with their env vars and the documented defaults and fallbacks
- [x] #4 The error enum maps each variant to the documented exit code and prints a JSON error with a stable `code` on stderr
- [x] #5 Database path resolves as `--db`, then `CAIRN_DB`, then the XDG data dir, with the home-directory fallback; unit tested
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-1. This plan is superseded; the final summary describes what landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added flake.nix (crane + rust-overlay; checks build/clippy/fmt/nextest; dev shell with clang, mold on Linux, cargo-watch, cargo-nextest, sqlite), lib+bin split (error.rs, config.rs, cli.rs), integration test in tests/cli.rs. Verified: nix flake check passed, nix build + --help + bad flag (exit 2, JSON stderr), dev shell tools present. Only x86_64-linux checked.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Flake, global-flag CLI skeleton, error enum with exit codes and JSON stderr, and DB path resolution, all unit and integration tested.
<!-- SECTION:FINAL_SUMMARY:END -->
