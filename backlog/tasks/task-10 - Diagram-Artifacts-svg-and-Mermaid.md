---
id: TASK-10
title: Diagram Artifacts (svg and Mermaid)
status: To Do
assignee: []
created_date: '2026-11-09T23:55:00Z'
labels: []
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
- [ ] #1 Valid Mermaid submits; invalid Mermaid exits 4 with Pele's error code and message
- [ ] #2 A pathological input hits the memory or interrupt limit and exits 4, not a hang or crash
- [ ] #3 Core exposes "render this Revision to SVG" for later use by the server
- [ ] #4 Malformed SVG is rejected; well-formed SVG is stored unsanitised
- [ ] #5 `nix build` and `nix flake check` pass with the engine included
- [ ] #6 Engine start-up cost is paid once per process and not on commands that never render
<!-- AC:END -->
