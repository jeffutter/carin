---
title: Temporary short section ids
type: grilling
status: closed
assignee: jeffutter
blocked_by: []
---

## Question

Are short section ids (hashes returned on submit or view) worth having as an alternative to heading paths, and if so how do they stay stable across edits? Weigh agent token savings against a second addressing scheme beside the one in the Markdown section addressing ticket; the answer may be no.

## Resolution

- **No short section ids, in v1 or planned.** Heading paths stay the only way to name a Section.
- **Why**: the saving is small, because any unique suffix of the path is already accepted (`Limits` instead of `Design > Storage > Limits`). Every stable-id rule has a cost: a hash of the heading breaks on rename, a hash of the content changes on every edit, and a stored counter needs a heading-to-id mapping carried across immutable Revisions (ADR 0003). A second scheme would also have to be explained in the skill, CLI help, API and Comment anchors, and a stale id fails less readably than a path.
- **Outline**: it lists paths and sizes only, with no id column.
- **Rejected alternative**: a per-Revision positional id (`#4` of `abc123@3`). It needs no stored state, but it is the positional addressing the Markdown section addressing ticket already rejected because indexes shift on insert.
- **Consequence**: the path syntax no longer needs to reserve room for an id syntax.
