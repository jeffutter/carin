---
title: CLI surface and output contract
type: grilling
status: open
assignee:
blocked_by: [revisions-and-comment-anchoring, kinds-and-storage, markdown-section-addressing, agent-reviewer-loop]
---

## Question

What are the commands and arguments for the agent and reviewer functions (submit, get, edit, comments, delete, list, view, comment, serve, skill)? Decide naming, how artifact content is passed (path, stdin), the output contract (JSON versus human text, flags), exit codes, and how the single binary separates the server subcommand from the database-manipulating ones. Reviewer functions available through the CLI too, as the brief requires.
