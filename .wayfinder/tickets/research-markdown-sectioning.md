---
title: Research Rust crates for heading-based Markdown sectioning
type: research
status: open
assignee: jeffutter
blocked_by: []
---

## Question

Which Rust crates (pulldown-cmark, comrak, markdown-rs, others) can parse Markdown into heading-delimited sections with exact byte offsets so a section can be read and replaced without reformatting the rest of the document? Note how each handles code fences containing `#`, setext headings, duplicate heading text, and front matter. Check how `notectl-outline` in github.com/jeffutter/notectl does it, as prior art. Surface facts only; the decision is the Markdown section addressing ticket.
