pub mod artifact;
pub mod cli;
pub mod comments;
pub mod config;
pub mod db;
pub mod error;
pub mod ids;
pub mod kind;
pub mod limits;
pub mod sections;

use std::io::{Read, Write};

use cli::{Command, CommentAction, Context};
use error::Error;

fn read_input(path: &str) -> Result<Vec<u8>, Error> {
    let io = |e: std::io::Error| Error::Usage(format!("cannot read {path}: {e}"));
    if path == "-" {
        let mut buf = Vec::new();
        std::io::stdin()
            .take(limits::MAX_FILE_BYTES + 1)
            .read_to_end(&mut buf)
            .map_err(io)?;
        return Ok(buf);
    }
    let meta = std::fs::metadata(path).map_err(io)?;
    limits::check_file(meta.len())?;
    std::fs::read(path).map_err(io)
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

pub fn run(command: Command, ctx: &Context) -> Result<(), Error> {
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
            if format.is_some() {
                return Err(Error::Rejected("--format is not supported yet".into()));
            }
            let kind = kind::infer(&path, kind)?;
            let name = match name {
                Some(n) => n,
                None => std::path::Path::new(&path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| Error::Usage(format!("no file name in {path:?}")))?
                    .to_string(),
            };
            let bytes = read_input(&path)?;
            limits::check_file(bytes.len() as u64)?;
            let mut conn = db::open(&ctx.db_path)?;
            let r = artifact::submit(&mut conn, ctx, artifact::SubmitInput { name, kind, bytes })?;
            println!("{}", serde_json::to_string(&r).expect("ref serializes"));
            Ok(())
        }
        Command::Edit {
            id,
            path,
            base,
            section,
        } => {
            if id.contains('@') {
                return Err(Error::Usage(
                    "edit takes a bare artifact id, not id@revision".into(),
                ));
            }
            let bytes = read_input(&path)?;
            let mut conn = db::open(&ctx.db_path)?;
            let r = match section {
                Some(q) => artifact::section_edit(&mut conn, ctx, &id, &q, &bytes, base)?,
                None => artifact::edit(&mut conn, ctx, &id, bytes, base)?,
            };
            println!("{}", serde_json::to_string(&r).expect("ref serializes"));
            Ok(())
        }
        Command::List {
            kind,
            mine,
            open,
            older_than,
            limit,
            text,
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
            };
            let conn = db::open(&ctx.db_path)?;
            let rows = artifact::list(&conn, &filter)?;
            if text {
                print!("{}", list_table(&rows));
            } else {
                println!("{}", serde_json::json!({ "artifacts": rows }));
            }
            Ok(())
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
            Ok(())
        }
        Command::Comment { action } => run_comment(action, ctx),
        Command::Outline { target } => {
            let (id, rev) = artifact::parse_target(&target)?;
            let conn = db::open(&ctx.db_path)?;
            let text = artifact::markdown_text(&conn, &id, rev)?;
            println!(
                "{}",
                serde_json::json!({ "sections": sections::outline(&text) })
            );
            Ok(())
        }
        Command::Get { target, section } => {
            let (id, rev) = artifact::parse_target(&target)?;
            let conn = db::open(&ctx.db_path)?;
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
        }
    }
}

fn run_comment(action: CommentAction, ctx: &Context) -> Result<(), Error> {
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
            text,
        } => {
            if let Some(t) = &target
                && t.contains('@')
            {
                return Err(Error::Usage(
                    "comment list takes a bare artifact id, not id@revision".into(),
                ));
            }
            let conn = db::open(&ctx.db_path)?;
            let q = comments::ListQuery {
                artifact: target,
                all,
                since,
            };
            let r = comments::list(&conn, ctx, &q)?;
            if text {
                print!("{}", comment_table(&r));
            } else {
                print(serde_json::to_value(&r).expect("list serializes"));
            }
        }
    }
    Ok(())
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
