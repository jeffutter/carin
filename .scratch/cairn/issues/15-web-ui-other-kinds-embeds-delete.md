# 15: Web UI: other Kinds, embeds and delete

**What to build:** The Artifact page works for every Kind. html shows in an iframe from the content port with the script-only sandbox token and never same-origin; images and diagrams show only via `<img>` from the content port. These pages use the same header and revision dropdown, and Comments are on the whole Revision with the same Thread UI. Markdown `cairn:` embeds render as `<img>` from the content origin, pinned or latest as recorded; a deleted target shows a missing-image placeholder; other URL schemes show alt text. The list gets its per-row delete action.

**Blocked by:** 13: Web UI: list and read-only Artifact page, 11: Markdown embeds (`cairn:` references), 14: Web UI: review a markdown Artifact

**Status:** ready-for-agent

- [ ] html, image and diagram Artifacts each render in the correct frame; tests assert the iframe sandbox attribute and the `<img>` usage
- [ ] A Bundle script cannot reach the UI origin (verified manually in a browser and noted in the PR)
- [ ] Embeds render pinned and latest; a deleted target shows the placeholder
- [ ] Whole-Revision Comments work on non-markdown Kinds
- [ ] Delete from the list removes the Artifact and refreshes the list
