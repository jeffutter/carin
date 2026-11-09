---
title: Artifact kinds and storage
type: grilling
status: closed
assignee: jeffutter
blocked_by: [revisions-and-comment-anchoring, diagram-format]
---

## Question

What is the final set of Kinds (markdown, single-page HTML, HTML with images, image, diagram), and how is each stored per Revision in sqlite (blob versus files on disk)? In particular: how is an HTML page with images submitted and stored (a bundle, inlined data URIs, or a directory), how is Kind detected or declared, and what size limits apply? Sharpens the Kind glossary term.

## Resolution

Decided with the user (grilling), 2026-11-09. See ADR 0003 and ADR 0004.

- **Content model**: every Revision is a set of one or more files (path, media type, bytes). Markdown, image and diagram hold one file; `html` is a Bundle with a required `index.html` plus assets under relative paths. A single-page HTML artifact is a bundle of one file. Revisions stay full snapshots, so editing a bundle resubmits the whole bundle. Submit takes a file or a directory.
- **Kinds**: `markdown`, `html`, `image` (png, jpeg, gif, webp) and `diagram` (format `mermaid` or `svg`). SVG is never an `image`.
- **Declaration**: `--kind` explicitly, or inferred from extension (`.md`, `.html`, png/jpg/gif/webp, `.mmd`); ambiguous input errors, and `.svg` always needs `--kind diagram --format svg`. Never sniffed from content. Kind (and diagram format) is fixed per Artifact at creation.
- **Markdown images**: markdown is a single file. It embeds image and diagram Artifacts with `![alt](cairn:abc123)` (latest) or `cairn:abc123@2` (pinned). At submit the server rejects missing targets and resolves unpinned references to the current Revision into a side table; stored text is never rewritten. `markdown` and `html` cannot be embedded. `http:`, `data:` and relative image URLs are not fetched and show as alt text. Deleting an embedded image is allowed; referencing markdown shows a missing-image placeholder.
- **Storage**: sqlite BLOBs only, in a `revision_files` table keyed by (revision, path); no files on disk. Contents are content-addressed (sha256) so Revisions share unchanged blobs; hard delete removes blobs no Revision references.
- **Limits** (hard-coded constants, not configuration): 10 MiB per file, 25 MiB per Revision, 200 files per bundle; over-limit submits are rejected naming the limit.

Handed on: the CLI surface ticket owns the submit flags, directory submit and any local-image upload convenience; the Web safety model ticket owns how blobs are served (Content-Type from the stored media type, `cairn:` embeds shown via `<img>` from the untrusted origin).
