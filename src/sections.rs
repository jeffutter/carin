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
        let mut p = self
            .path
            .iter()
            .map(|t| escape_segment(t))
            .collect::<Vec<_>>()
            .join(" > ");
        if self.repeated {
            p.push_str(&format!("#{}", self.occurrence));
        }
        p
    }
}

/// Escape one heading text for use as a path segment: `\\` and `>` always,
/// and `#` when what follows is all digits (otherwise it would read as an
/// occurrence suffix). A plain `C#` stays as is.
fn escape_segment(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let rest = &chars[i + 1..];
        let digits_follow = !rest.is_empty() && rest.iter().all(char::is_ascii_digit);
        if c == '\\' || c == '>' || (c == '#' && digits_follow) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Split a query into segments of `(char, escaped)`. A backslash makes the
/// next char literal (a trailing lone backslash is literal); an unescaped `>`
/// ends a segment. Unescaped edge whitespace is trimmed.
fn split_query(query: &str) -> Vec<Vec<(char, bool)>> {
    let mut segs: Vec<Vec<(char, bool)>> = vec![Vec::new()];
    let mut it = query.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            let lit = it.next().unwrap_or('\\');
            segs.last_mut().unwrap().push((lit, true));
        } else if c == '>' {
            segs.push(Vec::new());
        } else {
            segs.last_mut().unwrap().push((c, false));
        }
    }
    for seg in &mut segs {
        while seg.last().is_some_and(|&(c, e)| !e && c.is_whitespace()) {
            seg.pop();
        }
        let lead = seg
            .iter()
            .take_while(|&&(c, e)| !e && c.is_whitespace())
            .count();
        seg.drain(..lead);
    }
    segs
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
    let mut raw = split_query(query);
    let mut nth: Option<usize> = None;
    if let Some(last) = raw.last_mut()
        && let Some(pos) = last.iter().rposition(|&(c, e)| c == '#' && !e)
        && !last[pos + 1..].is_empty()
        && last[pos + 1..]
            .iter()
            .all(|&(c, e)| !e && c.is_ascii_digit())
        && let Ok(n) = last[pos + 1..]
            .iter()
            .map(|&(c, _)| c)
            .collect::<String>()
            .parse::<usize>()
        && n >= 1
    {
        nth = Some(n);
        last.truncate(pos);
        while last.last().is_some_and(|&(c, e)| !e && c.is_whitespace()) {
            last.pop();
        }
    }
    let segs: Vec<String> = raw
        .iter()
        .map(|s| s.iter().map(|&(c, _)| c).collect())
        .collect();
    if segs.iter().any(|s| s.is_empty()) {
        return Err(Error::Rejected(format!(
            "empty segment in section path {query:?}"
        )));
    }

    let candidates: Vec<&Heading> = headings
        .iter()
        .filter(|h| h.path.ends_with(&segs))
        .collect();

    let base = segs
        .iter()
        .map(|t| escape_segment(t))
        .collect::<Vec<_>>()
        .join(" > ");
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
/// line of the same level as the replaced one, and contain no heading
/// shallower than it. The result must also keep the headings after the
/// Section unchanged, which rejects an unclosed code fence swallowing the
/// tail. Text outside the Section is untouched.
pub fn splice(src: &str, query: &str, replacement: &str) -> Result<String, Error> {
    let hs = parse(src);
    let h = resolve(&hs, query)?;
    if parse(replacement).first().map(|r| r.start) != Some(0) {
        return Err(Error::Rejected(
            "a section replacement must start with a heading line".into(),
        ));
    }
    let new_hs = parse(replacement);
    if let Some(bad) = new_hs
        .iter()
        .find(|r| r.level != h.level && (r.start == 0 || r.level < h.level))
    {
        return Err(Error::Rejected(format!(
            "a section replacement must start with a level-{} heading and contain none shallower; found level {}",
            h.level, bad.level
        )));
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
    let out = format!("{}{new}{tail}", &src[..h.start]);
    let split = h.start + new.len();
    let got: Vec<(u8, String)> = parse(&out)
        .into_iter()
        .filter(|x| x.start >= split)
        .map(|x| (x.level, x.text))
        .collect();
    let want: Vec<(u8, String)> = parse(tail).into_iter().map(|x| (x.level, x.text)).collect();
    if got != want {
        return Err(Error::Rejected(
            "the replacement changes the structure of the rest of the document (for example an unclosed code fence)".into(),
        ));
    }
    Ok(out)
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
    fn splice_rejects_level_change_and_shallower_heading() {
        assert!(splice(DOC, "A", "### A\nx\n").is_err());
        assert!(splice(DOC, "A", "# A\nx\n").is_err());
        assert!(splice(DOC, "A", "## A\nx\n# Up\ny\n").is_err());
        assert!(splice(DOC, "A", "## A\nx\n").is_ok());
        assert!(splice(DOC, "A", "## A\nx\n#### deep\ny\n").is_ok());
    }

    #[test]
    fn splice_rejects_unclosed_fence_but_allows_balanced() {
        assert!(splice(DOC, "A", "## A\n```\ncode\n").is_err());
        let crlf = DOC.replace('\n', "\r\n");
        assert!(splice(&crlf, "A", "## A\n```\ncode\n").is_err());
        assert!(splice(DOC, "A", "## A\n```\n## x\n```\n").is_ok());
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

    #[test]
    fn printed_paths_round_trip() {
        let doc = "# Issue #12\n\na\n\n# A > B\n\nb\n\n# back\\slash\n\nc\n\n# C#\n\nd\n\n\
                   # Issue #12\n\ne\n\n# P > Q\n\n## Kid\n\nf\n\n# Intro <!-- x -->\n\ng\n\n\
                   # Intro\n\nh\n\n# P\n\n## Q\n\n### Kid\n\ni\n";
        let hs = parse(doc);
        for h in &hs {
            let p = h.display_path();
            let got = resolve(&hs, &p).unwrap_or_else(|e| panic!("{p:?}: {e:?}"));
            assert_eq!(got.start, h.start, "path {p:?}");
        }
    }

    #[test]
    fn escaped_hash_digits_is_a_name() {
        let doc = "# Issue #12\n\nbody\n";
        assert_eq!(paths(doc), vec!["Issue \\#12"]);
        assert_eq!(extract(doc, "Issue \\#12").unwrap(), doc);
        // Unescaped, it means name "Issue" occurrence 12.
        assert!(extract(doc, "Issue #12").is_err());
    }

    #[test]
    fn gt_in_heading_is_not_nesting() {
        let doc = "# A > B\n\nx\n\n# A\n\n## B\n\ny\n";
        assert_eq!(extract(doc, "A \\> B").unwrap(), "# A > B\n\nx\n\n");
        assert_eq!(extract(doc, "A > B").unwrap(), "## B\n\ny\n");
    }

    #[test]
    fn splice_via_escaped_path() {
        let doc = "# A > B\n\nx\n\n# C\n\nz\n";
        let out = splice(doc, "A \\> B", "# A > B\n\nnew\n").unwrap();
        assert_eq!(out, "# A > B\n\nnew\n# C\n\nz\n");
    }
}
