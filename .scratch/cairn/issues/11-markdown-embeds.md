# 11: Markdown embeds (`cairn:` references)

**What to build:** A markdown Artifact can show an image or diagram Artifact by reference, latest or pinned to a Revision. At submit and edit, each reference is checked: a missing Artifact or Revision, or a target that is not an image or diagram, is rejected with exit 4. Each unpinned reference is resolved to the target's current Revision and recorded in the embeds side table; the stored markdown text is never rewritten. Other image URLs (http, data, relative) are not fetched and will show as alt text. Deleting an embedded Artifact is allowed.

**Blocked by:** 09: Image Artifacts and html Bundles, 10: Diagram Artifacts (svg and Mermaid)

**Status:** ready-for-agent

- [ ] Latest and pinned references both validate and are recorded with the resolved Revision
- [ ] Missing target, missing Revision, and a markdown or html target each exit 4 with a clear message
- [ ] Embeds are re-resolved on each new markdown Revision
- [ ] Deleting an embedded Artifact succeeds
- [ ] Core exposes the embeds of a Revision for later use by the UI
