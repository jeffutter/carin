//! Database path resolution.

use std::path::PathBuf;

fn non_empty(v: Option<&str>) -> Option<&str> {
    v.filter(|s| !s.is_empty())
}

/// Resolve the database path from explicit inputs: `--db`, then `CAIRN_DB`,
/// then `$XDG_DATA_HOME/cairn/cairn.db`, then `$HOME/.local/share/cairn/cairn.db`.
/// Empty values count as unset. Does not create anything.
pub fn resolve_db_path(
    flag: Option<&str>,
    env_cairn_db: Option<&str>,
    xdg_data_home: Option<&str>,
    home: Option<&str>,
) -> PathBuf {
    if let Some(p) = non_empty(flag).or(non_empty(env_cairn_db)) {
        return PathBuf::from(p);
    }
    let data_dir = match (non_empty(xdg_data_home), non_empty(home)) {
        (Some(x), _) => PathBuf::from(x),
        (None, Some(h)) => PathBuf::from(h).join(".local/share"),
        (None, None) => PathBuf::from(".local/share"),
    };
    data_dir.join("cairn").join("cairn.db")
}

/// `resolve_db_path` reading the real process environment.
/// `CAIRN_DB` is handled by clap's `env` on `--db`, so `flag` already folds it.
pub fn resolve_db_path_from_env(flag: Option<&str>) -> PathBuf {
    resolve_db_path(
        flag,
        std::env::var("CAIRN_DB").ok().as_deref(),
        std::env::var("XDG_DATA_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_wins() {
        let p = resolve_db_path(Some("/f.db"), Some("/e.db"), Some("/x"), Some("/h"));
        assert_eq!(p, PathBuf::from("/f.db"));
    }

    #[test]
    fn env_beats_xdg() {
        let p = resolve_db_path(None, Some("/e.db"), Some("/x"), Some("/h"));
        assert_eq!(p, PathBuf::from("/e.db"));
    }

    #[test]
    fn xdg_beats_home() {
        let p = resolve_db_path(None, None, Some("/x"), Some("/h"));
        assert_eq!(p, PathBuf::from("/x/cairn/cairn.db"));
    }

    #[test]
    fn home_fallback() {
        let p = resolve_db_path(None, None, None, Some("/h"));
        assert_eq!(p, PathBuf::from("/h/.local/share/cairn/cairn.db"));
    }

    #[test]
    fn empty_values_are_unset() {
        let p = resolve_db_path(Some(""), Some(""), Some(""), Some("/h"));
        assert_eq!(p, PathBuf::from("/h/.local/share/cairn/cairn.db"));
    }
}
