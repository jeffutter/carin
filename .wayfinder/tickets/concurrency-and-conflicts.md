---
title: Concurrency and conflicts
type: grilling
status: open
assignee:
blocked_by: []
---

## Question

What happens when an Agent edits while a Reviewer comments, or two Agents edit the same Artifact? Decide sqlite locking and transaction mode across CLI processes and the server, whether edits carry a base Revision (optimistic concurrency, a conflict exit code), and what a Comment on a now-superseded Revision looks like to the Agent. Builds on the Revisions and Comments ticket and the CLI exit codes.
