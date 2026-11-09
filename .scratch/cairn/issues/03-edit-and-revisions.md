# 03: Edit and Revisions

**What to build:** An Agent edits an Artifact with `edit <id> <path|->`, creating a new immutable full-snapshot Revision and printing `abc123@4`. `get abc123@3` reads any earlier Revision. `--base <rev>` refuses a stale base with exit 6 naming the latest Revision and its author. The next Revision number and the Artifact-exists check happen inside the write transaction, so concurrent writers are safe; a write to a deleted Artifact exits 3. The per-Revision size limit is enforced here.

**Blocked by:** 02: Submit and get a markdown Artifact

**Status:** ready-for-agent

- [ ] `edit` creates Revision n+1 and leaves earlier Revisions unchanged
- [ ] `get <id>@<n>` returns that Revision; a missing Revision exits 3
- [ ] `--base` with a stale value exits 6 with `latest_revision` and its author in the error
- [ ] Unchanged content shares its blob with the previous Revision
- [ ] Two concurrent edits produce distinct, consecutive Revision numbers (test with two processes)
- [ ] A Revision over the per-Revision limit is rejected with exit 4
