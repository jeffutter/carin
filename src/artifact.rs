//! Core Artifact operations. Callers hand in a connection from `db::open`.

use std::fmt;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::cli::Context;
use crate::db::{db_err, write_tx};
use crate::error::Error;
use crate::ids::new_artifact_id;
use crate::kind::Kind;
use crate::limits;

pub struct SubmitInput {
    pub name: String,
    pub kind: Kind,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    pub id: String,
    pub revision: i64,
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.id, self.revision)
    }
}

impl Serialize for Ref {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Ref", 3)?;
        st.serialize_field("id", &self.id)?;
        st.serialize_field("revision", &self.revision)?;
        st.serialize_field("ref", &self.to_string())?;
        st.end()
    }
}

/// A user-supplied `id` or `id@N`.
pub fn parse_target(s: &str) -> Result<(String, Option<i64>), Error> {
    let (id, rev) = match s.split_once('@') {
        Some((id, rev)) => {
            let n: i64 = rev
                .parse()
                .ok()
                .filter(|n| *n >= 1)
                .ok_or_else(|| Error::Usage(format!("bad revision in {s:?}")))?;
            (id, Some(n))
        }
        None => (s, None),
    };
    if id.is_empty() {
        return Err(Error::Usage(format!("empty artifact id in {s:?}")));
    }
    Ok((id.to_string(), rev))
}

pub fn submit(conn: &mut Connection, ctx: &Context, input: SubmitInput) -> Result<Ref, Error> {
    limits::check_file(input.bytes.len() as u64)?;
    let media_type = input.kind.media_type()?;
    let sha = hex(&Sha256::digest(&input.bytes));
    write_tx(conn, |tx| {
        let mut id = None;
        for _ in 0..5 {
            let candidate = new_artifact_id()?;
            let n = tx
                .execute(
                    "INSERT OR IGNORE INTO artifacts (id, kind, name, created_at, updated_at)
                     VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ','now'), strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
                    params![candidate, input.kind.as_str(), input.name],
                )
                .map_err(db_err)?;
            if n == 1 {
                id = Some(candidate);
                break;
            }
        }
        let id = id.ok_or_else(|| Error::Unexpected("could not allocate an artifact id".into()))?;
        tx.execute(
            "INSERT INTO revisions (artifact_id, number, author, role, session, created_at)
             VALUES (?1, 1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
            params![id, ctx.author, ctx.role.as_str(), ctx.session],
        )
        .map_err(db_err)?;
        tx.execute(
            "INSERT OR IGNORE INTO blobs (sha256, bytes) VALUES (?1, ?2)",
            params![sha, input.bytes],
        )
        .map_err(db_err)?;
        tx.execute(
            "INSERT INTO revision_files (artifact_id, revision, path, media_type, size, sha256)
             VALUES (?1, 1, ?2, ?3, ?4, ?5)",
            params![id, input.name, media_type, input.bytes.len() as i64, sha],
        )
        .map_err(db_err)?;
        Ok(Ref { id, revision: 1 })
    })
}

/// Append a new full-snapshot Revision. The latest number and the existence
/// check are read inside the write transaction, so concurrent writers get
/// distinct, consecutive numbers.
pub fn edit(
    conn: &mut Connection,
    ctx: &Context,
    id: &str,
    bytes: Vec<u8>,
    base: Option<i64>,
) -> Result<Ref, Error> {
    limits::check_file(bytes.len() as u64)?;
    limits::check_revision(bytes.len() as u64)?;
    let sha = hex(&Sha256::digest(&bytes));
    write_tx(conn, |tx| {
        let exists = tx
            .query_row("SELECT 1 FROM artifacts WHERE id = ?1", [id], |_| Ok(()))
            .optional()
            .map_err(db_err)?
            .is_some();
        if !exists {
            return Err(Error::NotFound(format!("no artifact {id}")));
        }
        let (latest, author): (i64, String) = tx
            .query_row(
                "SELECT number, author FROM revisions WHERE artifact_id = ?1
                 ORDER BY number DESC LIMIT 1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(db_err)?;
        if let Some(b) = base
            && b != latest
        {
            return Err(Error::Conflict(format!(
                "stale base: you edited from @{b} but latest_revision is {latest}, written by {author}"
            )));
        }
        let (path, media_type): (String, String) = tx
            .query_row(
                "SELECT path, media_type FROM revision_files
                 WHERE artifact_id = ?1 AND revision = ?2",
                params![id, latest],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(db_err)?;
        let n = latest + 1;
        tx.execute(
            "INSERT INTO revisions (artifact_id, number, author, role, session, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
            params![id, n, ctx.author, ctx.role.as_str(), ctx.session],
        )
        .map_err(db_err)?;
        tx.execute(
            "INSERT OR IGNORE INTO blobs (sha256, bytes) VALUES (?1, ?2)",
            params![sha, bytes],
        )
        .map_err(db_err)?;
        tx.execute(
            "INSERT INTO revision_files (artifact_id, revision, path, media_type, size, sha256)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, n, path, media_type, bytes.len() as i64, sha],
        )
        .map_err(db_err)?;
        tx.execute(
            "UPDATE artifacts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            [id],
        )
        .map_err(db_err)?;
        Ok(Ref {
            id: id.to_string(),
            revision: n,
        })
    })
}

/// Content of the single file of an Artifact's Revision (latest by default).
pub fn get(conn: &Connection, id: &str, revision: Option<i64>) -> Result<Vec<u8>, Error> {
    let exists: bool = conn
        .query_row("SELECT 1 FROM artifacts WHERE id = ?1", [id], |_| Ok(true))
        .optional()
        .map_err(db_err)?
        .unwrap_or(false);
    if !exists {
        return Err(Error::NotFound(format!("no artifact {id}")));
    }
    let rev = match revision {
        Some(r) => r,
        None => conn
            .query_row(
                "SELECT MAX(number) FROM revisions WHERE artifact_id = ?1",
                [id],
                |r| r.get::<_, Option<i64>>(0),
            )
            .map_err(db_err)?
            .ok_or_else(|| Error::NotFound(format!("artifact {id} has no revisions")))?,
    };
    conn.query_row(
        "SELECT b.bytes FROM revision_files f JOIN blobs b ON b.sha256 = f.sha256
         WHERE f.artifact_id = ?1 AND f.revision = ?2",
        params![id, rev],
        |r| r.get::<_, Vec<u8>>(0),
    )
    .optional()
    .map_err(db_err)?
    .ok_or_else(|| Error::NotFound(format!("no revision {id}@{rev}")))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_display_and_json() {
        let r = Ref {
            id: "abc123".into(),
            revision: 2,
        };
        assert_eq!(r.to_string(), "abc123@2");
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"id":"abc123","revision":2,"ref":"abc123@2"}"#
        );
    }

    #[test]
    fn parse_targets() {
        assert_eq!(parse_target("abc").unwrap(), ("abc".into(), None));
        assert_eq!(parse_target("abc@3").unwrap(), ("abc".into(), Some(3)));
        assert_eq!(parse_target("abc@x").unwrap_err().exit_code(), 2);
        assert_eq!(parse_target("abc@0").unwrap_err().exit_code(), 2);
        assert_eq!(parse_target("@1").unwrap_err().exit_code(), 2);
    }
}
