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
    /// Print an Artifact's content: `abc123` or `abc123@2`
    Get { target: String },
}

/// Global flags after env fallbacks and defaults.
#[derive(Debug, PartialEq, Eq)]
pub struct Context {
    pub role: Role,
    pub author: String,
    pub session: Option<String>,
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
        let session = non_empty(self.session).or(non_empty(fallback_session));
        let ctx = Context {
            role: self.role,
            author,
            session,
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
