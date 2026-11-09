# 06: Comments and Threads (CLI)

**What to build:** Reviewer and Agent discussion from the command line. `comment add <id>[@rev] <body|->` with `--lines a-b` (quoted text stored) and, for markdown, `--section "A > B"` resolved against the Comment's own Revision. `--reply <comment-id>` replies in a one-level Thread with no locator. `comment resolve` and `reopen` toggle the Thread flag, the only mutable part of a Comment. `comment list [<id>]` shows open Threads on any Revision for the caller's Artifacts (those of the Session when one is set; otherwise all, until scoping lands in ticket 08), `--all` includes resolved, `--since <id>` returns only newer Comments, and every read returns `highest_comment_id`. Each Thread reports `revision`, `latest_revision`, a derived `stale` flag, the quoted text and the locator. A Comment on an older Revision is never refused. Comments are never edited or deleted, and anchors are never rewritten.

**Blocked by:** 05: Markdown Sections

**Status:** ready-for-agent

- [ ] Add with line range stores the quoted text; with section stores the resolved path
- [ ] Reply attaches to the Thread's Revision and rejects a locator
- [ ] Resolve and reopen work for either role
- [ ] Default list includes open stale Threads; `--all` and `--since` behave as documented
- [ ] `highest_comment_id` is present on every read and is monotonic across the database
- [ ] A Comment on a Revision that is not the latest is accepted and reported `stale`
- [ ] The open Thread count in `list` (ticket 04) is now populated
