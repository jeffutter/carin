---
id: TASK-10
title: Diagram Artifacts (svg and Mermaid)
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-09T23:55:00Z'
updated_date: '2026-11-10T01:08:00Z'
labels:
  - planned
dependencies:
  - TASK-3
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Agents submit diagrams as `--kind diagram` with format `svg` or `mermaid`. Mermaid source is validated at submit by rendering with Pele in an embedded QuickJS engine (`rquickjs`); invalid source is rejected with Pele's error code and message, read from the engine's exception. Only the source is stored, and SVG is rendered on demand (no cache) with fixed light-neutral variables, links and images off, options never author-supplied. The engine runs under a memory limit and an interrupt handler. Raw SVG gets only a well-formedness check. `get` prints the diagram source. Include the nix packaging: Pele's global script pinned with `fetchzip` and embedded, clang added to native build inputs. The throwaway spike under the wayfinder assets is reference material, not code to promote.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Valid Mermaid submits; invalid Mermaid exits 4 with Pele's error code and message
- [x] #2 A pathological input hits the memory or interrupt limit and exits 4, not a hang or crash
- [x] #3 Core exposes "render this Revision to SVG" for later use by the server
- [x] #4 Malformed SVG is rejected; well-formed SVG is stored unsanitised
- [x] #5 `nix build` and `nix flake check` pass with the engine included
- [x] #6 Engine start-up cost is paid once per process and not on commands that never render
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-10. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added src/diagram.rs (Format, validate, render_revision, thread-local lazy rquickjs engine with 64 MiB heap, 1 MiB stack, 5 s interrupt deadline; engine discarded after any non-Pele abort; roxmltree check for svg, DOCTYPE rejected). Wired format through bundle::prepare, artifact submit/edit (format re-read from the stored row inside the write tx; edit-time Mermaid render holds the DB lock up to 5 s, accepted), cli --format ValueEnum, lib.rs inference (.mmd implies mermaid; --format alone implies diagram). build.rs errors clearly without PELE_JS. flake.nix: Pele fetchzip pinned, PELE_JS in commonArgs and devShell; also added git to nextest nativeCheckInputs because pre-existing scoping tests need git in the sandbox (flake check failed on them before). Docs: cli.md, data-model.md limits, ADR 0002 status. nix build and nix flake check pass; nextest 147 tests pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Diagram Artifacts: mermaid (validated by rendering with Pele in an embedded QuickJS engine under memory, stack and time limits; invalid source exits 4 with Pele's code and message) and svg (well-formedness only, stored unsanitised). Core exposes diagram::render_revision for the server. Engine starts lazily once per thread. Nix packaging pins Pele; nix build and flake check pass.
<!-- SECTION:FINAL_SUMMARY:END -->
