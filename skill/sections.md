A Section is a markdown heading and everything under it, children included. Its path joins the heading texts with ` > `, for example `Design > Storage`. A path may be a suffix of the full path if that is unique. When it is not, add `#n` to pick the nth match (`Notes#2`). Inside a heading, `\`, `>` and a `#` before trailing digits are backslash-escaped (`Issue \#12`, `A \> B`). List the exact paths with:

```sh
cairn outline abc123
```

Use a path to read, edit or comment on one Section:

```sh
cairn get abc123 --section "Design > Storage"
cairn edit abc123 storage.md --section "Design > Storage"
cairn comment add abc123 "Why SQLite?" --section "Design > Storage"
```

A replacement file must start with a heading of the same level and must not contain a shallower heading. An ambiguous or missing path exits 4.
