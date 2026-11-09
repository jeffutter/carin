---
title: Spike: embedded JS engine for Mermaid in nix
type: prototype
status: closed
assignee: jeffutter
blocked_by: []
---

## Question

Does an embedded JS engine (for example rquickjs) running Pele build cleanly under nix, what does a render cost, should derived SVG be cached (and where in the file-set storage), and does Pele's CSS-variable theming work inside an `<img>`? Throwaway spike under .wayfinder/assets/; informs ADR 0002 (status may need amending).

## Resolution

Measured 2026-11-09 with a throwaway crate at `.wayfinder/assets/mermaid-engine-spike/` (flake in the notectl layout, crane, rquickjs 0.14, Pele 1.13.0 `pele.global.js` via `fetchzip`, embedded with `include_str!`). Not committed to a throwaway branch beyond this directory.

- **Builds in nix**: yes. `nix build` succeeded first try for rquickjs (bundled QuickJS via `rquickjs-sys`, needs only `clang` in `nativeBuildInputs`); about 50 s cold including deps. The ADR's fallback (browser rendering) is not needed.
- **Cost**: engine creation plus parsing Pele takes about 70-85 ms once per process; a small flowchart renders in about 0.8-1 ms warm; a 150-node flowchart (300 edges, 112 KB SVG) in about 55 ms. Resident memory about 13 MB. Measured in release mode on one Linux machine, single thread.
- **Caching**: not needed. Render on demand from the stored source; keep a long-lived engine inside `serve` (the 75 ms init is paid once) and let CLI submit pay it once per invocation. Derived SVG is not stored, so no storage-layout change to ADR 0003.
- **Theming in `<img>`**: Pele's variables all carry fallbacks (`var(--pele-bg,#fff)` etc.) so the SVG renders with its neutral defaults inside `<img>`. Checked in Chromium 148: page-level `--pele-bg:red` did not leak into the image, as expected. Diagrams therefore stay fixed light-neutral, matching the Diagram format decision; custom variables could only be applied by rewriting the SVG's `style` attribute at render time, not by CSS.
- **Error path**: an invalid source surfaces as a JS exception from `eval`; rquickjs returns a generic `Exception`, so the real `PeleError` code and message must be read with `ctx.catch()` (not done in the spike; to handle at build time).
- **Not checked**: Pele coverage gaps against Mermaid.js, other browsers' `<img>` SVG rules, Darwin builds, behaviour under hostile inputs beyond Pele's own size limits (run the render under a QuickJS memory and interrupt limit at build time).
