//! Markdown embeds: `![alt](cairn:ID)` / `![alt](cairn:ID@N)` show an image or
//! diagram Artifact inside a markdown Artifact.
//!
//! Each reference is validated and resolved when a markdown Revision is
//! written, and recorded in the `embeds` table. The stored markdown text is
//! never rewritten. The table deliberately has no foreign key on the target:
//! deleting an embedded Artifact is allowed, and the dangling row lets the UI
//! show a placeholder. A reference the previous Revision already recorded is
//! carried forward if its target has since gone, so it cannot block later
//! edits; new references are always validated. Only image syntax counts; other URL schemes are ignored.

use pulldown_cmark::{Event, Options, Parser, Tag};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;

use crate::artifact::parse_target;
use crate::db::db_err;
use crate::error::Error;

const SCHEME: &str = "cairn:";

/// One reference found in markdown text, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    pub target_id: String,
    /// `Some(n)` when the reference is pinned with `@n`.
    pub pinned: Option<i64>,
}

/// One recorded embed of a Revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Embed {
    pub ordinal: i64,
    pub target_id: String,
    /// The Revision the reference resolved to when the owner was written.
    pub target_revision: i64,
    /// Whether the reference named its Revision (`id@N`) rather than latest.
    pub pinned: bool,
}

/// The `cairn:` image references of `text`, in document order. A parser (not
/// a regex) so code spans, fences and reference-style images behave.
pub fn extract(text: &str) -> Result<Vec<Ref>, Error> {
    let mut out = Vec::new();
    for ev in Parser::new_ext(text, Options::all()) {
        if let Event::Start(Tag::Image { dest_url, .. }) = ev
            && let Some(rest) = dest_url.strip_prefix(SCHEME)
        {
            // Malformed content is rejected input, not a usage error.
            let (target_id, pinned) = parse_target(rest)
                .map_err(|e| Error::Rejected(format!("embed {dest_url}: {}", e)))?;
            out.push(Ref { target_id, pinned });
        }
    }
    Ok(out)
}

/// Validate and record the embeds of markdown Revision `owner_id@owner_rev`.
/// Runs inside the writer's transaction, so a rejection rolls the Revision back.
///
/// `prev_rev` is the owner's previous Revision. A reference that Revision
/// already recorded (same target, same pinned-ness, and for a pinned one the
/// same `@N`) is carried forward when its target or pinned Revision has since
/// disappeared, so a dangling embed never blocks an unrelated edit. New or
/// changed references are always fully validated.
pub fn record(
    tx: &Transaction,
    owner_id: &str,
    owner_rev: i64,
    prev_rev: Option<i64>,
    text: &str,
) -> Result<(), Error> {
    let prior = match prev_rev {
        Some(p) => for_revision(tx, owner_id, p)?,
        None => Vec::new(),
    };
    for (ordinal, r) in extract(text)?.into_iter().enumerate() {
        let shown = match r.pinned {
            Some(n) => format!("cairn:{}@{n}", r.target_id),
            None => format!("cairn:{}", r.target_id),
        };
        let carried = prior.iter().find(|e| {
            e.target_id == r.target_id
                && e.pinned == r.pinned.is_some()
                && r.pinned.is_none_or(|n| e.target_revision == n)
        });
        let kind: Option<String> = tx
            .query_row(
                "SELECT kind FROM artifacts WHERE id = ?1",
                [&r.target_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_err)?;
        let exists = kind.is_some();
        let resolved: Option<i64> = match kind {
            None => None,
            Some(kind) => {
                if kind != "image" && kind != "diagram" {
                    return Err(Error::Rejected(format!(
                        "embed {shown} is a {kind} Artifact; only image and diagram can be embedded"
                    )));
                }
                match r.pinned {
                    Some(n) => tx
                        .query_row(
                            "SELECT number FROM revisions WHERE artifact_id = ?1 AND number = ?2",
                            params![r.target_id, n],
                            |row| row.get(0),
                        )
                        .optional()
                        .map_err(db_err)?,
                    None => tx
                        .query_row(
                            "SELECT MAX(number) FROM revisions WHERE artifact_id = ?1",
                            [&r.target_id],
                            |row| row.get(0),
                        )
                        .map_err(db_err)?,
                }
            }
        };
        let revision = match (resolved, carried) {
            (Some(n), _) => n,
            (None, Some(c)) => c.target_revision,
            (None, None) => {
                return Err(Error::Rejected(match r.pinned {
                    Some(n) if exists => format!("embed {shown}: no revision {n}"),
                    _ => format!("embed {shown}: no such artifact"),
                }));
            }
        };
        tx.execute(
            "INSERT INTO embeds (artifact_id, revision, ordinal, target_id, target_revision, pinned)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                owner_id,
                owner_rev,
                ordinal as i64,
                r.target_id,
                revision,
                r.pinned.is_some()
            ],
        )
        .map_err(db_err)?;
    }
    Ok(())
}

/// The embeds recorded for a Revision, in document order. Does not join on
/// `artifacts`, so embeds of deleted targets still list.
pub fn for_revision(conn: &Connection, id: &str, revision: i64) -> Result<Vec<Embed>, Error> {
    let mut stmt = conn
        .prepare(
            "SELECT ordinal, target_id, target_revision, pinned FROM embeds
             WHERE artifact_id = ?1 AND revision = ?2 ORDER BY ordinal",
        )
        .map_err(db_err)?;
    stmt.query_map(params![id, revision], |r| {
        Ok(Embed {
            ordinal: r.get(0)?,
            target_id: r.get(1)?,
            target_revision: r.get(2)?,
            pinned: r.get(3)?,
        })
    })
    .map_err(db_err)?
    .collect::<Result<Vec<_>, _>>()
    .map_err(db_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(id: &str, p: Option<i64>) -> Ref {
        Ref {
            target_id: id.into(),
            pinned: p,
        }
    }

    #[test]
    fn latest_and_pinned() {
        let t = "![a](cairn:abc)\n\ntext ![b](cairn:def@2)";
        assert_eq!(extract(t).unwrap(), vec![r("abc", None), r("def", Some(2))]);
    }

    #[test]
    fn ignored_forms() {
        let t = "`![a](cairn:abc)`\n\n```\n![a](cairn:abc)\n```\n\n\
                 ![h](http://x/y.png) ![d](data:image/png;base64,AA) ![r](rel.png)\n\n\
                 [link](cairn:abc)";
        assert_eq!(extract(t).unwrap(), vec![]);
    }

    #[test]
    fn reference_style() {
        let t = "![a][x]\n\n[x]: cairn:abc@3";
        assert_eq!(extract(t).unwrap(), vec![r("abc", Some(3))]);
    }

    #[test]
    fn malformed_is_rejected() {
        assert!(matches!(extract("![a](cairn:)"), Err(Error::Rejected(_))));
        assert!(matches!(
            extract("![a](cairn:abc@x)"),
            Err(Error::Rejected(_))
        ));
    }
}
