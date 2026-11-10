//! Project and Worktree scoping: which git Project (and linked Worktree) an
//! Artifact belongs to, and which Artifacts a read shows by default.
//!
//! Detection shells out to `git` and never fails: no git, no repo, a bare repo
//! or any other problem just means "no scope".

use std::path::Path;
use std::process::Command;

use rusqlite::types::Value;

/// Where an Artifact was created. `root` is the Project's main checkout;
/// `worktree` is set only inside a linked worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub name: String,
    pub root: String,
    pub worktree: Option<String>,
}

/// What `--project` (or the cwd) selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// No scope at all: not in a repo, or detection disabled.
    None,
    /// A full scope: detected from the cwd or built from a root path.
    Scope(Scope),
    /// A bare name: a read filter that cannot say where to store anything.
    Name(String),
}

fn last_segment(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Parse the output of
/// `git rev-parse --path-format=absolute --git-common-dir --show-toplevel`.
fn parse_rev_parse(out: &str) -> Option<Scope> {
    let mut lines = out.lines();
    let common = lines.next()?.trim_end();
    let toplevel = lines.next()?.trim_end();
    if common.is_empty() || toplevel.is_empty() {
        return None;
    }
    let common = Path::new(common);
    // A normal repository's common dir is `<root>/.git`, shared by every linked
    // worktree. A submodule's lives under `<super>/.git/modules/`, so the
    // toplevel is its own root.
    let root = match (common.file_name(), common.parent()) {
        (Some(n), Some(parent)) if n == ".git" => parent.to_string_lossy().into_owned(),
        _ => toplevel.to_string(),
    };
    let worktree = (toplevel != root).then(|| toplevel.to_string());
    Some(Scope {
        name: last_segment(&root),
        root,
        worktree,
    })
}

fn detect_git(cwd: &Path) -> Option<Scope> {
    let out = Command::new("git")
        .args([
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
            "--show-toplevel",
        ])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_rev_parse(&String::from_utf8_lossy(&out.stdout))
}

/// A root path as stored and compared: a relative path is taken from `cwd`,
/// then symlinks are resolved. A path that does not exist is normalized
/// lexically instead.
fn normalize_root(p: &str, cwd: &Path) -> String {
    use std::path::Component;
    let joined = cwd.join(p);
    if let Ok(c) = std::fs::canonicalize(&joined) {
        return c.to_string_lossy().into_owned();
    }
    // Not on disk: collapse `.` and `..` lexically (this also drops any
    // trailing `/`).
    let mut out = std::path::PathBuf::new();
    for c in joined.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out.to_string_lossy().into_owned()
}

/// Resolve `--project` against the cwd.
/// - unset: detect from the cwd;
/// - empty or `none`: no scope;
/// - a path (absolute, containing `/`, or starting with `.`): that root,
///   relative paths taken from the cwd (the Worktree is kept only if the cwd is
///   in a Worktree of that same root);
/// - anything else: a name.
pub fn select(project: Option<&str>, cwd: &Path) -> Selection {
    match project {
        None => detect_git(cwd).map_or(Selection::None, Selection::Scope),
        Some("") | Some("none") => Selection::None,
        Some(p) if Path::new(p).is_absolute() || p.contains('/') || p.starts_with('.') => {
            let root = normalize_root(p, cwd);
            let worktree = detect_git(cwd)
                .filter(|d| d.root == root)
                .and_then(|d| d.worktree);
            Selection::Scope(Scope {
                name: last_segment(&root),
                root,
                worktree,
            })
        }
        Some(name) => Selection::Name(name.to_string()),
    }
}

/// The Scope to store on a new Artifact. A bare name cannot say where, so it
/// is a usage error for the caller to report.
pub fn for_write(sel: Selection) -> Result<Option<Scope>, String> {
    match sel {
        Selection::None => Ok(None),
        Selection::Scope(s) => Ok(Some(s)),
        Selection::Name(n) => Err(format!(
            "--project {n:?} is a name, which only reads accept; \
             pass a root path to submit into that Project"
        )),
    }
}

/// Which Artifacts a list shows. Built once, rendered as one SQL fragment for
/// both `list` and `comment list`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScopeFilter {
    name: Option<String>,
    root: Option<String>,
    /// Show Artifacts of every Worktree.
    any_worktree: bool,
    /// The Worktree the caller is in; its Artifacts show with the root's.
    worktree: Option<String>,
}

impl ScopeFilter {
    /// No filtering at all.
    pub fn unscoped() -> Self {
        ScopeFilter {
            any_worktree: true,
            ..Default::default()
        }
    }

    /// Default is strict: the Project's root Artifacts, plus the caller's own
    /// Worktree when in one. `--all-projects` drops the Project filter,
    /// `--all-worktrees` the Worktree one, and `project_root` replaces the
    /// root (and, being more specific, the detected name).
    pub fn build(
        sel: &Selection,
        all_projects: bool,
        all_worktrees: bool,
        project_root: Option<&str>,
        cwd: &Path,
    ) -> Self {
        let (mut name, mut root, worktree, scoped) = match sel {
            Selection::None => (None, None, None, false),
            Selection::Name(n) => (Some(n.clone()), None, None, false),
            Selection::Scope(s) => (
                Some(s.name.clone()),
                Some(s.root.clone()),
                s.worktree.clone(),
                true,
            ),
        };
        if let Some(r) = project_root {
            root = Some(normalize_root(r, cwd));
            if scoped {
                // The detected name belongs to the detected root.
                name = None;
            }
        }
        if all_projects {
            name = None;
            root = None;
        }
        ScopeFilter {
            name,
            root,
            // Only a cwd inside a repo (or an explicit root) has a Worktree to
            // be strict about.
            any_worktree: all_worktrees || !scoped,
            worktree,
        }
    }

    /// SQL conditions on the artifacts table aliased `alias`, using numbered
    /// parameters `?base` to `?base+3`; bind them with [`Self::params`].
    pub fn sql(&self, alias: &str, base: usize) -> String {
        let (n, r, a, w) = (base, base + 1, base + 2, base + 3);
        format!(
            "(?{n} IS NULL OR {alias}.project_name = ?{n})
             AND (?{r} IS NULL OR {alias}.project_root = ?{r})
             AND (?{a} = 1 OR {alias}.worktree IS NULL OR {alias}.worktree = ?{w})"
        )
    }

    pub fn params(&self) -> [Value; 4] {
        let opt = |o: &Option<String>| o.clone().map_or(Value::Null, Value::Text);
        [
            opt(&self.name),
            opt(&self.root),
            Value::Integer(self.any_worktree as i64),
            opt(&self.worktree),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_root_checkout() {
        let s = parse_rev_parse("/src/cairn/.git\n/src/cairn\n").unwrap();
        assert_eq!(s.name, "cairn");
        assert_eq!(s.root, "/src/cairn");
        assert_eq!(s.worktree, None);
    }

    #[test]
    fn parses_linked_worktree() {
        let s = parse_rev_parse("/src/cairn/.git\n/wt/feature\n").unwrap();
        assert_eq!(s.root, "/src/cairn");
        assert_eq!(s.worktree.as_deref(), Some("/wt/feature"));
    }

    #[test]
    fn submodule_is_its_own_project() {
        let s = parse_rev_parse("/src/super/.git/modules/lib\n/src/super/lib\n").unwrap();
        assert_eq!(s.name, "lib");
        assert_eq!(s.root, "/src/super/lib");
        assert_eq!(s.worktree, None);
    }

    #[test]
    fn garbage_is_no_scope() {
        assert_eq!(parse_rev_parse(""), None);
        assert_eq!(parse_rev_parse("/only/one/line\n"), None);
    }

    #[test]
    fn project_flag_forms() {
        let cwd = Path::new("/nonexistent");
        assert_eq!(select(Some(""), cwd), Selection::None);
        assert_eq!(select(Some("none"), cwd), Selection::None);
        assert_eq!(select(Some("cairn"), cwd), Selection::Name("cairn".into()));
        assert_eq!(
            select(Some("/no/such/dir/proj"), cwd),
            Selection::Scope(Scope {
                name: "proj".into(),
                root: "/no/such/dir/proj".into(),
                worktree: None
            })
        );
        assert!(for_write(Selection::Name("x".into())).is_err());
    }

    #[test]
    fn filter_rules() {
        let cwd = Path::new("/nonexistent");
        let sel = Selection::Scope(Scope {
            name: "p".into(),
            root: "/p".into(),
            worktree: Some("/wt".into()),
        });
        let strict = ScopeFilter::build(&sel, false, false, None, cwd);
        assert!(!strict.any_worktree);
        assert_eq!(strict.root.as_deref(), Some("/p"));
        assert!(ScopeFilter::build(&sel, false, true, None, cwd).any_worktree);
        let all = ScopeFilter::build(&sel, true, false, None, cwd);
        assert_eq!((all.name, all.root), (None, None));
        let narrowed = ScopeFilter::build(&sel, false, false, Some("/q"), cwd);
        assert_eq!(narrowed.name, None);
        assert_eq!(narrowed.root.as_deref(), Some("/q"));
        assert_eq!(
            ScopeFilter::build(&Selection::None, false, false, None, cwd),
            ScopeFilter::unscoped()
        );
    }

    #[test]
    fn relative_project_paths_resolve_against_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir(cwd.join("repo")).unwrap();
        let root = |s: Selection| match s {
            Selection::Scope(s) => s.root,
            other => panic!("expected a scope, got {other:?}"),
        };
        let here = cwd.to_string_lossy().into_owned();
        assert_eq!(root(select(Some("."), &cwd)), here);
        assert_eq!(
            root(select(Some("./repo"), &cwd)),
            cwd.join("repo").to_string_lossy()
        );
        assert_eq!(
            root(select(Some("repo/"), &cwd)),
            cwd.join("repo").to_string_lossy()
        );
        assert_eq!(
            root(select(Some("../x"), &cwd.join("repo"))),
            cwd.join("x").to_string_lossy()
        );
        assert_eq!(select(Some("repo"), &cwd), Selection::Name("repo".into()));
    }

    #[test]
    fn build_resolves_relative_project_root_like_select() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = std::fs::canonicalize(dir.path()).unwrap();
        let f = ScopeFilter::build(&Selection::None, false, false, Some("./repo"), &cwd);
        assert_eq!(
            f.root.as_deref(),
            Some(cwd.join("repo").to_string_lossy().as_ref())
        );
    }
}
