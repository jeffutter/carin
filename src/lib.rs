pub mod api;
pub mod artifact;
pub mod bundle;
pub mod cli;
pub mod comments;
pub mod config;
pub mod db;
pub mod diagram;
pub mod embeds;
pub mod error;
pub mod ids;
pub mod kind;
pub mod limits;
pub mod output;
pub mod scope;
pub mod sections;
pub mod serve;
pub mod skill;
pub mod ui;

use std::io::{Read, Write};

use cli::{Command, CommentAction, Context};
use error::Error;
use output::{Deleted, FileEntry, Files, Format, Listing, emit};

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

/// The UI URL of `target`. The base is `--url` / `CAIRN_URL`, else the default
/// loopback UI port (`CAIRN_PORT` or 7878). No server is contacted.
fn view_url(target: &str, url: Option<&str>) -> Result<String, Error> {
    let (id, rev) = artifact::parse_target(target)?;
    if !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(Error::Usage(format!("bad artifact id in {target:?}")));
    }
    let base = match url.map(str::trim).filter(|u| !u.is_empty()) {
        Some(u) => u.trim_end_matches('/').to_string(),
        None => {
            let port = std::env::var("CAIRN_PORT")
                .ok()
                .and_then(|p| p.trim().parse::<u16>().ok())
                .filter(|p| *p >= 1)
                .unwrap_or(7878);
            format!("http://127.0.0.1:{port}")
        }
    };
    Ok(match rev {
        Some(n) => format!("{base}/a/{id}@{n}"),
        None => format!("{base}/a/{id}"),
    })
}

/// Run a command; the Ok value is the exit code (0, or 5 for a wait timeout).
pub fn run(command: Command, ctx: &Context, fmt: Format) -> Result<u8, Error> {
    if let Command::Comment { action } = command {
        return run_comment(action, ctx, fmt);
    }
    if let Command::Serve {
        port,
        content_port,
        bind,
        reviewer_name,
    } = command
    {
        let (ui, content) = serve::resolve_ports(port, content_port)?;
        return serve::serve(&ctx.db_path, bind, ui, content, &reviewer_name, fmt);
    }
    if let Command::Skill { check } = command {
        return skill::run(check.as_deref());
    }
    if let Command::View { target, url } = command {
        println!("{}", view_url(&target, url.as_deref())?);
        return Ok(0);
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
                // `x.svg --format svg` is ambiguous to infer(); --format
                // resolves it to a diagram. Any other inferable extension wins.
                (None, Some(_), false) => kind::infer(&path, None).unwrap_or(kind::Kind::Diagram),
                _ => kind::infer(&path, kind)?,
            };
            if format.is_some() && kind != kind::Kind::Diagram {
                return Err(Error::Rejected(format!(
                    "--format applies only to diagram Artifacts; {path} is a {kind:?} Artifact"
                )));
            }
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
            emit(fmt, &r);
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
            emit(fmt, &r);
            Ok(0)
        }
        Command::List {
            kind,
            mine,
            open,
            older_than,
            limit,
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
                id: None,
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
            emit(fmt, &Listing { artifacts: &rows });
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
            emit(fmt, &Deleted { deleted });
            Ok(0)
        }
        Command::Comment { .. }
        | Command::Serve { .. }
        | Command::Skill { .. }
        | Command::View { .. } => {
            unreachable!("handled above")
        }
        Command::Outline { target } => {
            let (id, rev) = artifact::parse_target(&target)?;
            let conn = db::open(&ctx.db_path)?;
            let text = artifact::markdown_text(&conn, &id, rev)?;
            emit(fmt, &sections::outline(&text));
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
                emit(fmt, &Files { files: listing });
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
/// Existing regular files are replaced atomically via temp file and rename
/// (re-running `get` into the same directory refreshes it; a failed write
/// leaves the old file intact). Symlinks beneath `dir` are never followed, and
/// file/directory clashes are refused before anything is written. `dir`
/// itself may be a symlink, since the caller chose it.
fn write_out(
    dir: &std::path::Path,
    files: &[artifact::StoredFile],
) -> Result<Vec<FileEntry>, Error> {
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

    // Phase 2. Directories are created one component at a time with
    // create_dir (not create_dir_all), refusing a symlink raced in after
    // phase 1. Each file is written to a temp file beside its destination
    // (create_new / O_EXCL) and renamed into place: rename replaces the
    // destination entry without following a symlink there, and a failed
    // write leaves any previous file intact.
    std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
    let mut listing = Vec::with_capacity(files.len());
    for f in files {
        let dest = dir.join(&f.path);
        let mut cur = dir.to_path_buf();
        let comps: Vec<&str> = f.path.split('/').collect();
        for comp in &comps[..comps.len() - 1] {
            cur.push(comp);
            match std::fs::create_dir(&cur) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let m = std::fs::symlink_metadata(&cur).map_err(|e| io(&cur, e))?;
                    if m.file_type().is_symlink() || !m.is_dir() {
                        return Err(refuse(format!(
                            "{} is not a plain directory",
                            cur.display()
                        )));
                    }
                }
                Err(e) => return Err(io(&cur, e)),
            }
        }
        let mut rnd = [0u8; 8];
        getrandom::fill(&mut rnd)
            .map_err(|e| Error::Unexpected(format!("cannot generate temp name: {e}")))?;
        let suffix: String = rnd.iter().map(|b| format!("{b:02x}")).collect();
        let base = comps[comps.len() - 1];
        let tmp = dest.with_file_name(format!(".{base}.cairn-tmp-{suffix}"));
        let written = (|| {
            let mut h = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
            h.write_all(&f.bytes)?;
            drop(h);
            std::fs::rename(&tmp, &dest)
        })();
        if let Err(e) = written {
            let _ = std::fs::remove_file(&tmp);
            return Err(io(&dest, e));
        }
        listing.push(FileEntry {
            path: f.path.clone(),
            media_type: f.media_type.clone(),
            size: f.size,
        });
    }
    Ok(listing)
}

fn run_comment(action: CommentAction, ctx: &Context, fmt: Format) -> Result<u8, Error> {
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
            emit(fmt, &r);
        }
        CommentAction::Resolve { thread_id } | CommentAction::Reopen { thread_id } => {
            let status = if matches!(action_kind, "resolve") {
                "resolved"
            } else {
                "open"
            };
            let mut conn = db::open(&ctx.db_path)?;
            let r = comments::set_status(&mut conn, thread_id, status)?;
            emit(fmt, &r);
        }
        CommentAction::List {
            target,
            all,
            since,
            wait,
            timeout,
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
            emit(fmt, &r);
            return Ok(exit);
        }
    }
    Ok(0)
}
