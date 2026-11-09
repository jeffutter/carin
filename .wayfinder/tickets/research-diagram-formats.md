---
title: Research diagram formats and safe browser rendering
type: research
status: closed
assignee: jeffutter
blocked_by: []
---

## Question

Which diagram formats could an Agent submit (Mermaid, SVG, Excalidraw JSON, Graphviz/D2, others)? For each: how authorable it is for an LLM, how it renders in a browser, whether rendering needs JS or a server-side step, and the security surface (script in SVG, etc.). Surface facts only; the decision is the Diagram format ticket.

## Resolution

Findings are on branch `research/diagram-formats` (commit 2cfbf67, local only, never merged) in `docs/research/diagram-formats.md`. It covers Mermaid, raw SVG, Excalidraw JSON, Graphviz DOT, D2 and PlantUML: browser rendering, server-side options and security surface, each cited to first-party docs, plus a containment section that applies to any SVG or HTML output.

Gaps the file itself lists as not found: no primary source on LLM authorability (only syntax facts); mermaid-cli's rendering engine, D2's PNG mechanism, D2 WASM support and Excalidraw's license are unconfirmed. The agent read pages through a summarizing fetch tool and ran no renderer, so quotes are second-hand; verify anything the Diagram format decision hinges on.
