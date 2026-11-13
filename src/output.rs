//! The one output layer: the format decision and a renderer for every typed
//! command result. Commands build a value and call [`emit`]; none of them
//! chooses a format or formats text itself.
//!
//! JSON is the serialized core type (the same shapes the API returns). Text is
//! a short human rendering, defined next to the type by [`Render`].

use serde::Serialize;

use crate::artifact::{ListRow, Ref};
use crate::comments::{Added, CommentList, ThreadResult};
use crate::embeds::EmbedList;
use crate::error::Error;
use crate::sections::Outline;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Json,
    Text,
}

/// Env vars that mark a coding-agent session; any one set to a non-empty value
/// means JSON output. Claude Code sets all three. No verified marker is known
/// for other harnesses: add one here, or have the harness export
/// `CAIRN_FORMAT=json`.
pub const AGENT_ENV_VARS: &[&str] = &[
    "AI_AGENT",
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "PI_CODING_AGENT",
];

/// The `--json` / `--text` global flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    pub json: bool,
    pub text: bool,
}

/// Decide the format for one stream. Precedence, high to low:
///
/// 1. `--json` / `--text` (both is a usage error).
/// 2. `CAIRN_FORMAT=json|text` (anything else is a usage error; empty is unset).
/// 3. An agent session ([`AGENT_ENV_VARS`]): JSON.
/// 4. `is_tty`: text on a terminal, JSON otherwise.
///
/// Pure: the environment and the terminal check are inputs, so tests need
/// neither a real environment nor a real terminal.
pub fn decide(
    flags: Flags,
    env: &dyn Fn(&str) -> Option<String>,
    is_tty: bool,
) -> Result<Format, Error> {
    match (flags.json, flags.text) {
        (true, true) => {
            return Err(Error::Usage(
                "--json and --text are mutually exclusive".into(),
            ));
        }
        (true, false) => return Ok(Format::Json),
        (false, true) => return Ok(Format::Text),
        (false, false) => {}
    }
    let set = |k: &str| env(k).filter(|v| !v.is_empty());
    if let Some(v) = set("CAIRN_FORMAT") {
        return match v.as_str() {
            "json" => Ok(Format::Json),
            "text" => Ok(Format::Text),
            _ => Err(Error::Usage(format!(
                "CAIRN_FORMAT must be json or text, got {v:?}"
            ))),
        };
    }
    if AGENT_ENV_VARS.iter().any(|k| set(k).is_some()) || !is_tty {
        return Ok(Format::Json);
    }
    Ok(Format::Text)
}

/// A command result's human rendering. Each line ends with a newline.
pub trait Render {
    fn text(&self) -> String;
}

/// Print `value` to stdout in `fmt`.
pub fn emit<T: Serialize + Render>(fmt: Format, value: &T) {
    match fmt {
        Format::Json => println!(
            "{}",
            serde_json::to_string(value).expect("result serializes")
        ),
        Format::Text => print!("{}", value.text()),
    }
}

/// The line printed on stderr for `err`.
pub fn render_error(fmt: Format, err: &Error) -> String {
    match fmt {
        Format::Json => err.to_json(),
        Format::Text => format!("error: {err}"),
    }
}

/// `delete` result.
#[derive(Serialize)]
pub struct Deleted {
    pub deleted: Vec<String>,
}

/// `list` result.
#[derive(Serialize)]
pub struct Listing<'a> {
    pub artifacts: &'a [ListRow],
}

/// One file written by `get --out`.
#[derive(Serialize)]
pub struct FileEntry {
    pub path: String,
    pub media_type: String,
    pub size: i64,
}

/// `get --out` result.
#[derive(Serialize)]
pub struct Files {
    pub files: Vec<FileEntry>,
}

/// What `serve` prints once both ports are bound.
#[derive(Serialize)]
pub struct ServeStarted {
    pub ui: String,
    pub content: String,
    pub reviewer: String,
}

impl Render for Ref {
    fn text(&self) -> String {
        format!("{self}\n")
    }
}

impl Render for Deleted {
    fn text(&self) -> String {
        format!("deleted {}\n", self.deleted.join(" "))
    }
}

impl Render for Listing<'_> {
    fn text(&self) -> String {
        list_table(self.artifacts)
    }
}

impl Render for Outline {
    /// One line per heading, indented by level: the full path (what `--section`
    /// takes), then its size in bytes.
    fn text(&self) -> String {
        let mut out = String::new();
        for e in &self.sections {
            let indent = "  ".repeat(usize::from(e.level.saturating_sub(1)));
            out.push_str(&format!("{indent}{}  ({} bytes)\n", e.path, e.bytes));
        }
        out
    }
}

impl Render for EmbedList {
    /// One line per embed: `L<line> cairn:<id>[@N] <kind>[/<format>]`, then
    /// `-> @<rev>` for an unpinned reference or `dangling`.
    fn text(&self) -> String {
        let mut out = String::new();
        for e in &self.embeds {
            out.push_str(&format!("L{} cairn:{}", e.line, e.target_id));
            if e.pinned
                && let Some(n) = e.target_revision
            {
                out.push_str(&format!("@{n}"));
            }
            if let Some(k) = &e.kind {
                out.push_str(&format!(" {k}"));
                if let Some(f) = &e.format {
                    out.push_str(&format!("/{f}"));
                }
            }
            match (e.pinned, e.target_revision) {
                (_, None) => out.push_str(" dangling"),
                (false, Some(n)) => out.push_str(&format!(" -> @{n}")),
                (true, Some(_)) => {}
            }
            out.push('\n');
        }
        out
    }
}

impl Render for Added {
    fn text(&self) -> String {
        let at = format!("{}@{}", self.artifact_id, self.revision);
        let mut out = if self.id == self.thread_id {
            format!("#{} {at}", self.id)
        } else {
            format!("#{} reply in #{} {at}", self.id, self.thread_id)
        };
        if self.stale {
            out.push_str(&format!(" (stale, latest is @{})", self.latest_revision));
        }
        out.push_str(&format!(
            "\nhighest_comment_id {}\n",
            self.highest_comment_id
        ));
        out
    }
}

impl Render for ThreadResult {
    fn text(&self) -> String {
        let t = &self.thread;
        format!("#{} {}@{} {}\n", t.id, t.artifact_id, t.revision, t.status)
    }
}

impl Render for CommentList {
    fn text(&self) -> String {
        comment_table(self)
    }
}

impl Render for Files {
    fn text(&self) -> String {
        self.files
            .iter()
            .map(|f| format!("{}  {}  {} bytes\n", f.path, f.media_type, f.size))
            .collect()
    }
}

impl Render for ServeStarted {
    fn text(&self) -> String {
        format!(
            "ui       {}\ncontent  {}\nreviewer {}\n",
            self.ui, self.content, self.reviewer
        )
    }
}

fn list_table(rows: &[ListRow]) -> String {
    let header = [
        "ID", "KIND", "NAME", "REV", "OPEN", "AUTHOR", "BYTES", "UPDATED", "PROJECT", "WORKTREE",
    ];
    let mut table: Vec<Vec<String>> = vec![header.iter().map(|s| s.to_string()).collect()];
    for r in rows {
        table.push(vec![
            r.id.clone(),
            r.kind.clone(),
            r.name.clone(),
            r.latest_revision.to_string(),
            r.open_threads.to_string(),
            r.author.clone(),
            r.bytes.to_string(),
            r.updated_at.clone(),
            r.project.clone().unwrap_or_else(|| "-".into()),
            r.worktree.clone().unwrap_or_else(|| "-".into()),
        ]);
    }
    let widths: Vec<usize> = (0..header.len())
        .map(|c| {
            table
                .iter()
                .map(|row| row[c].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut out = String::new();
    for row in &table {
        let line: Vec<String> = row
            .iter()
            .zip(&widths)
            .map(|(cell, w)| format!("{cell:<w$}"))
            .collect();
        out.push_str(line.join("  ").trim_end());
        out.push('\n');
    }
    out
}

fn comment_table(r: &CommentList) -> String {
    let mut out = format!("highest_comment_id {}\n", r.highest_comment_id);
    for t in &r.threads {
        let loc = match (
            &t.locator.section_path,
            t.locator.line_start,
            t.locator.line_end,
        ) {
            (Some(p), _, _) => p.clone(),
            (None, Some(a), Some(b)) if a == b => format!("L{a}"),
            (None, Some(a), Some(b)) => format!("L{a}-{b}"),
            _ => "-".into(),
        };
        out.push_str(&format!(
            "#{} {}@{}{} {} {} {} [{}]: {}\n",
            t.id,
            t.artifact_id,
            t.revision,
            if t.stale { " (stale)" } else { "" },
            t.status,
            t.author,
            loc,
            t.role,
            t.body.lines().next().unwrap_or("")
        ));
        for rep in &t.replies {
            out.push_str(&format!(
                "    #{} {} [{}]: {}\n",
                rep.id,
                rep.author,
                rep.role,
                rep.body.lines().next().unwrap_or("")
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sections::OutlineEntry;

    const J: Flags = Flags {
        json: true,
        text: false,
    };
    const T: Flags = Flags {
        json: false,
        text: true,
    };
    const NONE: Flags = Flags {
        json: false,
        text: false,
    };

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    fn d(flags: Flags, pairs: &[(&str, &str)], tty: bool) -> Result<Format, Error> {
        decide(flags, &env(pairs), tty)
    }

    #[test]
    fn flags_beat_everything() {
        let all = [
            ("CAIRN_FORMAT", "text"),
            ("AI_AGENT", "x"),
            ("CLAUDECODE", "1"),
        ];
        for tty in [false, true] {
            assert_eq!(d(J, &all, tty).unwrap(), Format::Json);
            assert_eq!(
                d(T, &[("CAIRN_FORMAT", "json"), ("CLAUDECODE", "1")], tty).unwrap(),
                Format::Text
            );
        }
        // A bad CAIRN_FORMAT is not consulted when a flag decides.
        assert_eq!(
            d(T, &[("CAIRN_FORMAT", "yaml")], true).unwrap(),
            Format::Text
        );
    }

    #[test]
    fn both_flags_are_a_usage_error() {
        let both = Flags {
            json: true,
            text: true,
        };
        let err = d(both, &[], true).unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn cairn_format_beats_agent_and_tty() {
        for tty in [false, true] {
            assert_eq!(
                d(NONE, &[("CAIRN_FORMAT", "text"), ("CLAUDECODE", "1")], tty).unwrap(),
                Format::Text
            );
            assert_eq!(
                d(NONE, &[("CAIRN_FORMAT", "json")], tty).unwrap(),
                Format::Json
            );
        }
    }

    #[test]
    fn invalid_cairn_format_is_a_usage_error() {
        for bad in ["yaml", "JSON", " json"] {
            let err = d(NONE, &[("CAIRN_FORMAT", bad)], false).unwrap_err();
            assert_eq!(err.exit_code(), 2, "{bad:?}");
            assert_eq!(err.code(), "usage");
        }
        // Empty counts as unset.
        assert_eq!(
            d(NONE, &[("CAIRN_FORMAT", "")], true).unwrap(),
            Format::Text
        );
    }

    #[test]
    fn each_agent_var_forces_json_even_on_a_terminal() {
        for var in AGENT_ENV_VARS {
            assert_eq!(d(NONE, &[(var, "1")], true).unwrap(), Format::Json, "{var}");
            // Set but empty is not a marker.
            assert_eq!(d(NONE, &[(var, "")], true).unwrap(), Format::Text, "{var}");
        }
        assert_eq!(AGENT_ENV_VARS.len(), 4);
    }

    #[test]
    fn terminal_decides_when_nothing_else_does() {
        assert_eq!(d(NONE, &[], true).unwrap(), Format::Text);
        assert_eq!(d(NONE, &[], false).unwrap(), Format::Json);
    }

    #[test]
    fn errors_render_per_format() {
        let e = Error::NotFound("no such artifact".into());
        let j: serde_json::Value = serde_json::from_str(&render_error(Format::Json, &e)).unwrap();
        assert_eq!(j["code"], "not_found");
        assert_eq!(render_error(Format::Text, &e), "error: no such artifact");
    }

    #[test]
    fn ref_text_is_the_bare_ref() {
        let r = Ref {
            id: "4hk8zc4q".into(),
            revision: 1,
        };
        assert_eq!(r.text(), "4hk8zc4q@1\n");
    }

    #[test]
    fn deleted_and_files_text() {
        let del = Deleted {
            deleted: vec!["a".into(), "b".into()],
        };
        assert_eq!(del.text(), "deleted a b\n");
        let files = Files {
            files: vec![FileEntry {
                path: "css/a.css".into(),
                media_type: "text/css".into(),
                size: 12,
            }],
        };
        assert_eq!(files.text(), "css/a.css  text/css  12 bytes\n");
    }

    #[test]
    fn outline_text_is_an_indented_tree() {
        let o = Outline {
            sections: vec![
                OutlineEntry {
                    path: "A".into(),
                    level: 1,
                    line: 1,
                    bytes: 100,
                },
                OutlineEntry {
                    path: "A > B".into(),
                    level: 2,
                    line: 2,
                    bytes: 40,
                },
                OutlineEntry {
                    path: "A > B > C".into(),
                    level: 3,
                    line: 3,
                    bytes: 10,
                },
            ],
        };
        assert_eq!(
            o.text(),
            "A  (100 bytes)\n  A > B  (40 bytes)\n    A > B > C  (10 bytes)\n"
        );
    }

    #[test]
    fn serve_started_text_has_three_lines() {
        let s = ServeStarted {
            ui: "http://127.0.0.1:1".into(),
            content: "http://127.0.0.1:2".into(),
            reviewer: "me".into(),
        };
        assert_eq!(s.text().lines().count(), 3);
    }
}
