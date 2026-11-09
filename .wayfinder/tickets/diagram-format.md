---
title: Diagram format
type: grilling
status: closed
assignee: jeffutter
blocked_by: [research-diagram-formats]
---

## Question

Which diagram format or formats does Cairn accept as a Kind, and where are they rendered (browser, server, never)? Weigh authorability for agents against rendering safety, using the research findings.

## Resolution

Decided with the user (grilling), 2026-11-09.

- **Formats in v1**: Mermaid and raw SVG. D2 and Excalidraw are wanted later; DOT and PlantUML are dropped. `format` is an open attribute on a diagram so a new format is a new renderer plus a containment decision, not a data-model change.
- **Kind**: one Kind `diagram` with a required `format` (`mermaid` or `svg`). `image` stays binary-only.
- **Mermaid renderer**: Pele (github.com/obsidianmd/pele, MIT, npm `pele` 1.13.0, no runtime dependencies, synchronous `render()` returning an SVG string, no scripts, event handlers or HTML labels in output, `javascript:`/`data:` links blocked, limits on source and output size). Chosen over Mermaid.js, which needs a browser or Chromium.
- **Rendering location**: server-side at submit, with Pele in an embedded JS engine inside the one binary (for example QuickJS). Cairn stores only the source; the SVG is derived and cacheable. If embedding turns out not to build in nix, the fallback is browser rendering with `pele.global.js`.
- **Validation**: an invalid Mermaid diagram is rejected at submit, returning Pele's `PeleError` code and message, so the Agent sees the failure immediately. Raw SVG gets at most a well-formedness check.
- **Containment**: both formats end as SVG shown through `<img>` (or the sandboxed frame) from the untrusted origin, never inline in the review page. Pele is called with `links: false, images: false` and the options are never author-supplied. No server-side SVG sanitizer. Exact sandbox tokens and CSP belong to the Web safety model ticket.
- **Theming**: render once with fixed light-neutral Pele variables, shown on a light background. Dark-mode diagrams deferred.

Verified this session (scratch probe, not committed): Pele 1.13.0 from npm rendered a flowchart identically under Node and under QuickJS (2026-06-04 build from nixpkgs) with no `window` or `document`; output contained no script, event handler or `javascript:` text even for a `click ... "javascript:alert(1)"` line; invalid source threw `PeleError` with code `syntax` and a line-numbered message.

Not verified: that an embedded engine crate (for example rquickjs) builds in the nix flake; performance and memory per render; how Pele's CSS variables behave inside `<img>`; per-browser `<img>` SVG restrictions (MDN says they "may apply, depending on the browser"); Pele's Mermaid coverage gaps versus Mermaid.js (see pele.run/compatibility, not read).
