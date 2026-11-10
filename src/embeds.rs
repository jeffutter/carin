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

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;

use crate::artifact::parse_target;
use crate::db::db_err;
use crate::error::Error;

pub(crate) const SCHEME: &str = "cairn:";

/// One reference found in markdown text, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    pub target_id: String,
    /// `Some(n)` when the reference is pinned with `@n`.
    pub pinned: Option<i64>,
    /// The image's alt text (plain text of its label).
    pub alt: String,
    /// 1-based line of the reference in the source text.
    pub line: usize,
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
    // Alt text accumulates between an image's Start and End events. Images can
    // nest inside labels, so each open image has its own buffer.
    let mut open: Vec<Option<Ref>> = Vec::new();
    let mut alts: Vec<String> = Vec::new();
    for (ev, range) in Parser::new_ext(text, Options::all()).into_offset_iter() {
        match ev {
            Event::Start(Tag::Image { dest_url, .. }) => {
                let mut found = None;
                if let Some(rest) = dest_url.strip_prefix(SCHEME) {
                    // Malformed content is rejected input, not a usage error.
                    let (target_id, pinned) = parse_target(rest)
                        .map_err(|e| Error::Rejected(format!("embed {dest_url}: {}", e)))?;
                    let line = 1 + text[..range.start].bytes().filter(|&b| b == b'\n').count();
                    found = Some(Ref {
                        target_id,
                        pinned,
                        alt: String::new(),
                        line,
                    });
                }
                open.push(found);
                alts.push(String::new());
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some(a) = alts.last_mut() {
                    a.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(a) = alts.last_mut() {
                    a.push(' ');
                }
            }
            Event::End(TagEnd::Image) => {
                let alt = alts.pop().unwrap_or_default();
                if let Some(mut r) = open.pop().flatten() {
                    r.alt = alt;
                    out.push(r);
                }
            }
            _ => {}
        }
    }
    // Nested images finish inner-first; document order is by start.
    out.sort_by_key(|r| r.line);
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

/// One `cairn:` image reference of a markdown Revision, resolved at call time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct ResolvedEmbed {
    pub ordinal: i64,
    /// 1-based line of the reference in the stored markdown.
    pub line: usize,
    pub alt: String,
    pub target_id: String,
    /// Whether the reference named its Revision (`id@N`) rather than latest.
    pub pinned: bool,
    /// The pinned N, or the target's current latest Revision for an unpinned
    /// reference (the rule the web UI uses). Null when dangling.
    pub target_revision: Option<i64>,
    /// The target's Kind; null when dangling.
    pub kind: Option<String>,
    /// The target's diagram format; null unless it is a diagram.
    pub format: Option<String>,
    /// The target Artifact or pinned Revision no longer exists.
    pub dangling: bool,
}

/// The `cairn:` image references of markdown `text`, resolved against the
/// database now. Unlike [`for_revision`] this reads the text, so it reports
/// alt and line, and an unpinned reference follows the target's latest.
pub fn resolve(conn: &Connection, text: &str) -> Result<Vec<ResolvedEmbed>, Error> {
    let mut out = Vec::new();
    for (ordinal, r) in extract(text)?.into_iter().enumerate() {
        let target: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT kind, format FROM artifacts WHERE id = ?1",
                [&r.target_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(db_err)?;
        let revision: Option<i64> = match (&target, r.pinned) {
            (None, _) => None,
            (Some(_), Some(n)) => conn
                .query_row(
                    "SELECT number FROM revisions WHERE artifact_id = ?1 AND number = ?2",
                    params![r.target_id, n],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_err)?,
            (Some(_), None) => conn
                .query_row(
                    "SELECT MAX(number) FROM revisions WHERE artifact_id = ?1",
                    [&r.target_id],
                    |row| row.get(0),
                )
                .map_err(db_err)?,
        };
        let (kind, format) = match (&target, revision) {
            (Some((k, f)), Some(_)) => (Some(k.clone()), f.clone()),
            _ => (None, None),
        };
        out.push(ResolvedEmbed {
            ordinal: ordinal as i64,
            line: r.line,
            alt: r.alt,
            target_id: r.target_id,
            pinned: r.pinned.is_some(),
            target_revision: revision,
            kind,
            format,
            dangling: revision.is_none(),
        });
    }
    Ok(out)
}

/// What `embeds` prints on the CLI and `GET /api/v1/artifacts/{id}/embeds` returns.
#[derive(Debug, Serialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct EmbedList {
    pub embeds: Vec<ResolvedEmbed>,
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
            alt: String::new(),
            line: 0,
        }
    }

    /// Compare ignoring alt and line, which have their own test.
    fn bare(t: &str) -> Vec<Ref> {
        extract(t)
            .unwrap()
            .into_iter()
            .map(|mut x| {
                x.alt.clear();
                x.line = 0;
                x
            })
            .collect()
    }

    #[test]
    fn alt_and_line() {
        let t =
            "# T\n\nsee ![a *b*](cairn:abc) and\n![](cairn:def@2)\n\n```\n![x](cairn:zzz)\n```\n";
        let got = extract(t).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!((got[0].alt.as_str(), got[0].line), ("a b", 3));
        assert_eq!((got[1].alt.as_str(), got[1].line), ("", 4));
    }

    #[test]
    fn latest_and_pinned() {
        let t = "![a](cairn:abc)\n\ntext ![b](cairn:def@2)";
        assert_eq!(bare(t), vec![r("abc", None), r("def", Some(2))]);
    }

    #[test]
    fn ignored_forms() {
        let t = "`![a](cairn:abc)`\n\n```\n![a](cairn:abc)\n```\n\n\
                 ![h](http://x/y.png) ![d](data:image/png;base64,AA) ![r](rel.png)\n\n\
                 [link](cairn:abc)";
        assert_eq!(bare(t), vec![]);
    }

    #[test]
    fn reference_style() {
        let t = "![a][x]\n\n[x]: cairn:abc@3";
        assert_eq!(bare(t), vec![r("abc", Some(3))]);
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
