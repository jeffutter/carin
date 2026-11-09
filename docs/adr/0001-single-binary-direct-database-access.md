# One binary; the CLI manipulates the database directly

Cairn ships as one binary. A subcommand starts the axum server; the other subcommands call a shared core library that reads and writes the sqlite database directly, so submitting or editing an Artifact needs no running server. The server is a second frontend over the same core, serving the web UI and the JSON API.

We chose this over a CLI that is a client of the server because agents get zero setup and nothing to keep running. It means two processes write the same database (sqlite WAL mode), and it rules out agents on other hosts, which are out of scope. Moving to a server-only model later would change every CLI command's internals.
