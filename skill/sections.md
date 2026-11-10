A Section is a markdown heading plus everything under it, children included. Its path joins the heading texts with ` > `, for example `Design > Storage`. Use a suffix of the path when the suffix is unique. Otherwise add `#n` to pick the nth match (`Notes#2`). Escape `\`, `>`, and a `#` before trailing digits in heading text with a backslash (`Issue \#12`, `A \> B`). List the exact paths:

```sh
cairn outline abc123
```

Use a path to read, edit, or comment on one Section:

```sh
cairn get abc123 --section "Design > Storage"
cairn edit abc123 storage.md --section "Design > Storage"
cairn comment add abc123 "Why SQLite?" --section "Design > Storage"
```

Start a replacement file with a heading at the same level as the Section. Use headings at that level or deeper only. An ambiguous or missing path exits 4.
