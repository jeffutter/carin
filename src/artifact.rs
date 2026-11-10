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

/// Filters for [`list`]. `session` and `mine` look at Revision 1, the
/// Revision that created the Artifact.
pub struct ListFilter {
    pub kind: Option<Kind>,
    /// Created in this Session.
    pub session: Option<String>,
    /// Created by this author.
    pub author: Option<String>,
    /// Only Artifacts with open Threads.
    pub open: bool,
    /// Only Artifacts last updated more than this many seconds ago.
    pub older_than_secs: Option<i64>,
    pub limit: i64,
}

#[derive(Debug, Serialize)]
pub struct ListRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub latest_revision: i64,
    pub open_threads: i64,
    pub author: String,
    /// Sum of the sizes of the distinct blobs any Revision of the Artifact
    /// references: what the Artifact costs in the database file.
    pub bytes: i64,
    pub updated_at: String,
    pub project: Option<String>,
    pub worktree: Option<String>,
}

/// Artifacts, newest activity first.
pub fn list(conn: &Connection, f: &ListFilter) -> Result<Vec<ListRow>, Error> {
    // Threads arrive with the comments table (TASK-6); until then none are open.
    const OPEN_THREADS: &str = "0";
    let sql = format!(
        "SELECT a.id, a.kind, a.name,
                (SELECT MAX(number) FROM revisions WHERE artifact_id = a.id),
                {OPEN_THREADS},
                (SELECT author FROM revisions WHERE artifact_id = a.id ORDER BY number DESC LIMIT 1),
                COALESCE((SELECT SUM(LENGTH(b.bytes)) FROM blobs b WHERE b.sha256 IN
                    (SELECT sha256 FROM revision_files WHERE artifact_id = a.id)), 0),
                a.updated_at, a.project_name, a.worktree
         FROM artifacts a
         WHERE (?1 IS NULL OR a.kind = ?1)
           AND (?2 IS NULL OR EXISTS (SELECT 1 FROM revisions r
                 WHERE r.artifact_id = a.id AND r.number = 1 AND r.session = ?2))
           AND (?3 IS NULL OR EXISTS (SELECT 1 FROM revisions r
                 WHERE r.artifact_id = a.id AND r.number = 1 AND r.author = ?3))
           AND (?4 = 0 OR {OPEN_THREADS} > 0)
           AND (?5 IS NULL OR a.updated_at < strftime('%Y-%m-%dT%H:%M:%fZ','now', ?5))
         ORDER BY a.updated_at DESC, a.id DESC
         LIMIT ?6"
    );
    let older = f.older_than_secs.map(|n| format!("-{n} seconds"));
    let mut stmt = conn.prepare(&sql).map_err(db_err)?;
    let rows = stmt
        .query_map(
            params![
                f.kind.map(|k| k.as_str()),
                f.session,
                f.author,
                f.open as i64,
                older,
                f.limit
            ],
            |r| {
                Ok(ListRow {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    name: r.get(2)?,
                    latest_revision: r.get(3)?,
                    open_threads: r.get(4)?,
                    author: r.get(5)?,
                    bytes: r.get(6)?,
                    updated_at: r.get(7)?,
                    project: r.get(8)?,
                    worktree: r.get(9)?,
                })
            },
        )
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    Ok(rows)
}

/// Seconds in a duration like `30d`, `12h`, `90m`, `45s` or `2w`.
pub fn parse_duration(s: &str) -> Result<i64, Error> {
    let bad = || {
        Error::Usage(format!(
            "bad duration {s:?}: use a number and s, m, h, d or w"
        ))
    };
    let unit = s.chars().last().ok_or_else(bad)?;
    let mult: i64 = match unit {
        's' => 1,
        'm' => 60,
        'h' => 3600,
        'd' => 86_400,
        'w' => 604_800,
        _ => return Err(bad()),
    };
    let digits = &s[..s.len() - 1];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    digits
        .parse::<i64>()
        .ok()
        .and_then(|n| n.checked_mul(mult))
        .filter(|n| *n <= 100 * 365 * 86_400)
        .ok_or_else(bad)
}

/// Delete Artifacts with all their Revisions (by cascade) and any blob no
/// remaining Revision references. All or nothing: one unknown id deletes
/// nothing and is NotFound.
pub fn delete(conn: &mut Connection, ids: &[String]) -> Result<Vec<String>, Error> {
    let mut unique: Vec<String> = Vec::new();
    for id in ids {
        if !unique.contains(id) {
            unique.push(id.clone());
        }
    }
    write_tx(conn, |tx| {
        let mut missing = Vec::new();
        for id in &unique {
            let found = tx
                .query_row("SELECT 1 FROM artifacts WHERE id = ?1", [id], |_| Ok(()))
                .optional()
                .map_err(db_err)?
                .is_some();
            if !found {
                missing.push(id.as_str());
            }
        }
        if !missing.is_empty() {
            return Err(Error::NotFound(format!(
                "no artifact {}",
                missing.join(", ")
            )));
        }
        for id in &unique {
            tx.execute("DELETE FROM artifacts WHERE id = ?1", [id])
                .map_err(db_err)?;
        }
        tx.execute(
            "DELETE FROM blobs WHERE sha256 NOT IN (SELECT sha256 FROM revision_files)",
            [],
        )
        .map_err(db_err)?;
        Ok(unique)
    })
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
    fn durations() {
        assert_eq!(parse_duration("45s").unwrap(), 45);
        assert_eq!(parse_duration("90m").unwrap(), 5400);
        assert_eq!(parse_duration("12h").unwrap(), 43_200);
        assert_eq!(parse_duration("30d").unwrap(), 2_592_000);
        assert_eq!(parse_duration("2w").unwrap(), 1_209_600);
        for bad in ["", "d", "30", "-3d", "3x", "1.5h", "3 d", "99999999999999d"] {
            assert_eq!(parse_duration(bad).unwrap_err().exit_code(), 2, "{bad}");
        }
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
