# 05: Markdown Sections

**What to build:** An Agent reads and edits a markdown Artifact by heading Section to save tokens. `outline <id>[@rev]` prints every heading with its path and size. `get --section "A > B"` prints one Section (including children), optionally from a given Revision. `edit --section "A > B"` replaces a Section (replacement must start with a heading line) by splicing into the latest Revision and storing a new full Revision. Heading paths match exactly (case-sensitive, trimmed, inline markup stripped) with unique-suffix matching; duplicates are addressed `Name#2`; an ambiguous or missing path exits 4 listing candidates; an edit never guesses. Parsing uses pulldown-cmark, top-level headings only; front matter and the text before the first heading is an unaddressable preamble. Settle the CRLF build-time point with a test first.

**Blocked by:** 03: Edit and Revisions

**Status:** ready-for-agent

- [ ] `outline` lists paths and sizes, with no ids
- [ ] Suffix matching, `#n` disambiguation and the ambiguity error listing candidates with occurrence numbers
- [ ] Headings inside blockquotes or list items are not boundaries; setext headings and front matter handled
- [ ] `edit --section` splices correctly, makes a new full Revision, and fails if the path no longer matches
- [ ] `--section` on a non-markdown Artifact exits 4
- [ ] CRLF behaviour decided and covered by a test
