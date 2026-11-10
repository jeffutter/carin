pub mod artifact;
pub mod cli;
pub mod config;
pub mod db;
pub mod error;
pub mod ids;
pub mod kind;
pub mod limits;

use std::io::{Read, Write};

use cli::{Command, Context};
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
        Command::Edit { id, path, base } => {
            if id.contains('@') {
                return Err(Error::Usage(
                    "edit takes a bare artifact id, not id@revision".into(),
                ));
            }
            let bytes = read_input(&path)?;
            let mut conn = db::open(&ctx.db_path)?;
            let r = artifact::edit(&mut conn, ctx, &id, bytes, base)?;
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
        Command::Get { target } => {
            let (id, rev) = artifact::parse_target(&target)?;
            let conn = db::open(&ctx.db_path)?;
            let bytes = artifact::get(&conn, &id, rev)?;
            let mut out = std::io::stdout().lock();
            out.write_all(&bytes)
                .and_then(|_| out.flush())
                .map_err(|e| Error::Unexpected(format!("write failed: {e}")))
        }
    }
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
