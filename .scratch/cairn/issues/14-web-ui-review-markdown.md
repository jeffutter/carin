# 14: Web UI: review a markdown Artifact

**What to build:** The Reviewer discusses a markdown Artifact in the browser. A line-number gutter and a per-heading "+ section" button start a new Thread; Threads render inline under the Section or line they anchor to; the Reviewer replies and resolves or reopens Threads. Posts are attributed to `--reviewer-name` with role reviewer, calling core directly. An open Thread made on an earlier Revision still shows on other Revisions with a dashed border and an "earlier revision" label; resolved Threads from other Revisions are hidden. The page refreshes by an htmx poll every 2 seconds, paused while the tab is hidden, using the same latest-comment check as `--wait`. Optionally add the per-Artifact "last seen" marker if it stays small; it is a hint, not a contract.

**Blocked by:** 13: Web UI: list and read-only Artifact page, 06: Comments and Threads (CLI)

**Status:** ready-for-agent

- [ ] Posting, replying, resolving and reopening work and appear in `cairn comment list`
- [ ] Comments posted by the CLI appear in the open page within the poll interval
- [ ] Polling pauses when the tab is hidden
- [ ] Earlier-Revision open Threads show dashed and labelled; resolved ones are hidden
- [ ] Posted attribution uses `--reviewer-name` and role reviewer
- [ ] Open-Thread counts in the outline sidebar and list update
