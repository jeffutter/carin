//! The clap tree and the resolved invocation context.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::config::resolve_db_path_from_env;
use crate::error::Error;
use crate::kind::Kind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Role {
    Agent,
    Reviewer,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Agent => "agent",
            Role::Reviewer => "reviewer",
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "cairn", version, about = "Review Artifacts with agents")]
pub struct Cli {
    /// Attribution role for this invocation
    #[arg(
        long,
        global = true,
        env = "CAIRN_ROLE",
        value_enum,
        default_value = "agent"
    )]
    pub role: Role,

    /// Author name (default: the role name)
    #[arg(long, global = true, env = "CAIRN_AGENT")]
    pub author: Option<String>,

    /// Session label (falls back to CLAUDE_CODE_SESSION_ID)
    #[arg(long, global = true, env = "CAIRN_SESSION")]
    pub session: Option<String>,

    /// Database path
    #[arg(long, global = true, env = "CAIRN_DB")]
    pub db: Option<String>,

    /// Project root path or name (default: detected from the cwd)
    #[arg(long, global = true, env = "CAIRN_PROJECT")]
    pub project: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Store a new Artifact and print its ref
    Submit {
        /// File to submit, or `-` for stdin
        path: String,
        /// Artifact Kind (inferred from the extension; required for stdin)
        #[arg(long, value_enum)]
        kind: Option<Kind>,
        /// Artifact name (default: the file name; required for stdin)
        #[arg(long)]
        name: Option<String>,
        /// Diagram format
        #[arg(long)]
        format: Option<String>,
    },
    /// Replace an Artifact's content with a new Revision and print its ref
    Edit {
        /// Artifact id (no `@revision`)
        id: String,
        /// File with the new content, or `-` for stdin
        path: String,
        /// Refuse unless this is still the latest Revision (exit 6)
        #[arg(long)]
        base: Option<i64>,
        /// Replace only this markdown Section ("A > B"); the file must start with its heading
        #[arg(long)]
        section: Option<String>,
    },
    /// Print an Artifact's content: `abc123` or `abc123@2`
    Get {
        target: String,
        /// Print only this markdown Section ("A > B", children included)
        #[arg(long)]
        section: Option<String>,
    },
    /// List a markdown Artifact's headings with their paths and sizes
    Outline { target: String },
    /// List Artifacts, newest activity first
    List {
        /// Only this Kind
        #[arg(long, value_enum)]
        kind: Option<Kind>,
        /// Only Artifacts created by the caller (its Session if it has one, else its author)
        #[arg(long)]
        mine: bool,
        /// Only Artifacts with open Threads
        #[arg(long)]
        open: bool,
        /// Only Artifacts last updated longer ago than this: 30d, 12h, 90m, 45s, 2w
        #[arg(long, value_name = "DURATION")]
        older_than: Option<String>,
        /// Maximum rows
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(i64).range(1..))]
        limit: i64,
        /// Print a table instead of JSON
        #[arg(long)]
        text: bool,
    },
    /// Comments and Threads
    Comment {
        #[command(subcommand)]
        action: CommentAction,
    },
    /// Delete Artifacts with all their Revisions
    Delete {
        /// Artifact ids
        #[arg(required = true, num_args = 1..)]
        ids: Vec<String>,
        /// Confirm the deletion (there is no prompt)
        #[arg(long)]
        yes: bool,
    },
}

/// `a-b` or a single line `a`, 1-based and inclusive.
fn parse_lines(s: &str) -> Result<(i64, i64), String> {
    let bad = || format!("bad line range {s:?}: use a-b or a single line number");
    let num = |t: &str| t.parse::<i64>().ok().filter(|n| *n >= 1).ok_or_else(bad);
    match s.split_once('-') {
        Some((a, b)) => {
            let (a, b) = (num(a)?, num(b)?);
            if b < a {
                return Err(bad());
            }
            Ok((a, b))
        }
        None => num(s).map(|n| (n, n)),
    }
}

#[derive(Debug, Subcommand)]
pub enum CommentAction {
    /// Add a Comment (a new Thread, or a reply with --reply)
    Add {
        /// `abc123` or `abc123@2`; defaults to the latest Revision
        target: String,
        /// Comment text, or `-` for stdin
        body: String,
        /// Anchor to a line range of a text Artifact (`a-b` or `a`); the quote is stored
        #[arg(long, value_parser = parse_lines, conflicts_with = "reply")]
        lines: Option<(i64, i64)>,
        /// Anchor to a markdown Section ("A > B"), resolved against the Comment's own Revision
        #[arg(long, conflicts_with = "reply")]
        section: Option<String>,
        /// Reply in the Thread of this root Comment id
        #[arg(long, value_name = "COMMENT_ID")]
        reply: Option<i64>,
    },
    /// Resolve a Thread
    Resolve { thread_id: i64 },
    /// Reopen a resolved Thread
    Reopen { thread_id: i64 },
    /// List Threads (open ones by default, on any Revision, stale included)
    List {
        /// Only this Artifact (no `@revision`); ignores the Session scope
        target: Option<String>,
        /// Include resolved Threads
        #[arg(long)]
        all: bool,
        /// Only Threads holding a Comment newer than this id, with all their
        /// replies; the open/--all rule still applies
        #[arg(long, value_name = "COMMENT_ID")]
        since: Option<i64>,
        /// Print a table instead of JSON
        #[arg(long)]
        text: bool,
    },
}

/// Global flags after env fallbacks and defaults.
#[derive(Debug, PartialEq, Eq)]
pub struct Context {
    pub role: Role,
    pub author: String,
    pub session: Option<String>,
    /// The session given by `--session` or `CAIRN_SESSION`, without the
    /// `CLAUDE_CODE_SESSION_ID` fallback. `list --session` filters on this,
    /// so running inside Claude Code does not filter by default.
    pub session_explicit: Option<String>,
    pub db_path: PathBuf,
    pub project: Option<String>,
}

fn non_empty(v: Option<String>) -> Option<String> {
    v.filter(|s| !s.is_empty())
}

impl Cli {
    /// Resolve defaults and fallbacks. `fallback_session` is the value of
    /// `CLAUDE_CODE_SESSION_ID`, passed in so tests need not touch the env.
    pub fn resolve(self, fallback_session: Option<String>) -> (Context, Command) {
        let author = non_empty(self.author).unwrap_or_else(|| self.role.as_str().to_string());
        let session_explicit = non_empty(self.session);
        let session = session_explicit.clone().or(non_empty(fallback_session));
        let ctx = Context {
            role: self.role,
            author,
            session,
            session_explicit,
            db_path: resolve_db_path_from_env(self.db.as_deref()),
            // An empty value is meaningful ("disable detection"), so keep it.
            project: self.project,
        };
        (ctx, self.command)
    }
}

/// Parse argv. Help and version print and exit 0; other clap failures become
/// a usage `Error` (exit 2).
pub fn parse<I, T>(args: I) -> Result<Cli, Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    Cli::try_parse_from(args).map_err(|e| {
        if e.use_stderr() {
            Error::Usage(e.render().to_string().trim().to_string())
        } else {
            e.exit()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(args: &[&str]) -> Cli {
        Cli::try_parse_from(
            std::iter::once("cairn")
                .chain(args.iter().copied())
                .chain(["get", "x"]),
        )
        .unwrap()
    }

    #[test]
    fn defaults() {
        let c = cli(&["--db", "/d.db"]);
        // CAIRN_* env vars are not set by the test harness for role/author.
        let ctx = c.resolve(None).0;
        assert_eq!(ctx.author, ctx.role.as_str());
        assert_eq!(ctx.db_path, PathBuf::from("/d.db"));
    }

    #[test]
    fn reviewer_role_names_default_author() {
        let ctx = cli(&["--role", "reviewer", "--db", "/d.db"])
            .resolve(None)
            .0;
        assert_eq!(ctx.role, Role::Reviewer);
        assert_eq!(ctx.author, "reviewer");
    }

    #[test]
    fn explicit_author_and_session() {
        let ctx = cli(&[
            "--author",
            "bot",
            "--session",
            "s1",
            "--project",
            "p",
            "--db",
            "/d",
        ])
        .resolve(Some("fallback".into()))
        .0;
        assert_eq!(ctx.author, "bot");
        assert_eq!(ctx.session.as_deref(), Some("s1"));
        assert_eq!(ctx.project.as_deref(), Some("p"));
    }

    #[test]
    fn explicit_session_excludes_fallback() {
        let ctx = cli(&["--db", "/d"]).resolve(Some("fallback".into())).0;
        if std::env::var_os("CAIRN_SESSION").is_none() {
            assert_eq!(ctx.session_explicit, None);
        }
        let ctx = cli(&["--db", "/d", "--session", "s"])
            .resolve(Some("fallback".into()))
            .0;
        assert_eq!(ctx.session_explicit.as_deref(), Some("s"));
    }

    #[test]
    fn session_falls_back() {
        let ctx = cli(&["--db", "/d"]).resolve(Some("fallback".into())).0;
        if std::env::var_os("CAIRN_SESSION").is_none() {
            assert_eq!(ctx.session.as_deref(), Some("fallback"));
        }
    }

    #[test]
    fn bad_role_is_usage_error() {
        let err = parse(["cairn", "--role", "boss", "get", "x"]).unwrap_err();
        assert_eq!(err.exit_code(), 2);
        assert_eq!(err.code(), "usage");
    }
}
