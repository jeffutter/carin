---
title: Markdown section addressing
type: grilling
status: closed
assignee: jeffutter
blocked_by: [research-markdown-sectioning, revisions-and-comment-anchoring]
---

## Question

How does an Agent name a markdown section to read or replace (heading text, a path of headings, an index, a stable id)? Decide the behaviour for duplicate headings, renamed or removed headings between Revisions, nested subsections (does a section include its children), edits that change the heading itself, and how a section edit produces a new Revision. Also decide whether comment anchors to a line or section share this addressing.

## Resolution

- **Address**: a Section is named by a path of heading texts, for example `Design > Storage > Limits`. Segments match heading text exactly (case-sensitive, trimmed, inline markup stripped). A suffix of the path is accepted when it matches exactly one section. No positional index is accepted as an address, because it shifts on insert.
- **Duplicates**: an ambiguous path fails and lists the candidates, each with its 1-based occurrence. The Agent retries with an occurrence suffix such as `Notes#2`. A section edit never guesses.
- **Nesting**: a Section runs from its heading to the next heading of the same or a higher level, so it includes its children. An outline command lists every heading with its path and size so an Agent can pick a narrow section. No `--shallow` flag.
- **Replace**: the replacement text includes the heading line and must begin with a heading line. Renaming or re-levelling a heading is an ordinary edit. Extra headings inside the replacement become new subsections.
- **Revisions**: a section edit applies to the latest Revision only, splices into the full document, and stores a new full-snapshot Revision. It fails if the path no longer matches. No base-Revision check here; the concurrency fog revisits it. A section read may target a given Revision (`abc123@3`).
- **Comment anchors**: a Comment's optional section locator uses the same path format, resolved only against the Comment's own Revision. The stored line range and quoted text remain the fallback when a later Revision renames the heading.
- **Parsing**: pulldown-cmark. Only top-level headings are section boundaries (not those inside blockquotes or list items). Front matter and text before the first heading form an unaddressable preamble, changed only by whole-document replace. CRLF handling is a build-time test, not decided here.
- **Deferred idea (user)**: temporary short section ids, returned when an Agent submits or views a document, that the Agent could use instead of a path. Not in v1; the path syntax must leave room for a distinct id syntax later. Recorded in the map's Not yet specified.
