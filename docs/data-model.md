# Data model

Terms are defined in [`../GLOSSARY.md`](../GLOSSARY.md). This page is the logical model; exact column types and indexes are a build-time detail.

## Ids

- Artifact id: a short random id, about 8 characters, typeable (`abc123`).
- Revision: a per-Artifact integer from 1, written `abc123@3`. Where an Artifact is named, `abc123` means the latest Revision and `abc123@3` a specific one.
- Comment id: a monotonically increasing integer across the database. `--since <id>` depends on this.
- No global Revision ids and no content hashes in user-facing ids.

## Tables (logical)

**artifacts**: `id`, `kind` (`markdown` | `html` | `image` | `diagram`), `format` (diagram only: `mermaid` | `svg`), `name`, `project_name`, `project_root`, `worktree` (the three nullable), `created_at`, `updated_at`. Kind and format are fixed at creation. The scope columns are fixed at creation. An Artifact is not owned: any Session may add a Revision.

**revisions**: `artifact_id`, `number`, `author`, `role` (`agent` | `reviewer`), `session` (nullable), `created_at`. Immutable.

**revision_files**: keyed by (`artifact_id`, `revision`, `path`), with `media_type`, `size`, `sha256`. Markdown, image and diagram Revisions have one file; an html Bundle has one or more.

**blobs**: `sha256`, `bytes`. Content-addressed, so Revisions that share an unchanged file share one blob. There is no files-on-disk store ([ADR 0003](adr/0003-store-revisions-as-file-sets-in-sqlite.md)).

**embeds**: a side table recording, for each markdown Revision, each `cairn:` reference and the image or diagram Revision it resolved to at submit ([ADR 0004](adr/0004-markdown-embeds-images-by-artifact-reference.md)). The stored markdown text is never rewritten.

**comments**: `id`, `artifact_id`, `revision`, `parent_id` (null for a Thread root), `author`, `role`, `session` (nullable), `body`, `created_at`, and the optional locator: `line_start`, `line_end`, `quoted_text`, `section_path`. A Thread root also carries an `open` / `resolved` flag. That flag is the only mutable part of a Comment; Comments are otherwise immutable, with no edit and no delete in v1.

Optionally a per-Artifact "last seen" marker for the web UI ([web-ui.md](web-ui.md#reviewer-attention)); it is a hint, not part of any contract.

## Kinds and files

| Kind | Files | Accepted input |
| --- | --- | --- |
| `markdown` | one | `.md` |
| `html` | a Bundle: required `index.html` plus assets at relative paths | `.html`, or a directory |
| `image` | one | png, jpeg, gif, webp. Never svg. |
| `diagram` | one | format `mermaid` (`.mmd`) or `svg` |

- The Kind is given with `--kind` or inferred from the extension. It is never sniffed from content. Ambiguous input is an error, and `.svg` always needs `--kind diagram --format svg`.
- A single-page html Artifact is a Bundle of one file. Revisions are full snapshots, so editing a Bundle resubmits the whole Bundle.
- Media types are checked against an allowlist at submit and stored with each file.
- Mermaid: the source is validated by rendering it with Pele at submit; invalid source is rejected with Pele's error code and message. Only the source is stored; SVG is derived on demand and not cached ([ADR 0002](adr/0002-render-mermaid-server-side-with-pele.md)). Raw SVG gets at most a well-formedness check, and neither format is sanitised on the server, because containment is the untrusted origin ([web-ui.md](web-ui.md#untrusted-content)).
- Diagrams render with fixed light-neutral Pele variables (links and images off, options never author-supplied) on a light background. Dark-mode diagrams are deferred; D2 and Excalidraw are wanted later and need only a new renderer, since `format` is an open attribute.

### Limits

Hard-coded constants, not configuration: 10 MiB per file, 25 MiB per Revision, 200 files per Bundle. An over-limit submit is rejected, naming the limit. There is no total cap, no retention and no expiry.

## Markdown embeds

A markdown Artifact is one file with no attached assets. It shows an image with `![alt](cairn:abc123)` (latest) or `![alt](cairn:abc123@2)` (pinned). Only `image` and `diagram` Artifacts can be embedded. At submit a reference to a missing Artifact or Revision is rejected (exit 4) and each unpinned reference is resolved to the target's current Revision into the `embeds` table. `http:`, `data:` and relative image URLs are not fetched and show as alt text. Deleting an embedded Artifact is allowed; the referencing markdown shows a missing-image placeholder.

## Sections

A Section is named by a path of heading texts, for example `Design > Storage > Limits`.

- Segments match heading text exactly: case-sensitive, trimmed, inline markup stripped. A unique suffix of the path is accepted (`Limits`). No positional index is an address, because indexes shift on insert. There are no short section ids; heading paths are the only address.
- A Section runs from its heading to the next heading of the same or a higher level, so it includes its children.
- An ambiguous path fails and lists the candidates with their 1-based occurrence; retry with `Notes#2`. A section edit never guesses.
- Parsing uses pulldown-cmark. Only top-level headings are boundaries (not those inside blockquotes or list items). Front matter and text before the first heading form an unaddressable preamble, changed only by a whole-document edit.
- Replace: the replacement text includes the heading line and must start with a heading line. Renaming or re-levelling is an ordinary edit. Extra headings inside the replacement become subsections. A section edit splices into the latest Revision and stores a new full Revision; it fails if the path no longer matches.
- Read: a section read may target a given Revision (`abc123@3`).
- Outline: lists every heading with its path and size, no ids.

## Revisions

Every edit makes a new immutable full snapshot; nothing is rewritten and no Revision is ever deleted on its own. The only removal is whole-Artifact hard delete, which removes all Revisions and Comments and the blobs no remaining file row references, in one transaction.

An edit may name Comment ids it responds to (`--addresses 12,14`). For each, Cairn posts a reply in that Comment's Thread, "addressed in abc123@4", and does not resolve it. The reply is posted as the editing Revision's author and role. Only the Reviewer resolves; an Agent never closes a Thread on its own authority.

## Comments and Threads

- Every Comment belongs to exactly one Revision, with an optional locator. Text content (markdown, mermaid source) takes an optional line range with the quoted text stored on the Comment. Markdown may also take a Section path, resolved only against the Comment's own Revision; the line range and quote are the fallback when a later Revision renames the heading. Image, diagram and html Comments cover the whole Revision.
- Anchors are never rewritten when later Revisions change the text.
- A Thread is a root Comment plus flat replies (one level). Either role may reply; either may resolve or reopen. A reply has no locator and stays on its Thread's Revision.
- A Comment is never refused for being on an older Revision. Reads report `revision`, `latest_revision`, and a derived `stale` flag (the Comment's Revision is not the latest). Open stale Threads are included in the default view, so unresolved feedback on an old Revision still reaches the Agent.

## Attribution

Revisions and Comments record `author` (free text) and `role`. Both are unverified labels, not identity. `role` defaults to `agent`. The author comes from the flag or `CAIRN_AGENT`, falling back to `agent`; the reviewer defaults to `reviewer`. `session` is an opaque nullable label from `--session`, then `CAIRN_SESSION`, then `CLAUDE_CODE_SESSION_ID`, used only for filtering ("my Artifacts", "comments on my Artifacts"), never for access control. Role is attribution only; both roles may run every operation, including delete.

## Project and worktree scope

Detection shells out to `git rev-parse --path-format=absolute --git-common-dir --show-toplevel` from the cwd. The root checkout is the parent of the common dir; the project name is its last path segment. The worktree is the toplevel and counts as linked only when it differs from the root, otherwise it is null. Outside a repo, with `git` missing, or in a bare repo there is no scope and never an error. A submodule is its own project.

The Artifact stores `project_name` (filter, display), `project_root` (absolute path, disambiguates two checkouts that share a last segment) and `worktree`. Revisions and Comments carry no project. Moves and renames are not handled in v1.

Default scope for CLI lists is strict: in a linked worktree, Artifacts of the project whose worktree is null (the root) or this worktree, excluding sibling worktrees; in the root checkout, root Artifacts only. `--all-worktrees` widens to the whole project and `--all-projects` drops the project filter. The API and web UI default to unscoped. Flags are in [cli.md](cli.md#scoping).

## Concurrency

All writes use `BEGIN IMMEDIATE` on connections set to WAL, `foreign_keys=on`, `synchronous=NORMAL` and a 5 s `busy_timeout`. The next Revision number and the Artifact-exists check happen inside the transaction. Reads use plain transactions and never block on writers. A busy timeout exits 1 (database busy); a write to a deleted Artifact exits 3.

`edit --base <rev>` (API `base_revision`) refuses a stale base: if the latest Revision is not `<rev>` the edit fails with exit 6 (HTTP 409), naming `latest_revision` and its author. Without `--base` the edit applies to the latest. The skill tells Agents to pass `--base` ([ADR 0007](adr/0007-immediate-write-transactions-and-optional-base-check.md)).

## Cleanup

No pruning, retention, archive or total cap; only the per-file, per-Revision and per-Bundle limits. `list` (CLI, API, web UI) shows each Artifact's stored bytes and last-updated time; see [cli.md](cli.md#list) for `--older-than`. `delete` takes several ids and requires `--yes`.
