---
title: Config and data locations
type: grilling
status: open
assignee:
blocked_by: []
---

## Question

Where do the sqlite database and any config live (XDG data and config dirs, env var overrides), and how does the CLI find the database? Also decide how the server shares it, the serve flags for the UI port and the untrusted-content port (ADR 0005), and the web UI's author flag name and default (left open by the web UI prototype ticket). Consider one database per user (project and worktree scoping ticket) and the single-binary topology (ADR 0001).
