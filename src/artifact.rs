//! Core Artifact operations. Callers hand in a connection from `db::open`.

use std::fmt;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::bundle::{self, FileInput, Input, RawFile};
use crate::cli::Context;
use crate::comments;
use crate::db::{db_err, write_tx};
use crate::diagram::Format;
use crate::error::Error;
use crate::ids::new_artifact_id;
use crate::kind::Kind;
use crate::scope::{Scope, ScopeFilter};
use crate::sections;

pub struct SubmitInput {
    pub name: String,
    pub kind: Kind,
    /// Diagram format; required for a diagram, rejected otherwise.
    pub format: Option<Format>,
    pub input: Input,
    pub scope: Option<Scope>,
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
    let files = bundle::prepare(input.kind, input.format, input.input, &input.name)?;
    write_tx(conn, |tx| {
        let mut id = None;
        for _ in 0..5 {
            let candidate = new_artifact_id()?;
            let n = tx
                .execute(
                    "INSERT OR IGNORE INTO artifacts (id, kind, format, name, project_name, project_root, worktree, created_at, updated_at)
                     VALUES (?1, ?2, ?7, ?3, ?4, ?5, ?6, strftime('%Y-%m-%dT%H:%M:%fZ','now'), strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
                    params![
                        candidate,
                        input.kind.as_str(),
                        input.name,
                        input.scope.as_ref().map(|s| &s.name),
                        input.scope.as_ref().map(|s| &s.root),
                        input.scope.as_ref().and_then(|s| s.worktree.as_ref()),
                        input.format.map(Format::as_str)
                    ],
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
        insert_files(tx, &id, 1, &files)?;
        record_embeds(tx, input.kind == Kind::Markdown, &id, 1, None, &files)?;
        Ok(Ref { id, revision: 1 })
    })
}

/// Validate and record the embeds of a new markdown Revision. `prev_rev` is
/// the Revision it replaces; its dangling embeds are tolerated.
fn record_embeds(
    tx: &rusqlite::Transaction,
    is_markdown: bool,
    id: &str,
    revision: i64,
    prev_rev: Option<i64>,
    files: &[FileInput],
) -> Result<(), Error> {
    if !is_markdown {
        return Ok(());
    }
    for f in files {
        let text = std::str::from_utf8(&f.bytes)
            .map_err(|_| Error::Rejected("markdown is not valid UTF-8".into()))?;
        crate::embeds::record(tx, id, revision, prev_rev, text)?;
    }
    Ok(())
}

/// Store a Revision's file set. Blobs are content-addressed, so a file that
/// did not change shares its blob with earlier Revisions.
fn insert_files(
    tx: &rusqlite::Transaction,
    id: &str,
    revision: i64,
    files: &[FileInput],
) -> Result<(), Error> {
    for f in files {
        let sha = hex(&Sha256::digest(&f.bytes));
        tx.execute(
            "INSERT OR IGNORE INTO blobs (sha256, bytes) VALUES (?1, ?2)",
            params![sha, f.bytes],
        )
        .map_err(db_err)?;
        tx.execute(
            "INSERT INTO revision_files (artifact_id, revision, path, media_type, size, sha256)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                revision,
                f.path,
                f.media_type,
                f.bytes.len() as i64,
                sha
            ],
        )
        .map_err(db_err)?;
    }
    Ok(())
}

/// Append a new full-snapshot Revision. The latest number and the existence
/// check are read inside the write transaction, so concurrent writers get
/// distinct, consecutive numbers.
pub fn edit(
    conn: &mut Connection,
    ctx: &Context,
    id: &str,
    input: Input,
    base: Option<i64>,
    addresses: &[i64],
) -> Result<Ref, Error> {
    // Cheap checks (missing Artifact, stale base, unknown addresses) run first
    // so they never pay for, or lose to a Rejected from, the Mermaid render.
    // The render can take up to the render time limit, so it then runs before
    // the write lock is taken (as submit does); the checks and only the inputs
    // it depended on are re-checked inside the transaction. Kind and format are
    // fixed at creation; the single file path of a non-Html Artifact can
    // change, which is the one real race.
    check_preconditions(conn, id, base, addresses)?;
    let plan = EditPlan::read(conn, id)?;
    let mut input = input;
    if plan.kind == Kind::Image && input.files.len() == 1 && input.files[0].path == "-" {
        input.files[0].path = plan.single_path.clone();
    }
    let files = bundle::prepare(plan.kind, plan.format, input, &plan.single_path)?;
    append_revision(conn, ctx, id, base, addresses, |_, latest| {
        if EditPlan::read(latest.tx, id)? != plan {
            return Err(Error::Conflict(
                "the Artifact changed while the edit was being prepared; retry".into(),
            ));
        }
        Ok(files)
    })
}

/// What `edit` reads from the Artifact to prepare a new Revision.
#[derive(Debug, PartialEq, Eq)]
struct EditPlan {
    kind: Kind,
    format: Option<Format>,
    /// The latest Revision's only file path; empty for Html bundles.
    single_path: String,
}

impl EditPlan {
    fn read(conn: &Connection, id: &str) -> Result<Self, Error> {
        let kind: String = conn
            .query_row("SELECT kind FROM artifacts WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| Error::NotFound(format!("no artifact {id}")))?;
        let kind = Kind::parse(&kind)?;
        let format = if kind == Kind::Diagram {
            Some(diagram_format(conn, id)?)
        } else {
            None
        };
        let single_path = if kind == Kind::Html {
            String::new()
        } else {
            let revision: i64 = conn
                .query_row(
                    "SELECT MAX(number) FROM revisions WHERE artifact_id = ?1",
                    [id],
                    |r| r.get::<_, Option<i64>>(0),
                )
                .map_err(db_err)?
                .ok_or_else(|| Error::NotFound(format!("artifact {id} has no revisions")))?;
            Latest {
                tx: conn,
                id,
                revision,
            }
            .single_path()?
        };
        Ok(EditPlan {
            kind,
            format,
            single_path,
        })
    }
}

/// Replace one Section of the latest markdown Revision. The splice runs
/// inside the same write transaction as the insert, so it always applies to
/// the Revision it becomes the successor of.
pub fn section_edit(
    conn: &mut Connection,
    ctx: &Context,
    id: &str,
    query: &str,
    replacement: &[u8],
    base: Option<i64>,
    addresses: &[i64],
) -> Result<Ref, Error> {
    let replacement = std::str::from_utf8(replacement)
        .map_err(|_| Error::Rejected("section replacement is not valid UTF-8".into()))?;
    append_revision(conn, ctx, id, base, addresses, |kind, latest| {
        if kind != Kind::Markdown.as_str() {
            return Err(Error::Rejected(format!(
                "sections are for markdown Artifacts, not {kind}"
            )));
        }
        let path = latest.single_path()?;
        let bytes = latest.single_bytes()?;
        let text = utf8_markdown(kind, &bytes)?;
        let spliced = sections::splice(text, query, replacement)?.into_bytes();
        bundle::prepare(
            Kind::Markdown,
            None,
            Input {
                files: vec![RawFile {
                    path: path.clone(),
                    bytes: spliced,
                }],
                dir: false,
            },
            &path,
        )
    })
}

fn utf8_markdown<'a>(kind: &str, bytes: &'a [u8]) -> Result<&'a str, Error> {
    if kind != Kind::Markdown.as_str() {
        return Err(Error::Rejected(format!(
            "sections are for markdown Artifacts, not {kind}"
        )));
    }
    std::str::from_utf8(bytes)
        .map_err(|_| Error::Rejected("markdown Artifact is not valid UTF-8".into()))
}

/// A diagram Artifact's stored format; rejects any other Kind.
pub fn diagram_format(conn: &Connection, id: &str) -> Result<Format, Error> {
    let (kind, format): (String, Option<String>) = conn
        .query_row(
            "SELECT kind, format FROM artifacts WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(db_err)?
        .ok_or_else(|| Error::NotFound(format!("no artifact {id}")))?;
    match (kind.as_str(), format) {
        ("diagram", Some(f)) => Format::parse(&f),
        ("diagram", None) => Err(Error::Unexpected(format!("diagram {id} has no format"))),
        _ => Err(Error::Rejected(format!("{id} is a {kind}, not a diagram"))),
    }
}

/// Text of a markdown Artifact's Revision (latest by default); rejects other
/// Kinds and non-UTF-8 content.
pub fn markdown_text(conn: &Connection, id: &str, revision: Option<i64>) -> Result<String, Error> {
    let kind: String = conn
        .query_row("SELECT kind FROM artifacts WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()
        .map_err(db_err)?
        .ok_or_else(|| Error::NotFound(format!("no artifact {id}")))?;
    let bytes = get(conn, id, revision)?;
    Ok(utf8_markdown(&kind, &bytes)?.to_string())
}

/// The latest Revision's files, read lazily so a Bundle edit that does not
/// need them never loads their bytes.
struct Latest<'a> {
    tx: &'a Connection,
    id: &'a str,
    revision: i64,
}

impl Latest<'_> {
    /// Path-only query: does not load the blob bytes.
    fn single_path(&self) -> Result<String, Error> {
        let mut stmt = self
            .tx
            .prepare("SELECT path FROM revision_files WHERE artifact_id = ?1 AND revision = ?2")
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![self.id, self.revision], |r| r.get::<_, String>(0))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        let mut it = rows.into_iter();
        match (it.next(), it.next()) {
            (Some(path), None) => Ok(path),
            _ => Err(Error::Rejected(
                "the latest Revision is not a single file".into(),
            )),
        }
    }

    fn single_bytes(&self) -> Result<Vec<u8>, Error> {
        Ok(self.single()?.1)
    }

    fn single(&self) -> Result<(String, Vec<u8>), Error> {
        let mut stmt = self
            .tx
            .prepare(
                "SELECT f.path, b.bytes FROM revision_files f
                 JOIN blobs b ON b.sha256 = f.sha256
                 WHERE f.artifact_id = ?1 AND f.revision = ?2",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![self.id, self.revision], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        let mut it = rows.into_iter();
        match (it.next(), it.next()) {
            (Some(row), None) => Ok(row),
            _ => Err(Error::Rejected(
                "the latest Revision is not a single file".into(),
            )),
        }
    }
}

/// What `check_preconditions` learned: Kind, latest Revision number, addressed
/// threads as (root id, its Revision).
struct Preconditions {
    kind: String,
    latest: i64,
    threads: Vec<(i64, i64)>,
}

/// The cheap, read-only checks every edit must pass: the Artifact exists, a
/// given base is the latest Revision, and every addressed id is a root
/// Comment of it. Returns the Kind, the latest Revision number and the
/// addressed threads. `edit` runs it before the slow prepare and again inside
/// the write transaction, where it is authoritative.
fn check_preconditions(
    conn: &Connection,
    id: &str,
    base: Option<i64>,
    addresses: &[i64],
) -> Result<Preconditions, Error> {
    let kind: String = conn
        .query_row("SELECT kind FROM artifacts WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()
        .map_err(db_err)?
        .ok_or_else(|| Error::NotFound(format!("no artifact {id}")))?;
    let (latest, author): (i64, String) = conn
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
    // Validate before any insert so a bad id leaves no Revision.
    let threads = comments::check_addressable(conn, id, addresses)?;
    Ok(Preconditions {
        kind,
        latest,
        threads,
    })
}

/// Shared insert path. `produce` gets the Artifact's Kind and its latest
/// Revision and returns the checked files of the new one.
fn append_revision(
    conn: &mut Connection,
    ctx: &Context,
    id: &str,
    base: Option<i64>,
    addresses: &[i64],
    produce: impl FnOnce(&str, &Latest) -> Result<Vec<FileInput>, Error>,
) -> Result<Ref, Error> {
    write_tx(conn, |tx| {
        let Preconditions {
            kind,
            latest,
            threads,
        } = check_preconditions(tx, id, base, addresses)?;
        let files = produce(
            &kind,
            &Latest {
                tx,
                id,
                revision: latest,
            },
        )?;
        let n = latest + 1;
        tx.execute(
            "INSERT INTO revisions (artifact_id, number, author, role, session, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
            params![id, n, ctx.author, ctx.role.as_str(), ctx.session],
        )
        .map_err(db_err)?;
        insert_files(tx, id, n, &files)?;
        record_embeds(
            tx,
            kind == Kind::Markdown.as_str(),
            id,
            n,
            Some(latest),
            &files,
        )?;
        tx.execute(
            "UPDATE artifacts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            [id],
        )
        .map_err(db_err)?;
        let new_ref = Ref {
            id: id.to_string(),
            revision: n,
        };
        comments::post_addressed(tx, ctx, id, &threads, &new_ref.to_string())?;
        Ok(new_ref)
    })
}

/// One stored file of a Revision, with its bytes.
pub struct StoredFile {
    pub path: String,
    pub media_type: String,
    pub size: i64,
    pub bytes: Vec<u8>,
}

/// An Artifact's Kind and every file of a Revision (latest by default).
pub fn get_files(
    conn: &Connection,
    id: &str,
    revision: Option<i64>,
) -> Result<(Kind, Vec<StoredFile>), Error> {
    let kind: String = conn
        .query_row("SELECT kind FROM artifacts WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()
        .map_err(db_err)?
        .ok_or_else(|| Error::NotFound(format!("no artifact {id}")))?;
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
    let mut stmt = conn
        .prepare(
            "SELECT f.path, f.media_type, f.size, b.bytes FROM revision_files f
             JOIN blobs b ON b.sha256 = f.sha256
             WHERE f.artifact_id = ?1 AND f.revision = ?2 ORDER BY f.path",
        )
        .map_err(db_err)?;
    let files = stmt
        .query_map(params![id, rev], |r| {
            Ok(StoredFile {
                path: r.get(0)?,
                media_type: r.get(1)?,
                size: r.get(2)?,
                bytes: r.get(3)?,
            })
        })
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    if files.is_empty() {
        return Err(Error::NotFound(format!("no revision {id}@{rev}")));
    }
    Ok((Kind::parse(&kind)?, files))
}

/// An Artifact's Kind, or None when there is no such Artifact.
pub fn kind_of(conn: &Connection, id: &str) -> Result<Option<Kind>, Error> {
    conn.query_row("SELECT kind FROM artifacts WHERE id = ?1", [id], |r| {
        r.get::<_, String>(0)
    })
    .optional()
    .map_err(db_err)?
    .map(|k| Kind::parse(&k))
    .transpose()
}

/// One file of a Revision by exact stored path, or None. The lookup is an
/// equality match on the database column, so a request path can never reach
/// the filesystem.
pub fn get_file(
    conn: &Connection,
    id: &str,
    revision: i64,
    path: &str,
) -> Result<Option<StoredFile>, Error> {
    conn.query_row(
        "SELECT f.path, f.media_type, f.size, b.bytes FROM revision_files f
         JOIN blobs b ON b.sha256 = f.sha256
         WHERE f.artifact_id = ?1 AND f.revision = ?2 AND f.path = ?3",
        params![id, revision, path],
        |r| {
            Ok(StoredFile {
                path: r.get(0)?,
                media_type: r.get(1)?,
                size: r.get(2)?,
                bytes: r.get(3)?,
            })
        },
    )
    .optional()
    .map_err(db_err)
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
    let (_, mut files) = get_files(conn, id, Some(rev))?;
    if files.len() != 1 {
        return Err(Error::Usage(format!(
            "{id}@{rev} has {} files: pass --out <dir>",
            files.len()
        )));
    }
    Ok(files.remove(0).bytes)
}

/// Filters for [`list`]. `session` and `mine` look at the latest Revision,
/// the one the AUTHOR column shows.
pub struct ListFilter {
    pub kind: Option<Kind>,
    /// Latest Revision made in this Session.
    pub session: Option<String>,
    /// Latest Revision made by this author.
    pub author: Option<String>,
    /// Only Artifacts with at least one open Thread.
    pub open: bool,
    /// Only Artifacts last updated more than this many seconds ago.
    pub older_than_secs: Option<i64>,
    pub limit: i64,
    pub scope: ScopeFilter,
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
    pub project_root: Option<String>,
    pub worktree: Option<String>,
}

/// Artifacts, newest activity first.
pub fn list(conn: &Connection, f: &ListFilter) -> Result<Vec<ListRow>, Error> {
    let sql = format!(
        "SELECT a.id, a.kind, a.name,
                (SELECT MAX(number) FROM revisions WHERE artifact_id = a.id),
                (SELECT COUNT(*) FROM comments c WHERE c.artifact_id = a.id AND c.parent_id IS NULL AND c.status = 'open'),
                (SELECT author FROM revisions WHERE artifact_id = a.id ORDER BY number DESC LIMIT 1),
                COALESCE((SELECT SUM(LENGTH(b.bytes)) FROM blobs b WHERE b.sha256 IN
                    (SELECT sha256 FROM revision_files WHERE artifact_id = a.id)), 0),
                a.updated_at, a.project_name, a.worktree, a.project_root
         FROM artifacts a
         WHERE (?1 IS NULL OR a.kind = ?1)
           AND (?2 IS NULL OR EXISTS (SELECT 1 FROM revisions r
                 WHERE r.artifact_id = a.id AND r.number = (SELECT MAX(number) FROM revisions WHERE artifact_id = a.id) AND r.session = ?2))
           AND (?3 IS NULL OR EXISTS (SELECT 1 FROM revisions r
                 WHERE r.artifact_id = a.id AND r.number = (SELECT MAX(number) FROM revisions WHERE artifact_id = a.id) AND r.author = ?3))
           AND (?4 IS NULL OR a.updated_at < strftime('%Y-%m-%dT%H:%M:%fZ','now', ?4))
           AND (?6 = 0 OR EXISTS (SELECT 1 FROM comments c
                 WHERE c.artifact_id = a.id AND c.parent_id IS NULL AND c.status = 'open'))
           AND {scope}
         ORDER BY a.updated_at DESC, a.id DESC
         LIMIT ?5",
        scope = f.scope.sql("a", 7)
    );
    let older = f.older_than_secs.map(|n| format!("-{n} seconds"));
    let mut stmt = conn.prepare(&sql).map_err(db_err)?;
    let [sn, sr, sa, sw] = f.scope.params();
    let rows = stmt
        .query_map(
            params![
                f.kind.map(|k| k.as_str()),
                f.session,
                f.author,
                older,
                f.limit,
                f.open as i64,
                sn,
                sr,
                sa,
                sw
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
                    project_root: r.get(10)?,
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

pub(crate) fn hex(bytes: &[u8]) -> String {
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

    fn test_ctx(dir: &std::path::Path) -> Context {
        Context {
            role: crate::cli::Role::Agent,
            author: "t".into(),
            session: None,
            session_explicit: None,
            db_path: dir.join("db"),
            project: None,
        }
    }

    fn raw(path: &str, body: &str) -> Input {
        Input {
            files: vec![RawFile {
                path: path.into(),
                bytes: body.as_bytes().to_vec(),
            }],
            dir: false,
        }
    }

    #[test]
    fn invalid_edit_is_rejected_without_taking_the_write_lock() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_ctx(dir.path());
        let mut conn = crate::db::open(&ctx.db_path).unwrap();
        let r = submit(
            &mut conn,
            &ctx,
            SubmitInput {
                name: "d.svg".into(),
                kind: Kind::Diagram,
                format: Some(Format::Svg),
                input: raw("d.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
                scope: None,
            },
        )
        .unwrap();
        // Another connection holds the write lock; a rejected edit must come
        // back as Rejected, not as a busy error from waiting on that lock.
        let mut other = crate::db::open(&ctx.db_path).unwrap();
        let held = other
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let start = std::time::Instant::now();
        let err = edit(&mut conn, &ctx, &r.id, raw("d.svg", "not svg"), None, &[]).unwrap_err();
        assert!(matches!(err, Error::Rejected(_)), "{err:?}");
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
        drop(held);
    }

    #[test]
    fn cheap_checks_win_over_prepare_rejection() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_ctx(dir.path());
        let mut conn = crate::db::open(&ctx.db_path).unwrap();
        let r = submit(
            &mut conn,
            &ctx,
            SubmitInput {
                name: "d.svg".into(),
                kind: Kind::Diagram,
                format: Some(Format::Svg),
                input: raw("d.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
                scope: None,
            },
        )
        .unwrap();
        let bad = || raw("d.svg", "not svg");
        let err = edit(&mut conn, &ctx, &r.id, bad(), Some(99), &[]).unwrap_err();
        assert!(matches!(err, Error::Conflict(_)), "{err:?}");
        let err = edit(&mut conn, &ctx, "nope", bad(), None, &[]).unwrap_err();
        assert!(matches!(err, Error::NotFound(_)), "{err:?}");
        let err = edit(&mut conn, &ctx, &r.id, bad(), None, &[12345]).unwrap_err();
        assert!(matches!(err, Error::NotFound(_)), "{err:?}");
    }

    #[test]
    fn edit_plan_changes_when_the_single_path_changes() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_ctx(dir.path());
        let mut conn = crate::db::open(&ctx.db_path).unwrap();
        let r = submit(
            &mut conn,
            &ctx,
            SubmitInput {
                name: "a.md".into(),
                kind: Kind::Markdown,
                format: None,
                input: raw("a.md", "# hi\n"),
                scope: None,
            },
        )
        .unwrap();
        let before = EditPlan::read(&conn, &r.id).unwrap();
        assert_eq!(before.single_path, "a.md");
        conn.execute(
            "UPDATE revision_files SET path = 'b.md' WHERE artifact_id = ?1",
            [&r.id],
        )
        .unwrap();
        assert_ne!(EditPlan::read(&conn, &r.id).unwrap(), before);
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
