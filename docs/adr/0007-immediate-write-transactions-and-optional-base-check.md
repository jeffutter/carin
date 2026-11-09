# Immediate write transactions and an optional base-Revision check

Every write runs in one sqlite transaction opened with `BEGIN IMMEDIATE`, on connections set to WAL, `foreign_keys=on`, `synchronous=NORMAL` and a 5 second `busy_timeout`. The next Revision number and the check that the Artifact still exists happen inside that transaction. The shared core opens every connection, so the CLI and the server cannot differ. Reads use plain transactions and never block on writers.

`edit` takes an optional `--base <rev>` (API: `base_revision`). If the latest Revision is not `<rev>`, the edit is refused with exit code 6 (HTTP 409) and the error names the latest Revision and its author. Without `--base`, the edit applies to the latest. A `--section` edit without `--base` also applies to the latest as long as the Section resolves.

We chose this over leaving edits as last-writer-wins, which silently drops one Agent's work from the latest Revision when two edit the same Artifact, and over a required base, which burdens the common single-Agent case. The cost is one more exit code and a skill instruction to pass `--base`. Two processes writing one database means a write can wait up to the busy timeout; past it the command exits 1 as database busy.
