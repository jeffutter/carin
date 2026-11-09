---
title: Research Rust crates for heading-based Markdown sectioning
type: research
status: closed
assignee: jeffutter
blocked_by: []
---

## Question

Which Rust crates (pulldown-cmark, comrak, markdown-rs, others) can parse Markdown into heading-delimited sections with exact byte offsets so a section can be read and replaced without reformatting the rest of the document? Note how each handles code fences containing `#`, setext headings, duplicate heading text, and front matter. Check how `notectl-outline` in github.com/jeffutter/notectl does it, as prior art. Surface facts only; the decision is the Markdown section addressing ticket.

## Resolution

Findings are on branch `research/markdown-sectioning` (commit ac7a8f5, local only, never merged) in `docs/research/markdown-sectioning.md`. Unlike the other research tickets, the agent ran the crates on one probe document (pinned versions, scratch project outside the repo) rather than only reading docs. Headings checked, full text not re-read.

Facts the Markdown section addressing decision leans on, as the agent reported them:
- pulldown-cmark 0.13.6: byte-offset ranges via `into_offset_iter()`, including the trailing newline; front matter is opt-in (`MetadataBlock`).
- markdown-rs 1.0.0: byte offsets, no trailing newline; front matter is opt-in.
- markdown-it 0.6.1: byte offsets, no trailing newline; no front matter support; ATX and setext headings are separate node types.
- comrak 0.59.0: line:column only, with byte-based columns, so no byte offsets; front matter is opt-in.
- All four ignore `#` inside code fences and handle setext headings. None dedupes duplicate heading text in its tree.
- notectl-outline (prior art, read from source at commit 95975f9, tests not run): line-based ATX regex with a hand-written fence tracker, no parser crate; no setext support and no front matter handling (a `# comment` in YAML would read as a heading); returns every case-insensitive title match; read-only, no replace function; trims content and exposes only line numbers.

Not probed, per the file: CRLF, blockquotes and list items, and crates beyond these four.
