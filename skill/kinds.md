Cairn infers the Kind from the file extension. When submitting from stdin, pass `--kind` and `--name`.

- `markdown`: prose, plans, specs, reports. Choose it by default. Only markdown has Sections.
- `image`: a picture file. Submit it alone, then embed it in markdown.
- `diagram`: Mermaid (`.mmd`) or SVG source. Pass `--format svg` for SVG. Cairn rejects invalid Mermaid with exit 4.
- `html`: a single `.html` file or a directory with an `index.html`. Use it for an interactive page or a rendered report.

```sh
cairn submit diagram.mmd
cairn submit shot.png
cairn submit - --kind markdown --name notes
```
