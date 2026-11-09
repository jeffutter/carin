# 04: List and delete

**What to build:** An Agent or user can see and remove Artifacts. `list` shows newest activity first with id, Kind, name, latest Revision, open Thread count, author, stored bytes, last-updated time, and project and worktree columns (null until scoping lands). Filters: `--kind`, `--session`, `--mine`, `--open`, `--older-than <duration>`, `--limit` (default 50); `--text` gives a human table. `delete <id>... --yes` removes several Artifacts, each with all Revisions, Comments and now-unreferenced blobs, in one transaction; without `--yes` it exits 2.

**Blocked by:** 03: Edit and Revisions

**Status:** ready-for-agent

- [ ] `list` ordering, row shape and each filter behave as documented
- [ ] `--older-than` accepts a duration and filters on last-updated
- [ ] `delete` without `--yes` exits 2 and deletes nothing
- [ ] Deleting several ids is atomic; an unknown id exits 3
- [ ] Blobs still referenced by another Artifact survive; orphaned blobs are removed and the file shrinks
- [ ] A write to a deleted Artifact exits 3
