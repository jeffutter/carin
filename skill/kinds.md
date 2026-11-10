The Kind is inferred from the file extension. Pass `--kind` (and `--name`) when submitting from stdin.

- `markdown`: prose, plans, specs, reports. The default choice, and the only Kind with Sections.
- `image`: a picture file. Submit it on its own and embed it in markdown.
- `diagram`: Mermaid (`.mmd`) or SVG source. SVG needs `--format svg`. Invalid Mermaid is rejected with exit 4.
- `html`: a single `.html` file or a directory with an `index.html`, for an interactive page or a rendered report.

```sh
cairn submit diagram.mmd
cairn submit shot.png
cairn submit - --kind markdown --name notes
```
