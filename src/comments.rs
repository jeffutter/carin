//! Comments and Threads (docs/data-model.md "Comments and Threads").
//!
//! A Thread is a root Comment plus flat replies. The root carries the locator
//! and the `open`/`resolved` flag, the only mutable part of a Comment.
//! Comments are never edited or deleted, and anchors are never rewritten.

use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::artifact::parse_target;
use crate::cli::Context;
use crate::db::{db_err, write_tx};
use crate::error::Error;
use crate::limits;
use crate::scope::ScopeFilter;
use crate::sections;

/// Where a root Comment points inside its Revision.
#[derive(Debug, Default, Clone)]
pub struct Locator {
    /// Inclusive 1-based line range.
    pub lines: Option<(i64, i64)>,
    /// Markdown Section query ("A > B").
    pub section: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct StoredLocator {
    pub line_start: Option<i64>,
    pub line_end: Option<i64>,
    pub quoted_text: Option<String>,
    pub section_path: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Reply {
    pub id: i64,
    pub author: String,
    pub role: String,
    pub session: Option<String>,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Thread {
    pub id: i64,
    pub artifact_id: String,
    pub revision: i64,
    pub latest_revision: i64,
    /// The Comment's Revision is not the Artifact's latest.
    pub stale: bool,
    pub status: String,
    pub author: String,
    pub role: String,
    pub session: Option<String>,
    pub body: String,
    pub created_at: String,
    pub locator: StoredLocator,
    pub replies: Vec<Reply>,
}

#[derive(Debug, Serialize)]
pub struct Added {
    pub id: i64,
    /// The root Comment's id: the Comment's own id for a root.
    pub thread_id: i64,
    pub artifact_id: String,
    pub revision: i64,
    pub latest_revision: i64,
    pub stale: bool,
    pub locator: Option<StoredLocator>,
    pub highest_comment_id: i64,
}

#[derive(Debug, Serialize)]
pub struct ThreadResult {
    pub thread: Thread,
    pub highest_comment_id: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CommentList {
    pub threads: Vec<Thread>,
    pub highest_comment_id: i64,
}

pub struct ListQuery {
    /// Bare artifact id; overrides Session scoping.
    pub artifact: Option<String>,
    /// Include resolved Threads.
    pub all: bool,
    /// Only Threads holding a Comment with a larger id.
    pub since: Option<i64>,
    /// Project scope, used only when neither an Artifact nor a Session is explicit.
    pub scope: ScopeFilter,
}

/// Highest Comment id ever issued, read from the AUTOINCREMENT counter so
/// deleting the newest Artifact never lowers it.
fn highest_comment_id(conn: &Connection) -> Result<i64, Error> {
    conn.query_row(
        "SELECT COALESCE((SELECT seq FROM sqlite_sequence WHERE name = 'comments'), 0)",
        [],
        |r| r.get(0),
    )
    .map_err(db_err)
}

/// A cheap fingerprint of everything the review UI shows for one Artifact:
/// changes whenever a Comment is added to it, a Thread is resolved or reopened
/// (which adds no Comment), or a Revision is added. Scoped to `artifact`, so
/// activity on other Artifacts never changes it. The resolved Threads are
/// listed by id rather than summed, so no two distinct sets share a marker.
pub fn change_marker(conn: &Connection, artifact: &str) -> Result<String, Error> {
    let tx = conn.unchecked_transaction().map_err(db_err)?;
    let latest = latest_revision(&tx, artifact)?;
    let newest: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(id), 0) FROM comments WHERE artifact_id = ?1",
            [artifact],
            |r| r.get(0),
        )
        .map_err(db_err)?;
    let resolved: String = tx
        .query_row(
            "SELECT COALESCE(group_concat(id, ','), '') FROM
               (SELECT id FROM comments
                WHERE artifact_id = ?1 AND parent_id IS NULL AND status = 'resolved'
                ORDER BY id)",
            [artifact],
            |r| r.get(0),
        )
        .map_err(db_err)?;
    Ok(format!("{newest}-{resolved}-{latest}"))
}

fn latest_revision(conn: &Connection, artifact: &str) -> Result<i64, Error> {
    conn.query_row(
        "SELECT MAX(number) FROM revisions WHERE artifact_id = ?1",
        [artifact],
        |r| r.get::<_, Option<i64>>(0),
    )
    .map_err(db_err)?
    .ok_or_else(|| Error::NotFound(format!("no artifact {artifact}")))
}

/// Add a root Comment (new Thread) or, with `reply`, a reply in a Thread.
pub fn add(
    conn: &mut Connection,
    ctx: &Context,
    target: &str,
    body: &str,
    locator: &Locator,
    reply: Option<i64>,
) -> Result<Added, Error> {
    if body.trim().is_empty() {
        return Err(Error::Usage("comment body is empty".into()));
    }
    limits::check_comment(body.len() as u64)?;
    let (id, rev_opt) = parse_target(target)?;
    write_tx(conn, |tx| {
        let (revision, stored, parent_id) = match reply {
            Some(parent) => {
                if locator.lines.is_some() || locator.section.is_some() {
                    return Err(Error::Usage(
                        "a reply takes no --lines or --section; it stays on its Thread's Revision"
                            .into(),
                    ));
                }
                let (p_artifact, p_rev, p_parent): (String, i64, Option<i64>) = tx
                    .query_row(
                        "SELECT artifact_id, revision, parent_id FROM comments WHERE id = ?1",
                        [parent],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()
                    .map_err(db_err)?
                    .ok_or_else(|| Error::NotFound(format!("no comment {parent}")))?;
                if p_parent.is_some() {
                    return Err(Error::Usage(format!(
                        "comment {parent} is a reply; reply to the Thread's root comment"
                    )));
                }
                if p_artifact != id {
                    return Err(Error::Usage(format!(
                        "comment {parent} belongs to artifact {p_artifact}, not {id}"
                    )));
                }
                if let Some(r) = rev_opt
                    && r != p_rev
                {
                    return Err(Error::Usage(format!(
                        "comment {parent} is on revision {p_rev}, not {r}"
                    )));
                }
                (p_rev, None, Some(parent))
            }
            None => {
                let (kind, format): (String, Option<String>) = tx
                    .query_row(
                        "SELECT kind, format FROM artifacts WHERE id = ?1",
                        [&id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()
                    .map_err(db_err)?
                    .ok_or_else(|| Error::NotFound(format!("no artifact {id}")))?;
                let latest = latest_revision(tx, &id)?;
                let revision = rev_opt.unwrap_or(latest);
                let stored = resolve_locator(tx, &id, revision, &kind, format.as_deref(), locator)?;
                (revision, Some(stored), None)
            }
        };
        let new_id = insert_comment(tx, ctx, &id, revision, parent_id, body, stored.as_ref())?;
        let latest = latest_revision(tx, &id)?;
        Ok(Added {
            id: new_id,
            thread_id: parent_id.unwrap_or(new_id),
            artifact_id: id.clone(),
            revision,
            latest_revision: latest,
            stale: revision != latest,
            locator: stored,
            highest_comment_id: highest_comment_id(tx)?,
        })
    })
}

/// Insert one Comment row: a root when `parent_id` is None (open, with its
/// locator), otherwise a reply. Returns the new id.
fn insert_comment(
    tx: &Connection,
    ctx: &Context,
    artifact: &str,
    revision: i64,
    parent_id: Option<i64>,
    body: &str,
    loc: Option<&StoredLocator>,
) -> Result<i64, Error> {
    tx.execute(
        "INSERT INTO comments (artifact_id, revision, parent_id, author, role, session, body,
            created_at, line_start, line_end, quoted_text, section_path, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, strftime('%Y-%m-%dT%H:%M:%fZ','now'),
            ?8, ?9, ?10, ?11, ?12)",
        params![
            artifact,
            revision,
            parent_id,
            ctx.author,
            ctx.role.as_str(),
            ctx.session,
            body,
            loc.and_then(|l| l.line_start),
            loc.and_then(|l| l.line_end),
            loc.and_then(|l| l.quoted_text.as_deref()),
            loc.and_then(|l| l.section_path.as_deref()),
            parent_id.is_none().then_some("open"),
        ],
    )
    .map_err(db_err)?;
    Ok(tx.last_insert_rowid())
}

/// Check that every id names a root Comment of `artifact`, before anything is
/// written. Returns the distinct (root id, its Revision) pairs in given order.
pub(crate) fn check_addressable(
    tx: &Connection,
    artifact: &str,
    ids: &[i64],
) -> Result<Vec<(i64, i64)>, Error> {
    let mut out: Vec<(i64, i64)> = Vec::new();
    for &id in ids {
        if out.iter().any(|(i, _)| *i == id) {
            continue;
        }
        let (c_artifact, rev, parent): (String, i64, Option<i64>) = tx
            .query_row(
                "SELECT artifact_id, revision, parent_id FROM comments WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| Error::NotFound(format!("no comment {id}")))?;
        if parent.is_some() {
            return Err(Error::Usage(format!(
                "comment {id} is a reply; name the Thread's root comment"
            )));
        }
        if c_artifact != artifact {
            return Err(Error::Usage(format!(
                "comment {id} belongs to artifact {c_artifact}, not {artifact}"
            )));
        }
        out.push((id, rev));
    }
    Ok(out)
}

/// Post "addressed in <revision ref>" in each checked Thread as the caller.
/// Replies stay on their Thread's Revision and never change its status.
pub(crate) fn post_addressed(
    tx: &Connection,
    ctx: &Context,
    artifact: &str,
    threads: &[(i64, i64)],
    revision_ref: &str,
) -> Result<(), Error> {
    let body = format!("addressed in {revision_ref}");
    for &(root, rev) in threads {
        insert_comment(tx, ctx, artifact, rev, Some(root), &body, None)?;
    }
    Ok(())
}

/// Delay between database polls in [`wait`].
pub const WAIT_POLL_MS: u64 = 500;

pub enum WaitOutcome {
    /// Something newer than the baseline exists.
    Ready(CommentList),
    /// The deadline passed with nothing new: no Threads, same
    /// `highest_comment_id`.
    TimedOut(CommentList),
}

/// Block until [`list`] has something newer than `q.since`, polling every
/// [`WAIT_POLL_MS`]. Without `since` the baseline is the highest Comment id at
/// entry, so a bare wait means "the next new Comment". Each poll reads its own
/// snapshot, so a writer in another process is seen.
pub fn wait(
    conn: &Connection,
    ctx: &Context,
    q: &ListQuery,
    timeout: Duration,
) -> Result<WaitOutcome, Error> {
    // None when the timeout overflows Instant: wait without a deadline.
    let deadline = Instant::now().checked_add(timeout);
    let baseline = match q.since {
        Some(n) => n,
        None => {
            if let Some(a) = &q.artifact {
                latest_revision(conn, a)?;
            }
            highest_comment_id(conn)?
        }
    };
    let newer = ListQuery {
        artifact: q.artifact.clone(),
        all: q.all,
        since: Some(baseline),
        scope: q.scope.clone(),
    };
    let mut current = list_inner(conn, ctx, &newer, true)?;
    loop {
        if !current.threads.is_empty() {
            return Ok(WaitOutcome::Ready(current));
        }
        let poll = Duration::from_millis(WAIT_POLL_MS);
        let nap = match deadline {
            Some(d) => {
                let now = Instant::now();
                if now >= d {
                    return Ok(WaitOutcome::TimedOut(current));
                }
                (d - now).min(poll)
            }
            None => poll,
        };
        std::thread::sleep(nap);
        current = list_inner(conn, ctx, &newer, true)?;
    }
}

/// Validate a root Comment's locator against its own Revision and compute
/// what is stored: the quoted text and, for a Section, the resolved path.
fn resolve_locator(
    conn: &Connection,
    id: &str,
    revision: i64,
    kind: &str,
    format: Option<&str>,
    locator: &Locator,
) -> Result<StoredLocator, Error> {
    let mut stored = StoredLocator {
        line_start: None,
        line_end: None,
        quoted_text: None,
        section_path: None,
    };
    // The Revision must exist even for a whole-Revision Comment.
    let exists = conn
        .query_row(
            "SELECT 1 FROM revisions WHERE artifact_id = ?1 AND number = ?2",
            params![id, revision],
            |_| Ok(()),
        )
        .optional()
        .map_err(db_err)?
        .is_some();
    if !exists {
        return Err(Error::NotFound(format!("no revision {id}@{revision}")));
    }
    if locator.lines.is_none() && locator.section.is_none() {
        return Ok(stored);
    }
    if locator.lines.is_some() && locator.section.is_some() {
        return Err(Error::Usage(
            "--lines and --section are mutually exclusive; give one locator".into(),
        ));
    }
    let is_markdown = kind == "markdown";
    let is_text = is_markdown || (kind == "diagram" && format == Some("mermaid"));
    if !is_text {
        return Err(Error::Rejected(format!(
            "--lines and --section are for text Artifacts, not {kind}"
        )));
    }
    if locator.section.is_some() && !is_markdown {
        return Err(Error::Rejected(format!(
            "sections are for markdown Artifacts, not {kind}"
        )));
    }
    let bytes = crate::artifact::get(conn, id, Some(revision))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error::Rejected("Artifact is not valid UTF-8".into()))?;
    let lines: Vec<&str> = text.lines().collect();
    if let Some(q) = &locator.section {
        let headings = sections::parse(text);
        let h = sections::resolve(&headings, q)?;
        stored.section_path = Some(h.display_path());
        let line = text[..h.start].matches('\n').count() + 1;
        stored.line_start = Some(line as i64);
        stored.line_end = Some(line as i64);
        stored.quoted_text = lines.get(line - 1).map(|l| l.to_string());
    }
    if let Some((a, b)) = locator.lines {
        if a < 1 || b < a || b as usize > lines.len() {
            return Err(Error::Usage(format!(
                "line range {a}-{b} is outside {id}@{revision}, which has {} lines",
                lines.len()
            )));
        }
        stored.line_start = Some(a);
        stored.line_end = Some(b);
        stored.quoted_text = Some(lines[a as usize - 1..b as usize].join("\n"));
    }
    Ok(stored)
}

/// Set a Thread's flag. Either role may do this; idempotent.
pub fn set_status(
    conn: &mut Connection,
    thread_id: i64,
    status: &str,
) -> Result<ThreadResult, Error> {
    write_tx(conn, |tx| {
        let parent: Option<Option<i64>> = tx
            .query_row(
                "SELECT parent_id FROM comments WHERE id = ?1",
                [thread_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?;
        match parent {
            None => return Err(Error::NotFound(format!("no comment {thread_id}"))),
            Some(Some(_)) => {
                return Err(Error::Usage(format!(
                    "comment {thread_id} is a reply; name the Thread's root comment"
                )));
            }
            Some(None) => {}
        }
        tx.execute(
            "UPDATE comments SET status = ?2 WHERE id = ?1",
            params![thread_id, status],
        )
        .map_err(db_err)?;
        let thread = load_threads(tx, "AND c.id = ?1", params![thread_id])?
            .pop()
            .ok_or_else(|| Error::Unexpected("thread vanished".into()))?;
        Ok(ThreadResult {
            thread,
            highest_comment_id: highest_comment_id(tx)?,
        })
    })
}

/// Threads of the Artifacts in scope. With an explicit Artifact the Session
/// scope is ignored; otherwise an explicit `--session` limits to Artifacts any
/// Revision of which was made in it (so a Session keeps its Threads after
/// another Session edits), and with no explicit Session (the env fallback never
/// filters) the Project scope of `q.scope` applies, the same as `list`.
pub fn list(conn: &Connection, ctx: &Context, q: &ListQuery) -> Result<CommentList, Error> {
    list_inner(conn, ctx, q, false)
}

/// [`list`], optionally not counting the caller's own Comments (same author
/// and Session) as newer than `q.since`. [`wait`] uses this so the replies
/// that `edit --addresses` writes do not wake the editor's own wait.
fn list_inner(
    conn: &Connection,
    ctx: &Context,
    q: &ListQuery,
    exclude_own: bool,
) -> Result<CommentList, Error> {
    // One snapshot so highest_comment_id covers everything the list shows.
    let tx = conn.unchecked_transaction().map_err(db_err)?;
    let highest = highest_comment_id(&tx)?;
    if let Some(a) = &q.artifact {
        latest_revision(&tx, a)?;
    }
    let session = if q.artifact.is_some() {
        None
    } else {
        ctx.session_explicit.clone()
    };
    // An explicit Artifact or Session ignores the Project.
    let scope = if session.is_some() || q.artifact.is_some() {
        ScopeFilter::unscoped()
    } else {
        q.scope.clone()
    };
    let [sn, sr, sa, sw] = scope.params();
    let filter = format!(
        "AND (?1 IS NULL OR c.artifact_id = ?1)
         AND (?2 IS NULL OR EXISTS (SELECT 1 FROM revisions r
              WHERE r.artifact_id = c.artifact_id
                AND r.session = ?2))
         AND (?3 = 1 OR c.status = 'open')
         AND (?4 IS NULL
              OR (c.id > ?4 AND NOT (?9 = 1 AND c.author = ?10 AND c.session IS ?11))
              OR EXISTS (SELECT 1 FROM comments x WHERE x.parent_id = c.id AND x.id > ?4
                         AND NOT (?9 = 1 AND x.author = ?10 AND x.session IS ?11)))
         AND EXISTS (SELECT 1 FROM artifacts a WHERE a.id = c.artifact_id AND {})",
        scope.sql("a", 5)
    );
    let threads = load_threads(
        &tx,
        &filter,
        params![
            q.artifact,
            session,
            q.all as i64,
            q.since,
            sn,
            sr,
            sa,
            sw,
            exclude_own as i64,
            ctx.author,
            ctx.session
        ],
    )?;
    Ok(CommentList {
        threads,
        highest_comment_id: highest,
    })
}

/// Load root Comments matching `filter` (appended to the root predicate, with
/// its own numbered parameters) with their replies, oldest first.
fn load_threads(
    conn: &Connection,
    filter: &str,
    args: impl rusqlite::Params,
) -> Result<Vec<Thread>, Error> {
    let sql = format!(
        "SELECT c.id, c.artifact_id, c.revision,
                (SELECT MAX(number) FROM revisions WHERE artifact_id = c.artifact_id),
                c.status, c.author, c.role, c.session, c.body, c.created_at,
                c.line_start, c.line_end, c.quoted_text, c.section_path
         FROM comments c WHERE c.parent_id IS NULL {filter} ORDER BY c.id"
    );
    let mut stmt = conn.prepare(&sql).map_err(db_err)?;
    let mut threads = stmt
        .query_map(args, |r| {
            let revision: i64 = r.get(2)?;
            let latest: i64 = r.get(3)?;
            Ok(Thread {
                id: r.get(0)?,
                artifact_id: r.get(1)?,
                revision,
                latest_revision: latest,
                stale: revision != latest,
                status: r.get(4)?,
                author: r.get(5)?,
                role: r.get(6)?,
                session: r.get(7)?,
                body: r.get(8)?,
                created_at: r.get(9)?,
                locator: StoredLocator {
                    line_start: r.get(10)?,
                    line_end: r.get(11)?,
                    quoted_text: r.get(12)?,
                    section_path: r.get(13)?,
                },
                replies: Vec::new(),
            })
        })
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    let mut rstmt = conn
        .prepare(
            "SELECT id, author, role, session, body, created_at FROM comments
             WHERE parent_id = ?1 ORDER BY id",
        )
        .map_err(db_err)?;
    for t in &mut threads {
        t.replies = rstmt
            .query_map([t.id], |r| {
                Ok(Reply {
                    id: r.get(0)?,
                    author: r.get(1)?,
                    role: r.get(2)?,
                    session: r.get(3)?,
                    body: r.get(4)?,
                    created_at: r.get(5)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
    }
    Ok(threads)
}

#[cfg(test)]
mod marker_tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        for a in ["A", "B"] {
            conn.execute(
                "INSERT INTO revisions (artifact_id, number, author, role, created_at)
                 VALUES (?1, 1, 'x', 'agent', '')",
                [a],
            )
            .unwrap();
        }
        (dir, conn)
    }

    fn root(conn: &Connection, artifact: &str, status: &str) -> i64 {
        conn.execute(
            "INSERT INTO comments (artifact_id, revision, author, role, body, created_at, status)
             VALUES (?1, 1, 'x', 'agent', 'b', '', ?2)",
            rusqlite::params![artifact, status],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn set(conn: &Connection, id: i64, status: &str) {
        conn.execute(
            "UPDATE comments SET status = ?1 WHERE id = ?2",
            rusqlite::params![status, id],
        )
        .unwrap();
    }

    #[test]
    fn other_artifact_does_not_change_marker() {
        let (_d, c) = setup();
        let before = change_marker(&c, "A").unwrap();
        root(&c, "B", "open");
        assert_eq!(change_marker(&c, "A").unwrap(), before);
    }

    #[test]
    fn swapping_resolved_sets_does_not_collide() {
        let (_d, c) = setup();
        let (t1, t2, t3) = (
            root(&c, "A", "resolved"),
            root(&c, "A", "resolved"),
            root(&c, "A", "open"),
        );
        let before = change_marker(&c, "A").unwrap();
        set(&c, t1, "open");
        set(&c, t2, "open");
        set(&c, t3, "resolved");
        assert_ne!(change_marker(&c, "A").unwrap(), before);
    }

    #[test]
    fn changes_on_comment_resolve_reopen_and_revision() {
        let (_d, c) = setup();
        let m0 = change_marker(&c, "A").unwrap();
        let t = root(&c, "A", "open");
        let m1 = change_marker(&c, "A").unwrap();
        set(&c, t, "resolved");
        let m2 = change_marker(&c, "A").unwrap();
        set(&c, t, "open");
        let m3 = change_marker(&c, "A").unwrap();
        c.execute(
            "INSERT INTO revisions (artifact_id, number, author, role, created_at)
             VALUES ('A', 2, 'x', 'agent', '')",
            [],
        )
        .unwrap();
        let m4 = change_marker(&c, "A").unwrap();
        let all = [m0, m1, m2, m3, m4];
        for i in 0..all.len() {
            for j in i + 1..all.len() {
                if !(i == 1 && j == 3) {
                    assert_ne!(all[i], all[j], "{i} vs {j}");
                }
            }
        }
    }
}
