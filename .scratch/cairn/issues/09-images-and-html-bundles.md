# 09: Image Artifacts and html Bundles

**What to build:** Agents can submit images (png, jpeg, gif, webp; never svg) and html Artifacts, which are Bundles: a required entry page plus assets at relative paths, from a single `.html` file or a directory. Kind comes from `--kind` or the extension, never content sniffing; ambiguous input is an error. Media types are checked against an allowlist and stored per file. Bundle limits: 200 files and the per-Revision cap. `get` for these Kinds needs `--out <dir>` and prints a JSON file list. Editing a Bundle resubmits the whole Bundle; unchanged files share blobs.

**Blocked by:** 03: Edit and Revisions

**Status:** ready-for-agent

- [ ] A directory without an entry page is rejected with exit 4
- [ ] Over 200 files or over the Revision cap is rejected, naming the limit
- [ ] `.svg` without `--kind diagram --format svg` is an error, not an image
- [ ] Media types outside the allowlist are rejected
- [ ] `get --out` writes files and prints the list; without `--out` it exits 2
- [ ] Editing a Bundle with one changed file stores one new blob
