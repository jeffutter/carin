# 13: Web UI: list and read-only Artifact page

**What to build:** A Reviewer opens the UI and browses. The server-rendered UI (maud, htmx fragments, no npm, calling core in-process) shows a list table (name, Kind, project and worktree, latest Revision, open Thread count, size, updated; filterable by project and Kind; unscoped by default; newest activity first) and the markdown Artifact page: header with name, Kind and a revision dropdown, a banner linking to the latest when an old Revision is viewed, and a sticky outline sidebar with open-Thread counts per Section. Markdown renders with raw HTML dropped, `javascript:` and `data:` links stripped, external links `noopener noreferrer`, and all text escaped. The UI sets its own strict CSP. htmx is pinned with `fetchurl` and embedded with the hand-written CSS, served at a versioned URL. Match the look of the throwaway prototype. `cairn view <id>[@rev]` prints the URL (from `--url`, `CAIRN_URL`, or the default) without checking a server.

**Blocked by:** 12: `serve` with the untrusted content port, 05: Markdown Sections

**Status:** ready-for-agent

- [ ] List and markdown page render real data; filters work
- [ ] Revision dropdown navigates Revisions; old-Revision banner links to the latest
- [ ] A markdown file containing raw HTML and script links renders inert (tests assert it)
- [ ] Titles, author names and comment text are escaped
- [ ] Response CSP on UI pages is strict with `frame-ancestors 'self'`
- [ ] The nix build embeds the pinned htmx; no network access at build time beyond the fetch
- [ ] Layout collapses to one column below about 800 px
- [ ] `view` prints the correct URL for all three base sources
