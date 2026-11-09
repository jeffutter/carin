---
title: CLI surface and output contract
type: grilling
status: open
assignee:
blocked_by: [revisions-and-comment-anchoring, kinds-and-storage, markdown-section-addressing, agent-reviewer-loop]
---

## Question

What are the commands and arguments for the agent and reviewer functions (submit, get, edit, comments, delete, list, view, comment, serve, skill)? Decide naming, how artifact content is passed (path, stdin), the output contract (JSON versus human text, flags), exit codes, and how the single binary separates the server subcommand from the database-manipulating ones. Reviewer functions available through the CLI too, as the brief requires.

## Carried in from Artifact kinds and storage

Submit takes a file or a directory (a directory is an html Bundle needing `index.html`); `--kind` is explicit or inferred from extension, `.svg` needs `--kind diagram --format svg`. Markdown embeds images as `cairn:<id>[@<rev>]`; decide whether the CLI offers to upload local relative images and rewrite them. Size limits: 10 MiB per file, 25 MiB per Revision, 200 files per bundle.
