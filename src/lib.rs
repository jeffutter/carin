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
