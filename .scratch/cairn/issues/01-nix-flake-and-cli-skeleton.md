# 01: Nix flake and CLI skeleton

**What to build:** A buildable, checkable project foundation. `nix build` produces the `cairn` binary, `nix flake check` runs build, clippy, fmt and nextest, and the dev shell supports the `cargo watch` loop. The binary has the full global flag set with env fallbacks, the single core error enum with stable codes mapped to exit codes 0-6, JSON errors on stderr, and database path resolution (flag, env var, XDG data dir, with fallback). Follow the architecture doc's flake layout (notectl and graphql-document-utils as the reference). Every dependency comes from nix.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] `nix build` and `nix flake check` pass (build, clippy, fmt, nextest)
- [ ] Dev shell provides the toolchain, clang and mold on Linux, cargo-watch and sqlite
- [ ] Global flags `--role`, `--author`, `--session`, `--db`, `--project` parse, with their env vars and the documented defaults and fallbacks
- [ ] The error enum maps each variant to the documented exit code and prints a JSON error with a stable `code` on stderr
- [ ] Database path resolves as `--db`, then `CAIRN_DB`, then the XDG data dir, with the home-directory fallback; unit tested
