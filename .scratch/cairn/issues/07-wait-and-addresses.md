# 07: Agent feedback loop: `--wait` and `--addresses`

**What to build:** The closed loop between Reviewer and Agent. `comment list --wait` blocks until something newer than `--since` exists, polling the database every 500 ms (a constant in core, no server needed), with `--timeout <secs>` (default 60); a timeout with nothing new exits 5. `edit --addresses 12,14` posts a reply "addressed in abc123@4" in each named Thread as the editing Revision's author and role, and never resolves the Thread.

**Blocked by:** 06: Comments and Threads (CLI)

**Status:** ready-for-agent

- [ ] `--wait` returns promptly when a Comment arrives from another process during the wait
- [ ] Timeout exits 5 with an empty result and the same `highest_comment_id`
- [ ] `--addresses` posts one reply per named Thread, attributed to the Revision's author and role
- [ ] The Thread stays open after `--addresses`
- [ ] An unknown Comment id in `--addresses` exits 3 and creates no Revision
