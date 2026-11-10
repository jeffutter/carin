//! The only place a SQLite connection is opened. Connection pragmas, the
//! schema migration and the immediate write transaction all live here.

use std::path::Path;

use rusqlite::{Connection, ErrorCode, Transaction, TransactionBehavior};

use crate::error::Error;

pub const BUSY_TIMEOUT_MS: u32 = 5000;

pub fn db_err(e: rusqlite::Error) -> Error {
    if let rusqlite::Error::SqliteFailure(f, _) = &e
        && matches!(f.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
    {
        return Error::Unexpected("database busy".into());
    }
    Error::Unexpected(format!("database error: {e}"))
}

const SCHEMA_V1: &str = "
CREATE TABLE artifacts (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('markdown','html','image','diagram')),
    format TEXT CHECK (format IS NULL OR format IN ('mermaid','svg')),
    name TEXT NOT NULL,
    project_name TEXT,
    project_root TEXT,
    worktree TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE revisions (
    artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    number INTEGER NOT NULL,
    author TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('agent','reviewer')),
    session TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY (artifact_id, number)
);
CREATE TABLE blobs (
    sha256 TEXT PRIMARY KEY,
    bytes BLOB NOT NULL
);
CREATE TABLE revision_files (
    artifact_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    path TEXT NOT NULL,
    media_type TEXT NOT NULL,
    size INTEGER NOT NULL,
    sha256 TEXT NOT NULL REFERENCES blobs(sha256),
    PRIMARY KEY (artifact_id, revision, path),
    FOREIGN KEY (artifact_id, revision) REFERENCES revisions(artifact_id, number) ON DELETE CASCADE
);
";

const SCHEMA_V2: &str = "
CREATE TABLE comments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    artifact_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    parent_id INTEGER REFERENCES comments(id) ON DELETE CASCADE,
    author TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('agent','reviewer')),
    session TEXT,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL,
    line_start INTEGER,
    line_end INTEGER,
    quoted_text TEXT,
    section_path TEXT,
    status TEXT CHECK (status IS NULL OR status IN ('open','resolved')),
    FOREIGN KEY (artifact_id, revision) REFERENCES revisions(artifact_id, number) ON DELETE CASCADE
);
CREATE INDEX comments_artifact ON comments(artifact_id, id);
CREATE INDEX comments_parent ON comments(parent_id);
";

const SCHEMA_V3: &str = "
CREATE TABLE embeds (
    artifact_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    ordinal INTEGER NOT NULL,
    target_id TEXT NOT NULL,
    target_revision INTEGER NOT NULL,
    pinned INTEGER NOT NULL,
    PRIMARY KEY (artifact_id, revision, ordinal),
    FOREIGN KEY (artifact_id, revision) REFERENCES revisions(artifact_id, number) ON DELETE CASCADE
);
";

/// Open (creating the directory and file on first use) and migrate.
pub fn open(path: &Path) -> Result<Connection, Error> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|e| Error::Unexpected(format!("cannot create {}: {e}", dir.display())))?;
    }
    let conn = Connection::open(path).map_err(db_err)?;
    configure(&conn).map_err(db_err)?;
    migrate(&conn).map_err(db_err)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS as u64))?;
    // auto_vacuum only takes effect on a database with no tables yet, so it
    // is set before the schema exists and before WAL is enabled.
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version == 0 {
        conn.pragma_update(None, "auto_vacuum", "FULL")?;
    }
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        // Immediate, and re-check the version inside, so two first-use
        // processes cannot both create the schema.
        let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
        let v: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if v < 1 {
            tx.execute_batch(SCHEMA_V1)?;
            tx.pragma_update(None, "user_version", 1)?;
        }
        tx.commit()?;
    }
    if version < 2 {
        let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
        let v: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if v < 2 {
            tx.execute_batch(SCHEMA_V2)?;
            tx.pragma_update(None, "user_version", 2)?;
        }
        tx.commit()?;
    }
    if version < 3 {
        let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
        let v: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if v < 3 {
            tx.execute_batch(SCHEMA_V3)?;
            tx.pragma_update(None, "user_version", 3)?;
        }
        tx.commit()?;
    }
    Ok(())
}

/// Run `f` in a `BEGIN IMMEDIATE` transaction. Every write goes through here.
pub fn write_tx<T>(
    conn: &mut Connection,
    f: impl FnOnce(&Transaction) -> Result<T, Error>,
) -> Result<T, Error> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_err)?;
    let out = f(&tx)?;
    tx.commit().map_err(db_err)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pragma(conn: &Connection, name: &str) -> String {
        conn.query_row(&format!("PRAGMA {name}"), [], |r| {
            r.get::<_, rusqlite::types::Value>(0)
        })
        .map(|v| match v {
            rusqlite::types::Value::Integer(i) => i.to_string(),
            rusqlite::types::Value::Text(t) => t,
            other => format!("{other:?}"),
        })
        .unwrap()
    }

    #[test]
    fn pragmas_applied_and_dir_created() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("a/b/c.db")).unwrap();
        assert_eq!(pragma(&conn, "journal_mode"), "wal");
        assert_eq!(pragma(&conn, "foreign_keys"), "1");
        assert_eq!(pragma(&conn, "busy_timeout"), "5000");
        assert_eq!(pragma(&conn, "auto_vacuum"), "1");
        assert_eq!(pragma(&conn, "user_version"), "3");
    }

    #[test]
    fn v1_database_upgrades_to_v3_keeping_rows() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.db");
        {
            let conn = Connection::open(&p).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute_batch(
                "INSERT INTO artifacts VALUES ('a1','markdown',NULL,'n',NULL,NULL,NULL,'t','t');
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        }
        let conn = open(&p).unwrap();
        assert_eq!(pragma(&conn, "user_version"), "3");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM artifacts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        conn.execute("SELECT COUNT(*) FROM comments", []).ok();
    }

    #[test]
    fn reopen_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.db");
        drop(open(&p).unwrap());
        let conn = open(&p).unwrap();
        assert_eq!(pragma(&conn, "auto_vacuum"), "1");
    }

    #[test]
    fn write_tx_takes_write_lock_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.db");
        let mut first = open(&p).unwrap();
        let second = open(&p).unwrap();
        second
            .busy_timeout(std::time::Duration::from_millis(0))
            .unwrap();
        write_tx(&mut first, |_tx| {
            // No statement has run in the first tx; an IMMEDIATE begin
            // still holds the write lock.
            let err = second.execute_batch("BEGIN IMMEDIATE").unwrap_err();
            assert_eq!(
                db_err(err).to_string(),
                "database busy",
                "second writer must be refused while the first tx is open"
            );
            Ok(())
        })
        .unwrap();
        second.execute_batch("BEGIN IMMEDIATE; COMMIT").unwrap();
    }

    #[test]
    fn only_this_module_opens_connections() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let needle = ["Connection", "::open"].concat();
        for entry in std::fs::read_dir(src).unwrap() {
            let path = entry.unwrap().path();
            // src/ui/ holds only assets.
            if path.is_dir() || path.file_name().unwrap() == "db.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(!text.contains(&needle), "{} opens sqlite", path.display());
        }
    }
}
