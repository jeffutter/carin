# Cairn

Cairn gives your coding agent a way to ask for your review. The agent submits a plan, a design doc, a diagram, an image, or an HTML page. You read it in a browser and comment on specific sections or lines. The agent reads your comments, edits the document, and answers each thread.

Everything runs on your machine. Cairn is one binary with one sqlite file. It has no accounts and no cloud service.

## Install

Download the tarball for your platform (Linux x86_64 or macOS aarch64) from the [releases page](https://github.com/jeffutter/cairn/releases) and put `cairn` on your `PATH`.

With Nix:

```sh
nix build github:jeffutter/cairn     # the binary
nix run github:jeffutter/cairn -- serve
```

Build from source inside `nix develop`. A plain `cargo build` compiles, but it leaves out the Mermaid renderer.

## Set up your agent

Agents learn cairn from a skill file. Cairn generates it from its own command definitions, so it matches the version you installed:

```sh
mkdir -p ~/.claude/skills/cairn
cairn skill > ~/.claude/skills/cairn/SKILL.md
```

Any agent that reads Markdown instructions can use the same file. To check whether an installed copy is out of date after an upgrade, run `cairn skill --check ~/.claude/skills/cairn/SKILL.md`. Exit code 1 means regenerate it.

Nix users can link the `skill` package from this flake into the skills directory instead.

Inside Claude Code and pi, cairn prints JSON on its own. For another harness, export `CAIRN_FORMAT=json`.

## Review a document

1. Start the review UI in a terminal and leave it running:

   ```sh
   cairn serve
   ```

   The UI is at <http://127.0.0.1:7878>.

2. Ask your agent to get a review. For example: "Write the migration plan and submit it to cairn for my review."

3. The agent submits the document and tells you its id. Open the page directly with `cairn view <id>`, or pick it from the list at the UI's home page.

4. Read, then comment. In a markdown document, click a line number to comment on a line range, or "+ section" beside a heading to comment on that section. For images, diagrams, and HTML pages, comments attach to the whole revision.

5. The agent is waiting for comments. When yours arrive, it edits the document, which creates a new revision, and replies in each thread with the revision that addresses it. Replies never resolve a thread. You do that when you are satisfied. The page refreshes itself every two seconds.

6. Use the revision dropdown at the top of the page to compare against earlier versions. An open thread from an older revision stays visible with an "earlier revision" label.

## What you can submit

| Kind | Source | Notes |
| --- | --- | --- |
| markdown | `.md` | Section-level comments. Images are embedded by pointing at other artifacts with `cairn:<id>`. |
| html | one `.html` file or a directory with `index.html` | Runs in a sandboxed frame and must be self-contained. |
| image | png, jpeg, gif, webp | |
| diagram | `.mmd` (Mermaid) or `.svg` | Cairn renders Mermaid to SVG itself. |

Limits: 10 MiB per file, 25 MiB per revision, 200 files per HTML bundle.

## Use the CLI yourself

The agent's commands work for you too. Run `cairn --help` for the full list.

```sh
cairn list                           # artifacts in the current project
cairn list --open                    # only those with open threads
cairn get <id>                       # print the latest revision
cairn comment add <id> "Looks good"  # comment from the terminal
cairn comment list --all             # include resolved threads
cairn delete <id> --yes
```

Output is text on a terminal and JSON in a pipe or an agent session. Force one with `--text` or `--json`.

Cairn scopes artifacts to the git project you run it in. Add `--all-projects` to see everything.

## Where data lives

The database is `$XDG_DATA_HOME/cairn/cairn.db`, usually `~/.local/share/cairn/cairn.db`. Change it with `--db` or `CAIRN_DB`. Deleting an artifact shrinks the file. Cairn never expires or prunes anything on its own.

## Security

The UI and artifact content are served from different ports, so a hostile HTML page cannot act as the review UI. HTML runs in a sandboxed frame with no network access.

`cairn serve` binds to loopback. `--bind 0.0.0.0` makes it reachable from other machines, but cairn has no authentication. Anyone who can reach the port can read and comment. Set `CAIRN_URL` to the address your browser uses so `cairn view` prints a working link.

## More documentation

The design is written down in [`docs/`](docs/README.md): the [CLI reference](docs/cli.md), the [JSON API](docs/api.md) (OpenAPI schema in [`docs/openapi.json`](docs/openapi.json)), the [data model](docs/data-model.md), and the [architecture decisions](docs/adr/). Terms such as artifact, revision, and thread are defined in the [glossary](GLOSSARY.md). Contributors should read [`CLAUDE.md`](CLAUDE.md) for the build and test commands.
