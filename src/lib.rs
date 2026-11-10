pub mod artifact;
pub mod bundle;
pub mod cli;
pub mod comments;
pub mod config;
pub mod db;
pub mod diagram;
pub mod error;
pub mod ids;
pub mod kind;
pub mod limits;
pub mod scope;
pub mod sections;

use std::io::{Read, Write};

use cli::{Command, CommentAction, Context};
use error::Error;

/// Read a submitted path: a file, a directory (a Bundle) or `-` for stdin.
/// A single file is returned under its own file name, or `-` for stdin.
fn read_input(path: &str) -> Result<bundle::Input, Error> {
    let io = |e: std::io::Error| Error::Usage(format!("cannot read {path}: {e}"));
    if path == "-" {
        let mut buf = Vec::new();
        std::io::stdin()
            .take(limits::MAX_FILE_BYTES + 1)
            .read_to_end(&mut buf)
            .map_err(io)?;
        limits::check_file(buf.len() as u64)?;
        return Ok(bundle::Input {
            files: vec![bundle::RawFile {
                path: "-".into(),
                bytes: buf,
            }],
            dir: false,
        });
    }
    let meta = std::fs::metadata(path).map_err(io)?;
    if meta.is_dir() {
        return bundle::read_dir(std::path::Path::new(path));
    }
    limits::check_file(meta.len())?;
    let file_name = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::Usage(format!("no file name in {path:?}")))?;
    Ok(bundle::Input {
        files: vec![bundle::RawFile {
            path: file_name.to_string(),
            bytes: std::fs::read(path).map_err(io)?,
        }],
        dir: false,
    })
}

fn read_body(body: &str) -> Result<String, Error> {
    if body != "-" {
        return Ok(body.to_string());
    }
    let mut buf = Vec::new();
    std::io::stdin()
        .take(limits::MAX_COMMENT_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| Error::Usage(format!("cannot read stdin: {e}")))?;
    String::from_utf8(buf).map_err(|_| Error::Usage("comment body is not valid UTF-8".into()))
}

fn selection(ctx: &Context) -> scope::Selection {
    scope::select(ctx.project.as_deref(), &cwd())
}

/// An unreadable cwd just means nothing to detect.
fn cwd() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_default()
}

/// Run a command; the Ok value is the exit code (0, or 5 for a wait timeout).
pub fn run(command: Command, ctx: &Context) -> Result<u8, Error> {
    if let Command::Comment { action } = command {
        return run_comment(action, ctx);
    }
    match command {
        Command::Submit {
            path,
            kind,
            name,
            format,
        } => {
            let stdin = path == "-";
            if stdin && (kind.is_none() || name.is_none()) {
                return Err(Error::Usage(
                    "submitting from stdin requires --kind and --name".into(),
                ));
            }
            let is_dir = !stdin && std::path::Path::new(&path).is_dir();
            // A directory can only be an html Bundle.
            let kind = match (kind, format, is_dir) {
                (None, _, true) => kind::Kind::Html,
                // --format only makes sense for a diagram.
                (None, Some(_), false) => kind::Kind::Diagram,
                _ => kind::infer(&path, kind)?,
            };
            let format = match (kind, format) {
                (kind::Kind::Diagram, None) => {
                    Some(diagram::Format::infer(&path).ok_or_else(|| {
                        Error::Rejected("a diagram needs --format mermaid or --format svg".into())
                    })?)
                }
                (_, f) => f,
            };
            let name = match name {
                Some(n) => n,
                // Prefer the path's own last component so a missing file
                // reaches read_input's error; canonicalize only for '.'/'..'.
                None => {
                    let p = std::path::Path::new(&path);
                    p.file_name()
                        .map(std::path::PathBuf::from)
                        .or_else(|| {
                            std::fs::canonicalize(p)
                                .ok()
                                .and_then(|c| c.file_name().map(std::path::PathBuf::from))
                        })
                        .and_then(|n| n.to_str().map(str::to_string))
                        .ok_or_else(|| Error::Usage(format!("no file name in {path:?}")))?
                }
            };
            let mut input = read_input(&path)?;
            if !input.dir {
                input.files[0].path = name.clone();
            }
            let scope = scope::for_write(selection(ctx)).map_err(Error::Usage)?;
            let mut conn = db::open(&ctx.db_path)?;
            let r = artifact::submit(
                &mut conn,
                ctx,
                artifact::SubmitInput {
                    name,
                    kind,
                    format,
                    input,
                    scope,
                },
            )?;
            println!("{}", serde_json::to_string(&r).expect("ref serializes"));
            Ok(0)
        }
        Command::Edit {
            id,
            path,
            base,
            section,
            addresses,
        } => {
            if id.contains('@') {
                return Err(Error::Usage(
                    "edit takes a bare artifact id, not id@revision".into(),
                ));
            }
            let mut input = read_input(&path)?;
            let mut conn = db::open(&ctx.db_path)?;
            let r = match section {
                Some(q) => {
                    if input.dir {
                        return Err(Error::Usage(
                            "--section takes a file, not a directory".into(),
                        ));
                    }
                    let bytes = input.files.remove(0).bytes;
                    artifact::section_edit(&mut conn, ctx, &id, &q, &bytes, base, &addresses)?
                }
                None => artifact::edit(&mut conn, ctx, &id, input, base, &addresses)?,
            };
            println!("{}", serde_json::to_string(&r).expect("ref serializes"));
            Ok(0)
        }
        Command::List {
            kind,
            mine,
            open,
            older_than,
            limit,
            text,
            all_projects,
            all_worktrees,
            project_root,
        } => {
            let older_than_secs = older_than
                .as_deref()
                .map(artifact::parse_duration)
                .transpose()?;
            let (session, author) = match (&ctx.session_explicit, mine) {
                (Some(s), true) => (Some(s.clone()), None),
                (None, true) => match &ctx.session {
                    Some(s) => (Some(s.clone()), None),
                    None => (None, Some(ctx.author.clone())),
                },
                (explicit, false) => (explicit.clone(), None),
            };
            let filter = artifact::ListFilter {
                kind,
                session,
                author,
                open,
                older_than_secs,
                limit,
                scope: scope::ScopeFilter::build(
                    &selection(ctx),
                    all_projects,
                    all_worktrees,
                    project_root.as_deref(),
                    &cwd(),
                ),
            };
            let conn = db::open(&ctx.db_path)?;
            let rows = artifact::list(&conn, &filter)?;
            if text {
                print!("{}", list_table(&rows));
            } else {
                println!("{}", serde_json::json!({ "artifacts": rows }));
            }
            Ok(0)
        }
        Command::Delete { ids, yes } => {
            if !yes {
                return Err(Error::Usage(
                    "delete is permanent: pass --yes to confirm".into(),
                ));
            }
            if let Some(id) = ids.iter().find(|i| i.contains('@')) {
                return Err(Error::Usage(format!(
                    "delete takes bare artifact ids, not {id:?}"
                )));
            }
            let mut conn = db::open(&ctx.db_path)?;
            let deleted = artifact::delete(&mut conn, &ids)?;
            println!("{}", serde_json::json!({ "deleted": deleted }));
            Ok(0)
        }
        Command::Comment { .. } => unreachable!("handled above"),
        Command::Outline { target } => {
            let (id, rev) = artifact::parse_target(&target)?;
            let conn = db::open(&ctx.db_path)?;
            let text = artifact::markdown_text(&conn, &id, rev)?;
            println!(
                "{}",
                serde_json::json!({ "sections": sections::outline(&text) })
            );
            Ok(0)
        }
        Command::Get {
            target,
            section,
            out,
        } => {
            let (id, rev) = artifact::parse_target(&target)?;
            let conn = db::open(&ctx.db_path)?;
            let (kind, files) = artifact::get_files(&conn, &id, rev)?;
            if matches!(kind, kind::Kind::Html | kind::Kind::Image) {
                if section.is_some() {
                    return Err(Error::Usage("--section is for markdown Artifacts".into()));
                }
                let out = out.ok_or_else(|| {
                    Error::Usage(format!(
                        "{} Artifacts are files: pass --out <dir>",
                        kind.as_str()
                    ))
                })?;
                let listing = write_out(std::path::Path::new(&out), &files)?;
                println!("{}", serde_json::json!({ "files": listing }));
                return Ok(0);
            }
            if out.is_some() {
                return Err(Error::Usage(format!(
                    "--out is for image and html Artifacts, not {}",
                    kind.as_str()
                )));
            }
            let bytes = match section {
                Some(q) => {
                    let text = artifact::markdown_text(&conn, &id, rev)?;
                    sections::extract(&text, &q)?.as_bytes().to_vec()
                }
                None => artifact::get(&conn, &id, rev)?,
            };
            let mut out = std::io::stdout().lock();
            out.write_all(&bytes)
                .and_then(|_| out.flush())
                .map_err(|e| Error::Unexpected(format!("write failed: {e}")))
                .map(|_| 0)
        }
    }
}

/// Write a Revision's files under `dir` and return the file list.
///
/// Two phases: validate everything without touching the disk, then write.
/// Existing regular files are overwritten (re-running `get` into the same
/// directory refreshes it). Symlinks beneath `dir` are never followed, and
/// file/directory clashes are refused before anything is written. `dir`
/// itself may be a symlink, since the caller chose it.
fn write_out(
    dir: &std::path::Path,
    files: &[artifact::StoredFile],
) -> Result<Vec<serde_json::Value>, Error> {
    use std::collections::BTreeSet;
    let io = |p: &std::path::Path, e: std::io::Error| {
        Error::Unexpected(format!("cannot write {}: {e}", p.display()))
    };
    let refuse = |msg: String| Error::Unexpected(format!("refusing to write: {msg}"));

    // Phase 1a: path safety and file-set clashes (a.js vs a.js/b.css).
    let mut file_paths = BTreeSet::new();
    let mut dir_paths = BTreeSet::new();
    for f in files {
        if !bundle::safe_path(&f.path) {
            return Err(Error::Unexpected(format!(
                "stored path {:?} is unsafe",
                f.path
            )));
        }
        file_paths.insert(f.path.as_str());
        let mut end = 0;
        while let Some(i) = f.path[end..].find('/') {
            end += i;
            dir_paths.insert(&f.path[..end]);
            end += 1;
        }
    }
    if let Some(clash) = file_paths.intersection(&dir_paths).next() {
        return Err(refuse(format!(
            "{clash:?} is both a file and a directory in this Revision"
        )));
    }

    // Phase 1b: walk each destination below `dir` for symlinks and clashes
    // with what already exists on disk.
    for f in files {
        let mut cur = dir.to_path_buf();
        let n = f.path.split('/').count();
        for (i, comp) in f.path.split('/').enumerate() {
            cur.push(comp);
            let meta = match std::fs::symlink_metadata(&cur) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                Err(e) => return Err(io(&cur, e)),
            };
            let last = i + 1 == n;
            if meta.file_type().is_symlink() {
                return Err(refuse(format!("{} is a symlink", cur.display())));
            }
            if last && meta.is_dir() {
                return Err(refuse(format!(
                    "{} is an existing directory",
                    cur.display()
                )));
            }
            if !last && !meta.is_dir() {
                return Err(refuse(format!("{} is not a directory", cur.display())));
            }
        }
    }

    // Phase 2. An existing regular file is removed and recreated with
    // create_new (O_EXCL), which fails rather than follows a symlink raced
    // in after phase 1.
    let mut listing = Vec::with_capacity(files.len());
    for f in files {
        let dest = dir.join(&f.path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
        }
        match std::fs::remove_file(&dest) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io(&dest, e)),
        }
        let mut h = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
            .map_err(|e| io(&dest, e))?;
        h.write_all(&f.bytes).map_err(|e| io(&dest, e))?;
        listing.push(serde_json::json!({
            "path": f.path, "media_type": f.media_type, "size": f.size
        }));
    }
    Ok(listing)
}

fn run_comment(action: CommentAction, ctx: &Context) -> Result<u8, Error> {
    let print = |v: serde_json::Value| println!("{v}");
    let action_kind = match &action {
        CommentAction::Resolve { .. } => "resolve",
        _ => "",
    };
    match action {
        CommentAction::Add {
            target,
            body,
            lines,
            section,
            reply,
        } => {
            let body = read_body(&body)?;
            let mut conn = db::open(&ctx.db_path)?;
            let locator = comments::Locator { lines, section };
            let r = comments::add(&mut conn, ctx, &target, &body, &locator, reply)?;
            print(serde_json::to_value(r).expect("added serializes"));
        }
        CommentAction::Resolve { thread_id } | CommentAction::Reopen { thread_id } => {
            let status = if matches!(action_kind, "resolve") {
                "resolved"
            } else {
                "open"
            };
            let mut conn = db::open(&ctx.db_path)?;
            let r = comments::set_status(&mut conn, thread_id, status)?;
            print(serde_json::to_value(r).expect("thread serializes"));
        }
        CommentAction::List {
            target,
            all,
            since,
            wait,
            timeout,
            text,
            all_projects,
            all_worktrees,
            project_root,
        } => {
            if let Some(t) = &target
                && t.contains('@')
            {
                return Err(Error::Usage(
                    "comment list takes a bare artifact id, not id@revision".into(),
                ));
            }
            if (target.is_some() || ctx.session_explicit.is_some())
                && (all_projects || all_worktrees || project_root.is_some())
            {
                return Err(Error::Usage(
                    "--all-projects/--all-worktrees/--project-root cannot be combined with an \
                     artifact id or --session, which ignore Project scope"
                        .into(),
                ));
            }
            let conn = db::open(&ctx.db_path)?;
            let q = comments::ListQuery {
                artifact: target,
                all,
                since,
                scope: scope::ScopeFilter::build(
                    &selection(ctx),
                    all_projects,
                    all_worktrees,
                    project_root.as_deref(),
                    &cwd(),
                ),
            };
            let (r, exit) = if wait {
                match comments::wait(&conn, ctx, &q, std::time::Duration::from_secs(timeout))? {
                    comments::WaitOutcome::Ready(r) => (r, 0),
                    comments::WaitOutcome::TimedOut(r) => (r, error::EXIT_WAIT_TIMEOUT),
                }
            } else {
                (comments::list(&conn, ctx, &q)?, 0)
            };
            if text {
                print!("{}", comment_table(&r));
            } else {
                print(serde_json::to_value(&r).expect("list serializes"));
            }
            return Ok(exit);
        }
    }
    Ok(0)
}

fn list_table(rows: &[artifact::ListRow]) -> String {
    let header = [
        "ID", "KIND", "NAME", "REV", "OPEN", "AUTHOR", "BYTES", "UPDATED", "PROJECT", "WORKTREE",
    ];
    let mut table: Vec<Vec<String>> = vec![header.iter().map(|s| s.to_string()).collect()];
    for r in rows {
        table.push(vec![
            r.id.clone(),
            r.kind.clone(),
            r.name.clone(),
            r.latest_revision.to_string(),
            r.open_threads.to_string(),
            r.author.clone(),
            r.bytes.to_string(),
            r.updated_at.clone(),
            r.project.clone().unwrap_or_else(|| "-".into()),
            r.worktree.clone().unwrap_or_else(|| "-".into()),
        ]);
    }
    let widths: Vec<usize> = (0..header.len())
        .map(|c| {
            table
                .iter()
                .map(|row| row[c].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut out = String::new();
    for row in &table {
        let line: Vec<String> = row
            .iter()
            .zip(&widths)
            .map(|(cell, w)| format!("{cell:<w$}"))
            .collect();
        out.push_str(line.join("  ").trim_end());
        out.push('\n');
    }
    out
}

fn comment_table(r: &comments::CommentList) -> String {
    let mut out = format!("highest_comment_id {}\n", r.highest_comment_id);
    for t in &r.threads {
        let loc = match (
            &t.locator.section_path,
            t.locator.line_start,
            t.locator.line_end,
        ) {
            (Some(p), _, _) => p.clone(),
            (None, Some(a), Some(b)) if a == b => format!("L{a}"),
            (None, Some(a), Some(b)) => format!("L{a}-{b}"),
            _ => "-".into(),
        };
        out.push_str(&format!(
            "#{} {}@{}{} {} {} {} [{}]: {}\n",
            t.id,
            t.artifact_id,
            t.revision,
            if t.stale { " (stale)" } else { "" },
            t.status,
            t.author,
            loc,
            t.role,
            t.body.lines().next().unwrap_or("")
        ));
        for rep in &t.replies {
            out.push_str(&format!(
                "    #{} {} [{}]: {}\n",
                rep.id,
                rep.author,
                rep.role,
                rep.body.lines().next().unwrap_or("")
            ));
        }
    }
    out
}
