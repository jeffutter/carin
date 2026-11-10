//! Markdown Sections: heading paths, outline, extract and splice.
//!
//! Pure functions over `&str`; nothing here touches the database. A Section
//! is a byte range of the stored text, from a top-level heading to the next
//! heading of the same or a higher level (so it includes its children).
//! Front matter and text before the first heading belong to no Section.
//!
//! Line endings: ranges are byte offsets into the original text and are never
//! rewritten, so CRLF documents parse and slice exactly like LF ones. The
//! one adjustment is in [`splice`]: a replacement is inserted verbatim,
//! except that when the replaced Section uses CRLF the replacement's bare LF
//! are converted to CRLF so one Revision never mixes endings.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::Serialize;

use crate::error::Error;

/// A top-level heading and the Section it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    /// Heading text with inline markup stripped and trimmed.
    pub text: String,
    /// Texts of the ancestors and this heading, outermost first.
    pub path: Vec<String>,
    /// 1-based position among the headings this full path matches as a suffix.
    pub occurrence: usize,
    /// Whether another heading also matches this full path as a suffix.
    pub repeated: bool,
    /// Byte range of the Section in the source.
    pub start: usize,
    pub end: usize,
}

impl Heading {
    /// The path as printed by `outline`: `A > B`, with `#n` on the last
    /// segment when the full path repeats, so it can be used as an address.
    pub fn display_path(&self) -> String {
        let mut p = self.path.join(" > ");
        if self.repeated {
            p.push_str(&format!("#{}", self.occurrence));
        }
        p
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct OutlineEntry {
    pub path: String,
    pub level: u8,
    /// Byte length of the Section, children included.
    pub bytes: usize,
}

fn level_num(l: HeadingLevel) -> u8 {
    match l {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Every top-level heading of `src`, in document order.
pub fn parse(src: &str) -> Vec<Heading> {
    let opts = Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut out: Vec<Heading> = Vec::new();
    // Containers a heading may sit inside; headings there are not boundaries.
    let mut depth = 0usize;
    // (level, start, text) of the heading being read, if it is top-level.
    let mut open: Option<(u8, usize, String)> = None;
    for (ev, range) in Parser::new_ext(src, opts).into_offset_iter() {
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                if depth == 0 {
                    open = Some((level_num(level), range.start, String::new()));
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, start, text)) = open.take() {
                    out.push(Heading {
                        level,
                        text: text.trim().to_string(),
                        path: Vec::new(),
                        occurrence: 1,
                        repeated: false,
                        start,
                        end: src.len(),
                    });
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, _, text)) = open.as_mut() {
                    text.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, _, text)) = open.as_mut() {
                    text.push(' ');
                }
            }
            Event::Start(
                Tag::BlockQuote(_) | Tag::List(_) | Tag::Item | Tag::FootnoteDefinition(_),
            ) => {
                depth += 1;
            }
            Event::End(
                TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item | TagEnd::FootnoteDefinition,
            ) => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    // Section ends and paths.
    let mut stack: Vec<(u8, String)> = Vec::new();
    for i in 0..out.len() {
        let level = out[i].level;
        out[i].end = out[i + 1..]
            .iter()
            .find(|h| h.level <= level)
            .map_or(src.len(), |h| h.start);
        while stack.last().is_some_and(|(l, _)| *l >= level) {
            stack.pop();
        }
        stack.push((level, out[i].text.clone()));
        out[i].path = stack.iter().map(|(_, t)| t.clone()).collect();
    }
    // Occurrence is counted among the headings the full path would match as
    // a suffix, so the printed `path#n` always resolves back to this heading.
    for i in 0..out.len() {
        let same: Vec<usize> = (0..out.len())
            .filter(|&j| out[j].path.ends_with(&out[i].path))
            .collect();
        out[i].repeated = same.len() > 1;
        out[i].occurrence = same.iter().position(|&j| j == i).unwrap_or(0) + 1;
    }
    out
}

pub fn outline(src: &str) -> Vec<OutlineEntry> {
    parse(src)
        .iter()
        .map(|h| OutlineEntry {
            path: h.display_path(),
            level: h.level,
            bytes: h.end - h.start,
        })
        .collect()
}

/// Find the one heading a query names. Never guesses.
///
/// The query is `A > B` with an optional `#n` on the last segment. Candidates
/// are the headings whose path ends with the query's segments; `#n` picks the
/// n-th candidate in document order. More than one candidate without `#n` is
/// an error, even when one of them equals the query exactly.
pub fn resolve<'a>(headings: &'a [Heading], query: &str) -> Result<&'a Heading, Error> {
    let mut segs: Vec<String> = query.split('>').map(|s| s.trim().to_string()).collect();
    let mut nth: Option<usize> = None;
    if let Some(last) = segs.last_mut()
        && let Some((name, n)) = last.rsplit_once('#')
        && let Ok(n) = n.parse::<usize>()
        && n >= 1
    {
        nth = Some(n);
        *last = name.trim_end().to_string();
    }
    if segs.iter().any(|s| s.is_empty()) {
        return Err(Error::Rejected(format!(
            "empty segment in section path {query:?}"
        )));
    }

    let candidates: Vec<&Heading> = headings
        .iter()
        .filter(|h| h.path.ends_with(&segs))
        .collect();

    let base = segs.join(" > ");
    match (candidates.as_slice(), nth) {
        ([], _) => {
            let avail: Vec<String> = headings.iter().map(Heading::display_path).collect();
            Err(Error::Rejected(format!(
                "no section matches {query:?}; available: {}",
                if avail.is_empty() {
                    "(none)".to_string()
                } else {
                    avail.join("; ")
                }
            )))
        }
        (_, Some(n)) => candidates.get(n - 1).copied().ok_or_else(|| {
            Error::Rejected(format!(
                "section {base:?} has {} match(es), no occurrence #{n}",
                candidates.len()
            ))
        }),
        ([one], None) => Ok(one),
        (many, None) => {
            let list: Vec<String> = many
                .iter()
                .enumerate()
                .map(|(i, h)| format!("{base}#{} = {}", i + 1, h.display_path()))
                .collect();
            Err(Error::Rejected(format!(
                "ambiguous section {query:?}; candidates: {}",
                list.join("; ")
            )))
        }
    }
}

/// The text of one Section, children included.
pub fn extract<'a>(src: &'a str, query: &str) -> Result<&'a str, Error> {
    let hs = parse(src);
    let h = resolve(&hs, query)?;
    Ok(&src[h.start..h.end])
}

/// Replace one Section with `replacement`, which must begin with a heading
/// line. Text outside the Section is untouched.
pub fn splice(src: &str, query: &str, replacement: &str) -> Result<String, Error> {
    let hs = parse(src);
    let h = resolve(&hs, query)?;
    if parse(replacement).first().map(|r| r.start) != Some(0) {
        return Err(Error::Rejected(
            "a section replacement must start with a heading line".into(),
        ));
    }
    let old = &src[h.start..h.end];
    let crlf = old
        .find('\n')
        .is_some_and(|i| i > 0 && old.as_bytes()[i - 1] == b'\r');
    let mut new = if crlf {
        replacement.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        replacement.to_string()
    };
    let tail = &src[h.end..];
    if !tail.is_empty() && !new.ends_with('\n') {
        new.push_str(if crlf { "\r\n" } else { "\n" });
    }
    Ok(format!("{}{new}{tail}", &src[..h.start]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# Title\n\nintro\n\n## A\n\na body\n\n### A1\n\ndeep\n\n## B\n\nb body\n";

    fn paths(src: &str) -> Vec<String> {
        outline(src).into_iter().map(|e| e.path).collect()
    }

    // CRLF decision: pulldown-cmark offsets are byte offsets into the
    // original text and heading text excludes the CR, so CRLF parses like LF.
    #[test]
    fn crlf_parses_like_lf_with_exact_ranges() {
        let crlf = DOC.replace('\n', "\r\n");
        let lf = parse(DOC);
        let cr = parse(&crlf);
        assert_eq!(lf.len(), cr.len());
        for (a, b) in lf.iter().zip(&cr) {
            assert_eq!((a.level, &a.text, &a.path), (b.level, &b.text, &b.path));
        }
        for h in &cr {
            assert!(crlf[h.start..].starts_with('#'));
            assert!(h.end == crlf.len() || crlf[h.end..].starts_with('#'));
        }
        assert_eq!(extract(&crlf, "A1").unwrap(), "### A1\r\n\r\ndeep\r\n\r\n");
    }

    #[test]
    fn crlf_splice_never_mixes_endings() {
        let crlf = DOC.replace('\n', "\r\n");
        let out = splice(&crlf, "A > A1", "### A1\nnew\n").unwrap();
        assert_eq!(
            out,
            "# Title\r\n\r\nintro\r\n\r\n## A\r\n\r\na body\r\n\r\n### A1\r\nnew\r\n## B\r\n\r\nb body\r\n"
        );
        assert!(!out.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn lf_splice_is_verbatim_and_keeps_the_rest() {
        let out = splice(DOC, "B", "## B2\nx").unwrap();
        assert_eq!(
            out,
            format!("{}## B2\nx", &DOC[..DOC.find("## B").unwrap()])
        );
        let out = splice(DOC, "A", "## A\n\nnew\n\n").unwrap();
        assert_eq!(out, "# Title\n\nintro\n\n## A\n\nnew\n\n## B\n\nb body\n");
    }

    #[test]
    fn splice_adds_newline_before_following_heading() {
        let out = splice(DOC, "A", "## A\nnew").unwrap();
        assert_eq!(out, "# Title\n\nintro\n\n## A\nnew\n## B\n\nb body\n");
    }

    #[test]
    fn replacement_must_start_with_heading() {
        for bad in ["text\n", "\n## A\n", "", "---\na: 1\n---\n## A\n"] {
            assert!(
                matches!(splice(DOC, "A", bad), Err(Error::Rejected(_))),
                "{bad:?}"
            );
        }
        splice(DOC, "A", "Setext\n------\nbody\n").unwrap();
    }

    #[test]
    fn outline_has_paths_levels_and_sizes() {
        let o = outline(DOC);
        assert_eq!(
            paths(DOC),
            ["Title", "Title > A", "Title > A > A1", "Title > B"]
        );
        assert_eq!(o[0].bytes, DOC.len());
        assert_eq!(o[1].level, 2);
        assert_eq!(o[2].bytes, "### A1\n\ndeep\n\n".len());
    }

    #[test]
    fn suffix_matching() {
        let hs = parse(DOC);
        assert_eq!(resolve(&hs, "A1").unwrap().text, "A1");
        assert_eq!(resolve(&hs, "A > A1").unwrap().text, "A1");
        assert_eq!(resolve(&hs, " Title > A ").unwrap().text, "A");
        assert!(resolve(&hs, "a1").is_err(), "case-sensitive");
        assert!(
            resolve(&hs, "Title > A1").is_err(),
            "not a contiguous suffix"
        );
    }

    #[test]
    fn duplicates_need_an_occurrence() {
        let src = "# X\n\n## Notes\n\none\n\n# Y\n\n## Notes\n\ntwo\n\n# Z\n\n# Z\n\nz2\n";
        let hs = parse(src);
        let err = resolve(&hs, "Notes").unwrap_err().to_string();
        assert!(err.contains("ambiguous"), "{err}");
        assert!(
            err.contains("Notes#1 = X > Notes") && err.contains("Notes#2 = Y > Notes"),
            "{err}"
        );
        assert_eq!(extract(src, "Notes#2").unwrap(), "## Notes\n\ntwo\n\n");
        assert_eq!(extract(src, "Y > Notes").unwrap(), "## Notes\n\ntwo\n\n");
        assert!(resolve(&hs, "Notes#3").is_err());
        // Printed paths are directly usable addresses.
        assert_eq!(paths(src)[4], "Z#1");
        assert_eq!(extract(src, "Z#2").unwrap(), "# Z\n\nz2\n");
        assert!(
            resolve(&hs, "Z")
                .unwrap_err()
                .to_string()
                .contains("ambiguous")
        );
    }

    #[test]
    fn missing_path_lists_available() {
        let err = resolve(&parse(DOC), "Nope").unwrap_err().to_string();
        assert!(
            err.contains("no section") && err.contains("Title > A"),
            "{err}"
        );
    }

    #[test]
    fn inline_markup_is_stripped() {
        let src = "# *Em* `code` [link](http://x) **b**\n\ntext\n";
        assert_eq!(paths(src), ["Em code link b"]);
    }

    #[test]
    fn nested_headings_are_not_boundaries() {
        let src = "# Top\n\n> ## Quoted\n\n- ## Listed\n\n- item\n\n## Real\n\nx\n";
        assert_eq!(paths(src), ["Top", "Top > Real"]);
    }

    #[test]
    fn setext_and_front_matter() {
        let src = "---\ntitle: t\n---\n\npre\n\nTop\n===\n\nbody\n\nSub\n---\n\nsub body\n";
        assert_eq!(paths(src), ["Top", "Top > Sub"]);
        let out = splice(src, "Sub", "## Sub2\n\nnew\n").unwrap();
        assert!(out.starts_with("---\ntitle: t\n---\n\npre\n\nTop\n===\n\nbody\n\n## Sub2"));
    }

    #[test]
    fn level_gaps_nest_under_nearest_lower() {
        let src = "# A\n\n### C\n\n## B\n";
        assert_eq!(paths(src), ["A", "A > C", "A > B"]);
    }

    #[test]
    fn no_headings() {
        assert!(outline("just text\n").is_empty());
    }
}
