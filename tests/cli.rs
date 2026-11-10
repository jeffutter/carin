use std::process::Command;

fn cairn() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cairn"))
}

#[test]
fn help_exits_zero() {
    let out = cairn().arg("--help").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("--role"));
}

#[test]
fn bad_flag_exits_two_with_json_error() {
    let out = cairn().arg("--nope").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: serde_json::Value = serde_json::from_slice(&out.stderr).expect("stderr is a JSON error");
    assert_eq!(v["code"], "usage");
}

mod roundtrip {
    use super::cairn;
    use std::io::Write;
    use std::process::Stdio;

    fn base(db: &std::path::Path) -> std::process::Command {
        let mut c = cairn();
        for v in [
            "CAIRN_ROLE",
            "CAIRN_AGENT",
            "CAIRN_SESSION",
            "CLAUDE_CODE_SESSION_ID",
            "CAIRN_DB",
            "CAIRN_PROJECT",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
        ] {
            c.env_remove(v);
        }
        // Not inside any repo, so detection finds no scope.
        c.current_dir(std::env::temp_dir());
        c.arg("--db").arg(db);
        c
    }

    fn json(b: &[u8]) -> serde_json::Value {
        serde_json::from_slice(b).unwrap()
    }

    #[test]
    fn submit_then_get_roundtrip_with_nested_db() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("no/such/dir/cairn.db");
        let md = dir.path().join("doc.md");
        let text = "# Title\n\nline two - h\u{e9}llo \u{4e16}\u{754c}\n\n- item\n";
        std::fs::write(&md, text).unwrap();

        let out = base(&db).arg("submit").arg(&md).output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(db.exists());
        let v = json(&out.stdout);
        let id = v["id"].as_str().unwrap().to_string();
        assert_eq!(v["revision"], 1);
        assert_eq!(v["ref"], format!("{id}@1"));

        for target in [id.clone(), format!("{id}@1")] {
            let out = base(&db).args(["get", &target]).output().unwrap();
            assert_eq!(out.status.code(), Some(0));
            assert_eq!(out.stdout, text.as_bytes());
        }
    }

    #[test]
    fn unknown_id_and_revision_exit_three() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let out = base(&db).args(["get", "nope"]).output().unwrap();
        assert_eq!(out.status.code(), Some(3));
        assert_eq!(json(&out.stderr)["code"], "not_found");

        let md = dir.path().join("a.md");
        std::fs::write(&md, "x").unwrap();
        let v = json(&base(&db).arg("submit").arg(&md).output().unwrap().stdout);
        let target = format!("{}@9", v["id"].as_str().unwrap());
        assert_eq!(
            base(&db)
                .args(["get", &target])
                .output()
                .unwrap()
                .status
                .code(),
            Some(3)
        );
    }

    fn stdin_submit(db: &std::path::Path, args: &[&str]) -> std::process::Output {
        let mut child = base(db)
            .arg("submit")
            .arg("-")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"# hi\n").unwrap();
        child.wait_with_output().unwrap()
    }

    #[test]
    fn stdin_requires_kind_and_name() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        assert_eq!(stdin_submit(&db, &[]).status.code(), Some(2));
        assert_eq!(
            stdin_submit(&db, &["--kind", "markdown"]).status.code(),
            Some(2)
        );
        assert_eq!(
            stdin_submit(&db, &["--name", "n.md"]).status.code(),
            Some(2)
        );
        let out = stdin_submit(&db, &["--kind", "markdown", "--name", "n.md"]);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn attribution_flags_env_and_defaults_are_stored() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let md = dir.path().join("a.md");
        std::fs::write(&md, "x").unwrap();

        let ok = |mut c: std::process::Command| {
            let o = c.arg("submit").arg(&md).output().unwrap();
            assert_eq!(o.status.code(), Some(0));
        };
        let mut c = base(&db);
        c.args(["--author", "bot", "--role", "reviewer", "--session", "s1"]);
        ok(c);
        let mut c = base(&db);
        c.env("CLAUDE_CODE_SESSION_ID", "from-env");
        ok(c);
        ok(base(&db));

        let conn = rusqlite::Connection::open(&db).unwrap();
        let mut stmt = conn
            .prepare("SELECT author, role, session FROM revisions ORDER BY rowid")
            .unwrap();
        let rows: Vec<(String, String, Option<String>)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            rows,
            vec![
                ("bot".into(), "reviewer".into(), Some("s1".into())),
                ("agent".into(), "agent".into(), Some("from-env".into())),
                ("agent".into(), "agent".into(), None),
            ]
        );
    }

    #[test]
    fn over_limit_file_is_rejected_naming_limit() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let md = dir.path().join("big.md");
        std::fs::write(&md, vec![b'a'; 10 * 1024 * 1024 + 1]).unwrap();
        let out = base(&db).arg("submit").arg(&md).output().unwrap();
        assert_eq!(out.status.code(), Some(4));
        assert!(String::from_utf8_lossy(&out.stderr).contains("10 MiB"));

        let mut child = base(&db)
            .args(["submit", "-", "--kind", "markdown", "--name", "b.md"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let w = std::thread::spawn(move || {
            let _ = stdin.write_all(&vec![b'a'; 10 * 1024 * 1024 + 1]);
        });
        let out = child.wait_with_output().unwrap();
        w.join().unwrap();
        assert_eq!(out.status.code(), Some(4));
    }

    #[test]
    fn identical_content_shares_one_blob() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let md = dir.path().join("a.md");
        std::fs::write(&md, "same").unwrap();
        for _ in 0..2 {
            assert_eq!(
                base(&db)
                    .arg("submit")
                    .arg(&md)
                    .output()
                    .unwrap()
                    .status
                    .code(),
                Some(0)
            );
        }
        let conn = rusqlite::Connection::open(&db).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM blobs", [], |r| r.get(0))
            .unwrap();
        let a: i64 = conn
            .query_row("SELECT COUNT(*) FROM artifacts", [], |r| r.get(0))
            .unwrap();
        assert_eq!((n, a), (1, 2));
    }

    #[test]
    fn unsupported_kind_and_unknown_extension_exit_four() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let t = dir.path().join("a.txt");
        std::fs::write(&t, "x").unwrap();
        assert_eq!(
            base(&db)
                .arg("submit")
                .arg(&t)
                .output()
                .unwrap()
                .status
                .code(),
            Some(4)
        );
    }

    fn submit_md(db: &std::path::Path, dir: &std::path::Path, text: &str) -> String {
        let md = dir.join("a.md");
        std::fs::write(&md, text).unwrap();
        let o = base(db).arg("submit").arg(&md).output().unwrap();
        json(&o.stdout)["id"].as_str().unwrap().to_string()
    }

    fn edit_with(
        db: &std::path::Path,
        dir: &std::path::Path,
        id: &str,
        text: &str,
        extra: &[&str],
    ) -> std::process::Output {
        let f = dir.join("edit-input.md");
        std::fs::write(&f, text).unwrap();
        base(db)
            .args(extra)
            .args(["edit", id])
            .arg(&f)
            .output()
            .unwrap()
    }

    #[test]
    fn edit_appends_revision_and_keeps_earlier() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let id = submit_md(&db, dir.path(), "one");
        let o = edit_with(&db, dir.path(), &id, "two", &[]);
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert_eq!(json(&o.stdout)["ref"], format!("{id}@2"));
        let get = |t: &str| base(&db).args(["get", t]).output().unwrap();
        assert_eq!(get(&format!("{id}@1")).stdout, b"one");
        assert_eq!(get(&format!("{id}@2")).stdout, b"two");
        assert_eq!(get(&id).stdout, b"two");
        assert_eq!(get(&format!("{id}@3")).status.code(), Some(3));
    }

    #[test]
    fn stale_base_exits_six_naming_latest_and_author() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let id = submit_md(&db, dir.path(), "one");
        let o = edit_with(&db, dir.path(), &id, "two", &["--author", "alice"]);
        assert_eq!(o.status.code(), Some(0));
        let o = {
            let f = dir.path().join("edit-input.md");
            std::fs::write(&f, "three").unwrap();
            base(&db)
                .args(["edit", &id, "--base", "1"])
                .arg(&f)
                .output()
                .unwrap()
        };
        assert_eq!(o.status.code(), Some(6));
        let v = json(&o.stderr);
        assert_eq!(v["code"], "conflict");
        let m = v["message"].as_str().unwrap();
        assert!(
            m.contains("latest_revision is 2") && m.contains("alice"),
            "{m}"
        );
        let f = dir.path().join("edit-input.md");
        let o = base(&db)
            .args(["edit", &id, "--base", "2"])
            .arg(&f)
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(0));
    }

    #[test]
    fn unchanged_edit_shares_blob() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let id = submit_md(&db, dir.path(), "same");
        assert_eq!(
            edit_with(&db, dir.path(), &id, "same", &[]).status.code(),
            Some(0)
        );
        let conn = rusqlite::Connection::open(&db).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM blobs", [], |r| r.get(0))
            .unwrap();
        let r: i64 = conn
            .query_row("SELECT COUNT(*) FROM revisions", [], |r| r.get(0))
            .unwrap();
        assert_eq!((n, r), (1, 2));
    }

    #[test]
    fn concurrent_edits_get_consecutive_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let id = submit_md(&db, dir.path(), "zero");
        let n = 8;
        let children: Vec<_> = (0..n)
            .map(|i| {
                let f = dir.path().join(format!("c{i}.md"));
                std::fs::write(&f, format!("content {i}")).unwrap();
                base(&db)
                    .args(["edit", &id])
                    .arg(&f)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap()
            })
            .collect();
        let mut revs: Vec<i64> = children
            .into_iter()
            .map(|c| {
                let o = c.wait_with_output().unwrap();
                assert_eq!(
                    o.status.code(),
                    Some(0),
                    "{}",
                    String::from_utf8_lossy(&o.stderr)
                );
                json(&o.stdout)["revision"].as_i64().unwrap()
            })
            .collect();
        revs.sort();
        assert_eq!(revs, (2..=n as i64 + 1).collect::<Vec<_>>());
    }

    #[test]
    fn edit_unknown_or_deleted_artifact_exits_three() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let id = submit_md(&db, dir.path(), "one");
        assert_eq!(
            edit_with(&db, dir.path(), "nope", "x", &[]).status.code(),
            Some(3)
        );
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        conn.execute("DELETE FROM artifacts WHERE id = ?1", [&id])
            .unwrap();
        drop(conn);
        assert_eq!(
            edit_with(&db, dir.path(), &id, "x", &[]).status.code(),
            Some(3)
        );
    }

    #[test]
    fn edit_rejects_revision_suffix_and_oversize() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let id = submit_md(&db, dir.path(), "one");
        assert_eq!(
            edit_with(&db, dir.path(), &format!("{id}@1"), "x", &[])
                .status
                .code(),
            Some(2)
        );
        let big = "a".repeat(10 * 1024 * 1024 + 1);
        let o = edit_with(&db, dir.path(), &id, &big, &[]);
        assert_eq!(o.status.code(), Some(4));
        assert!(String::from_utf8_lossy(&o.stderr).contains("10 MiB"));
    }

    #[test]
    fn submit_stdin_without_name_is_usage_error() {
        let dir = tempfile::tempdir().unwrap();
        let o = base(&dir.path().join("c.db"))
            .args(["submit", "-", "--kind", "markdown"])
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&o.stderr).contains("--name"));
    }

    #[test]
    fn submit_missing_file_reports_read_error_not_no_file_name() {
        let dir = tempfile::tempdir().unwrap();
        let o = base(&dir.path().join("c.db"))
            .current_dir(dir.path())
            .args(["submit", "missing.md"])
            .output()
            .unwrap();
        assert_ne!(o.status.code(), Some(0));
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(!err.contains("no file name"), "{err}");
        assert!(err.contains("missing.md"), "{err}");
    }

    #[test]
    fn submit_dot_names_artifact_after_directory() {
        let dir = tempfile::tempdir().unwrap();
        let site = dir.path().join("mysite");
        std::fs::create_dir(&site).unwrap();
        std::fs::write(site.join("index.html"), "<p>hi</p>").unwrap();
        let db = dir.path().join("c.db");
        let o = base(&db)
            .current_dir(&site)
            .args(["submit", "."])
            .output()
            .unwrap();
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        let l = base(&db).arg("list").output().unwrap();
        assert!(String::from_utf8_lossy(&l.stdout).contains("\"name\":\"mysite\""));
    }
}

mod list_delete {
    use super::cairn;

    fn base(db: &std::path::Path) -> std::process::Command {
        let mut c = cairn();
        for v in [
            "CAIRN_ROLE",
            "CAIRN_AGENT",
            "CAIRN_SESSION",
            "CLAUDE_CODE_SESSION_ID",
            "CAIRN_DB",
            "CAIRN_PROJECT",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
        ] {
            c.env_remove(v);
        }
        // Not inside any repo, so detection finds no scope.
        c.current_dir(std::env::temp_dir());
        c.arg("--db").arg(db);
        c
    }

    fn json(b: &[u8]) -> serde_json::Value {
        serde_json::from_slice(b).unwrap()
    }

    fn submit(
        db: &std::path::Path,
        dir: &std::path::Path,
        name: &str,
        text: &[u8],
        extra: &[&str],
    ) -> String {
        let f = dir.join(name);
        std::fs::write(&f, text).unwrap();
        let o = base(db).args(extra).arg("submit").arg(&f).output().unwrap();
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        json(&o.stdout)["id"].as_str().unwrap().to_string()
    }

    fn edit(
        db: &std::path::Path,
        dir: &std::path::Path,
        id: &str,
        text: &str,
    ) -> std::process::Output {
        let f = dir.join("edit-input.md");
        std::fs::write(&f, text).unwrap();
        base(db).args(["edit", id]).arg(&f).output().unwrap()
    }

    fn list(db: &std::path::Path, args: &[&str]) -> Vec<serde_json::Value> {
        let o = base(db).args(["list"]).args(args).output().unwrap();
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        json(&o.stdout)["artifacts"].as_array().unwrap().clone()
    }

    fn ids(rows: &[serde_json::Value]) -> Vec<String> {
        rows.iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn newest_activity_first_with_full_row_shape() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let a = submit(&db, dir.path(), "a.md", b"aaa", &["--author", "bot"]);
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = submit(&db, dir.path(), "b.md", b"bb", &[]);
        assert_eq!(ids(&list(&db, &[])), vec![b.clone(), a.clone()]);
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(edit(&db, dir.path(), &a, "aaaa").status.code(), Some(0));
        let rows = list(&db, &[]);
        assert_eq!(ids(&rows), vec![a.clone(), b]);
        let r = &rows[0];
        assert_eq!(r["kind"], "markdown");
        assert_eq!(r["name"], "a.md");
        assert_eq!(r["latest_revision"], 2);
        assert_eq!(r["open_threads"], 0);
        assert_eq!(r["author"], "agent");
        assert_eq!(r["bytes"], 3 + 4);
        assert!(r["updated_at"].as_str().unwrap().ends_with('Z'));
        assert!(r["project"].is_null());
        assert!(r["worktree"].is_null());
        assert_eq!(list(&db, &["--limit", "1"]).len(), 1);
        assert_eq!(
            base(&db)
                .args(["list", "--limit", "0"])
                .output()
                .unwrap()
                .status
                .code(),
            Some(2)
        );
    }

    #[test]
    fn filters_kind_session_mine_open() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let a = submit(
            &db,
            dir.path(),
            "a.md",
            b"a",
            &["--session", "s1", "--author", "bot"],
        );
        let b = submit(
            &db,
            dir.path(),
            "b.md",
            b"b",
            &["--session", "s2", "--author", "other"],
        );
        assert_eq!(list(&db, &["--kind", "html"]).len(), 0);
        assert_eq!(list(&db, &["--kind", "markdown"]).len(), 2);
        let o = base(&db)
            .args(["--session", "s1", "list"])
            .output()
            .unwrap();
        assert_eq!(
            ids(json(&o.stdout)["artifacts"].as_array().unwrap()),
            vec![a.clone()]
        );
        // --mine with a session
        let o = base(&db)
            .args(["--session", "s2", "list", "--mine"])
            .output()
            .unwrap();
        assert_eq!(
            ids(json(&o.stdout)["artifacts"].as_array().unwrap()),
            vec![b]
        );
        // --mine without a session falls back to the author
        let o = base(&db)
            .args(["--author", "bot", "list", "--mine"])
            .output()
            .unwrap();
        assert_eq!(
            ids(json(&o.stdout)["artifacts"].as_array().unwrap()),
            vec![a]
        );
        // the fallback env session does not filter by itself
        let o = base(&db)
            .env("CLAUDE_CODE_SESSION_ID", "zzz")
            .arg("list")
            .output()
            .unwrap();
        assert_eq!(json(&o.stdout)["artifacts"].as_array().unwrap().len(), 2);
        // --open needs a Thread; none exist here
        assert_eq!(list(&db, &["--open"]).len(), 0);
    }

    #[test]
    fn session_and_mine_match_latest_revision_author() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let id = submit(
            &db,
            dir.path(),
            "a.md",
            b"one",
            &["--session", "s1", "--author", "A"],
        );
        let f = dir.path().join("e.md");
        std::fs::write(&f, "two").unwrap();
        let o = base(&db)
            .args(["--session", "s2", "--author", "B", "edit", &id])
            .arg(&f)
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(0));
        let ids_for = |args: &[&str]| {
            let o = base(&db).args(args).output().unwrap();
            ids(json(&o.stdout)["artifacts"].as_array().unwrap())
        };
        assert_eq!(
            ids_for(&["--session", "s2", "list", "--mine"]),
            vec![id.clone()]
        );
        assert_eq!(ids_for(&["--session", "s2", "list"]), vec![id.clone()]);
        assert_eq!(
            ids_for(&["--author", "B", "list", "--mine"]),
            vec![id.clone()]
        );
        assert!(ids_for(&["--session", "s1", "list", "--mine"]).is_empty());
        assert!(ids_for(&["--author", "A", "list", "--mine"]).is_empty());
        let rows = list(&db, &[]);
        assert_eq!(rows[0]["author"], "B");
    }

    #[test]
    fn older_than_filters_on_updated_at() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let old = submit(&db, dir.path(), "a.md", b"a", &[]);
        let new = submit(&db, dir.path(), "b.md", b"b", &[]);
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute(
            "UPDATE artifacts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now','-40 days') WHERE id = ?1",
            [&old],
        )
        .unwrap();
        drop(conn);
        assert_eq!(ids(&list(&db, &["--older-than", "30d"])), vec![old.clone()]);
        assert_eq!(list(&db, &["--older-than", "50d"]).len(), 0);
        assert_eq!(list(&db, &["--older-than", "1h"]).len(), 1);
        assert!(!ids(&list(&db, &["--older-than", "30d"])).contains(&new));
        for bad in ["30", "d", "5y", "-1d"] {
            let o = base(&db)
                .args(["list", "--older-than", bad])
                .output()
                .unwrap();
            assert_eq!(o.status.code(), Some(2), "{bad}");
        }
    }

    #[test]
    fn text_table_has_headers_and_dashes() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        submit(&db, dir.path(), "a.md", b"a", &[]);
        let o = base(&db).args(["list", "--text"]).output().unwrap();
        assert_eq!(o.status.code(), Some(0));
        let s = String::from_utf8(o.stdout).unwrap();
        let mut lines = s.lines();
        let h = lines.next().unwrap();
        assert!(h.starts_with("ID") && h.contains("KIND") && h.contains("WORKTREE"));
        assert!(lines.next().unwrap().contains("a.md"));
    }

    #[test]
    fn delete_without_yes_exits_two_and_keeps_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let a = submit(&db, dir.path(), "a.md", b"a", &[]);
        assert_eq!(
            base(&db)
                .args(["delete", &a])
                .output()
                .unwrap()
                .status
                .code(),
            Some(2)
        );
        assert_eq!(
            base(&db).args(["get", &a]).output().unwrap().status.code(),
            Some(0)
        );
        assert_eq!(
            base(&db)
                .args(["delete", "--yes"])
                .output()
                .unwrap()
                .status
                .code(),
            Some(2)
        );
    }

    #[test]
    fn delete_is_atomic_and_unknown_id_exits_three() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let a = submit(&db, dir.path(), "a.md", b"a", &[]);
        let b = submit(&db, dir.path(), "b.md", b"b", &[]);
        let o = base(&db)
            .args(["delete", &a, &b, "missing", "--yes"])
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(3));
        assert_eq!(list(&db, &[]).len(), 2);
        let o = base(&db)
            .args(["delete", &a, &b, &a, "--yes"])
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(0));
        assert_eq!(json(&o.stdout)["deleted"], serde_json::json!([a, b]));
        assert_eq!(list(&db, &[]).len(), 0);
        assert_eq!(
            base(&db).args(["get", &a]).output().unwrap().status.code(),
            Some(3)
        );
    }

    #[test]
    fn write_to_deleted_artifact_exits_three() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let a = submit(&db, dir.path(), "a.md", b"a", &[]);
        assert_eq!(
            base(&db)
                .args(["delete", &a, "--yes"])
                .output()
                .unwrap()
                .status
                .code(),
            Some(0)
        );
        assert_eq!(edit(&db, dir.path(), &a, "x").status.code(), Some(3));
    }

    fn blob_count(db: &std::path::Path) -> i64 {
        rusqlite::Connection::open(db)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM blobs", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn shared_blobs_survive_and_orphans_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let a = submit(&db, dir.path(), "a.md", b"same", &[]);
        let b = submit(&db, dir.path(), "b.md", b"same", &[]);
        assert_eq!(blob_count(&db), 1);
        base(&db).args(["delete", &a, "--yes"]).output().unwrap();
        assert_eq!(blob_count(&db), 1);
        assert_eq!(
            base(&db).args(["get", &b]).output().unwrap().stdout,
            b"same"
        );
        base(&db).args(["delete", &b, "--yes"]).output().unwrap();
        assert_eq!(blob_count(&db), 0);
    }

    #[test]
    fn delete_shrinks_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let a = submit(&db, dir.path(), "big.md", &vec![b'x'; 1024 * 1024], &[]);
        let size = |db: &std::path::Path| {
            let conn = rusqlite::Connection::open(db).unwrap();
            conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
                .unwrap();
            std::fs::metadata(db).unwrap().len()
        };
        let before = size(&db);
        assert!(before > 1024 * 1024);
        base(&db).args(["delete", &a, "--yes"]).output().unwrap();
        let after = size(&db);
        assert!(after < before / 2, "{before} -> {after}");
    }
}

mod sections {
    use super::cairn;
    use std::io::Write;
    use std::process::Stdio;

    fn base(db: &std::path::Path) -> std::process::Command {
        let mut c = cairn();
        for v in [
            "CAIRN_ROLE",
            "CAIRN_AGENT",
            "CAIRN_SESSION",
            "CLAUDE_CODE_SESSION_ID",
            "CAIRN_DB",
            "CAIRN_PROJECT",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
        ] {
            c.env_remove(v);
        }
        // Not inside any repo, so detection finds no scope.
        c.current_dir(std::env::temp_dir());
        c.arg("--db").arg(db);
        c
    }

    fn json(b: &[u8]) -> serde_json::Value {
        serde_json::from_slice(b).unwrap()
    }

    fn submit(
        db: &std::path::Path,
        dir: &std::path::Path,
        name: &str,
        text: &[u8],
        _extra: &[&str],
    ) -> String {
        let f = dir.join(name);
        std::fs::write(&f, text).unwrap();
        let o = base(db).arg("submit").arg(&f).output().unwrap();
        json(&o.stdout)["id"].as_str().unwrap().to_string()
    }

    const SECTIONED: &str = "---\ntitle: t\n---\n\npre\n\n# Top\n\nintro\n\n## Notes\n\none\n\n> ## Quoted\n\n## Other\n\no\n\n# Notes\n\ntwo\n";

    fn run(db: &std::path::Path, args: &[&str]) -> std::process::Output {
        base(db).args(args).output().unwrap()
    }

    fn rejected(out: &std::process::Output) -> String {
        assert_eq!(
            out.status.code(),
            Some(4),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        json(&out.stderr)["message"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn outline_lists_paths_and_sizes_without_ids() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let a = submit(&db, dir.path(), "d.md", SECTIONED.as_bytes(), &[]);
        let out = run(&db, &["outline", &a]);
        assert_eq!(out.status.code(), Some(0));
        let v = json(&out.stdout);
        let paths: Vec<&str> = v["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, ["Top", "Top > Notes", "Top > Other", "Notes#2"]);
        let e = &v["sections"][1];
        assert_eq!(e["level"], 2);
        assert_eq!(e["bytes"], "## Notes\n\none\n\n> ## Quoted\n\n".len());
        assert!(!String::from_utf8_lossy(&out.stdout).contains("\"id\""));
    }

    #[test]
    fn get_section_suffix_occurrence_and_revision() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let a = submit(&db, dir.path(), "d.md", SECTIONED.as_bytes(), &[]);
        let out = run(&db, &["get", &a, "--section", "Top > Other"]);
        assert_eq!(out.stdout, b"## Other\n\no\n\n");
        // Suffix "Notes" matches both; the error lists numbered candidates.
        let msg = rejected(&run(&db, &["get", &a, "--section", "Notes"]));
        assert!(
            msg.contains("Notes#1 = Top > Notes") && msg.contains("Notes#2 = Notes"),
            "{msg}"
        );
        // The printed form of the top-level one.
        let out = run(&db, &["get", &a, "--section", "Notes#2"]);
        assert_eq!(out.stdout, b"# Notes\n\ntwo\n");
        // Missing path.
        rejected(&run(&db, &["get", &a, "--section", "Nope"]));
        // From a given Revision.
        let new = "# Top\n\nchanged\n";
        let f = dir.path().join("n.md");
        std::fs::write(&f, new).unwrap();
        run(&db, &["edit", &a, f.to_str().unwrap()]);
        let out = run(&db, &["get", &format!("{a}@1"), "--section", "Top > Other"]);
        assert_eq!(out.stdout, b"## Other\n\no\n\n");
        rejected(&run(&db, &["get", &format!("{a}@2"), "--section", "Other"]));
    }

    fn edit_section(db: &std::path::Path, args: &[&str], body: &str) -> std::process::Output {
        let mut child = base(db)
            .args(["edit"])
            .args(args)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(body.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    #[test]
    fn edit_section_splices_into_a_new_revision() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let a = submit(&db, dir.path(), "d.md", SECTIONED.as_bytes(), &[]);
        let out = edit_section(&db, &[&a, "--section", "Top"], "# Top v2\n\nnew\n\n");
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(json(&out.stdout)["revision"], 2);
        let r2 = run(&db, &["get", &format!("{a}@2")]).stdout;
        let want = SECTIONED.replace(
            "# Top\n\nintro\n\n## Notes\n\none\n\n> ## Quoted\n\n## Other\n\no\n\n",
            "# Top v2\n\nnew\n\n",
        );
        assert_eq!(String::from_utf8(r2).unwrap(), want);
        assert_eq!(
            run(&db, &["get", &format!("{a}@1")]).stdout,
            SECTIONED.as_bytes()
        );
        // The path no longer matches in the latest Revision.
        rejected(&edit_section(&db, &[&a, "--section", "Top"], "# Top\n"));
        // Replacement must start with a heading.
        rejected(&edit_section(
            &db,
            &[&a, "--section", "Top v2"],
            "no heading\n",
        ));
        // Stale base still conflicts.
        let out = edit_section(&db, &[&a, "--section", "Top v2", "--base", "1"], "# X\n");
        assert_eq!(out.status.code(), Some(6));
        let out = edit_section(&db, &[&a, "--section", "Top v2", "--base", "2"], "# X\n");
        assert_eq!(out.status.code(), Some(0));
    }

    #[test]
    fn sections_on_non_markdown_exit_four() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let a = submit(&db, dir.path(), "d.md", SECTIONED.as_bytes(), &[]);
        rusqlite::Connection::open(&db)
            .unwrap()
            .execute("UPDATE artifacts SET kind = 'diagram' WHERE id = ?1", [&a])
            .unwrap();
        rejected(&run(&db, &["outline", &a]));
        rejected(&run(&db, &["get", &a, "--section", "Top"]));
        rejected(&edit_section(&db, &[&a, "--section", "Top"], "# Top\n"));
        assert_eq!(run(&db, &["get", &a]).status.code(), Some(0));
    }

    #[test]
    fn edit_section_on_crlf_document_keeps_crlf() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let doc = "# A\r\n\r\nx\r\n\r\n# B\r\n\r\ny\r\n";
        let a = submit(&db, dir.path(), "d.md", doc.as_bytes(), &[]);
        edit_section(&db, &[&a, "--section", "A"], "# A\nnew\n");
        let out = run(&db, &["get", &a]).stdout;
        assert_eq!(out, b"# A\r\nnew\r\n# B\r\n\r\ny\r\n");
    }
}

mod comments {
    use super::cairn;

    const DOC: &str = "# A\n\nalpha\n\n## B\n\nbeta\ngamma\n\n# C\n\ndelta\n";

    fn base(db: &std::path::Path) -> std::process::Command {
        let mut c = cairn();
        for v in [
            "CAIRN_ROLE",
            "CAIRN_AGENT",
            "CAIRN_SESSION",
            "CLAUDE_CODE_SESSION_ID",
            "CAIRN_DB",
            "CAIRN_PROJECT",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
        ] {
            c.env_remove(v);
        }
        // Not inside any repo, so detection finds no scope.
        c.current_dir(std::env::temp_dir());
        c.arg("--db").arg(db);
        c
    }

    fn run(db: &std::path::Path, args: &[&str]) -> std::process::Output {
        base(db).args(args).output().unwrap()
    }

    fn ok(db: &std::path::Path, args: &[&str]) -> serde_json::Value {
        let o = run(db, args);
        assert_eq!(
            o.status.code(),
            Some(0),
            "{args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    }

    fn code(db: &std::path::Path, args: &[&str]) -> Option<i32> {
        run(db, args).status.code()
    }

    fn submit(db: &std::path::Path, dir: &std::path::Path, text: &str) -> String {
        let f = dir.join(format!("d{}.md", text.len()));
        std::fs::write(&f, text).unwrap();
        let o = base(db).arg("submit").arg(&f).output().unwrap();
        let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        v["id"].as_str().unwrap().to_string()
    }

    fn edit(db: &std::path::Path, dir: &std::path::Path, id: &str, text: &str) {
        let f = dir.join("e.md");
        std::fs::write(&f, text).unwrap();
        let o = base(db).args(["edit", id]).arg(&f).output().unwrap();
        assert_eq!(o.status.code(), Some(0));
    }

    fn setup() -> (tempfile::TempDir, std::path::PathBuf, String) {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let id = submit(&db, dir.path(), DOC);
        (dir, db, id)
    }

    #[test]
    fn lines_store_quote_and_section_stores_path() {
        let (_d, db, id) = setup();
        let v = ok(&db, &["comment", "add", &id, "hm", "--lines", "7-8"]);
        assert_eq!(v["locator"]["quoted_text"], "beta\ngamma");
        assert_eq!(v["locator"]["line_start"], 7);
        let v = ok(&db, &["comment", "add", &id, "sec", "--section", "A > B"]);
        assert_eq!(v["locator"]["section_path"], "A > B");
        assert_eq!(v["locator"]["quoted_text"], "## B");
        assert_eq!(v["stale"], false);
        // bad ranges
        for r in ["0-2", "3-99", "5-2", "x"] {
            assert_eq!(
                code(&db, &["comment", "add", &id, "x", "--lines", r]),
                Some(2),
                "{r}"
            );
        }
        assert_eq!(
            code(&db, &["comment", "add", &id, "x", "--section", "Nope"]),
            Some(4)
        );
        // mixed locators are rejected
        assert_eq!(
            code(
                &db,
                &[
                    "comment",
                    "add",
                    &id,
                    "x",
                    "--section",
                    "A > B",
                    "--lines",
                    "7-8"
                ]
            ),
            Some(2)
        );
        assert_eq!(code(&db, &["comment", "add", "nope", "x"]), Some(3));
        assert_eq!(code(&db, &["comment", "add", &id, ""]), Some(2));
    }

    #[test]
    fn section_resolves_against_the_comments_own_revision() {
        let (d, db, id) = setup();
        edit(&db, d.path(), &id, &DOC.replace("## B", "## Renamed"));
        let target = format!("{id}@1");
        let v = ok(
            &db,
            &["comment", "add", &target, "old", "--section", "A > B"],
        );
        assert_eq!(v["locator"]["section_path"], "A > B");
        assert_eq!(v["stale"], true);
        assert_eq!(
            code(&db, &["comment", "add", &id, "new", "--section", "A > B"]),
            Some(4)
        );
    }

    #[test]
    fn non_text_kind_rejects_lines() {
        let (_d, db, id) = setup();
        rusqlite::Connection::open(&db)
            .unwrap()
            .execute("UPDATE artifacts SET kind = 'html' WHERE id = ?1", [&id])
            .unwrap();
        assert_eq!(
            code(&db, &["comment", "add", &id, "x", "--lines", "1"]),
            Some(4)
        );
        // a whole-Revision comment is fine
        ok(&db, &["comment", "add", &id, "whole"]);
    }

    #[test]
    fn reply_stays_on_thread_revision_and_rejects_locators() {
        let (d, db, id) = setup();
        let root = ok(&db, &["comment", "add", &id, "root", "--lines", "1"]);
        let rid = root["id"].as_i64().unwrap().to_string();
        edit(&db, d.path(), &id, "# new\n");
        edit(&db, d.path(), &id, "# newer\n");
        let r = ok(
            &db,
            &[
                "--role", "reviewer", "comment", "add", &id, "re", "--reply", &rid,
            ],
        );
        assert_eq!(r["revision"], 1);
        assert_eq!(r["latest_revision"], 3);
        assert_eq!(r["thread_id"], root["id"]);
        let rep = r["id"].as_i64().unwrap().to_string();
        assert_eq!(
            code(
                &db,
                &["comment", "add", &id, "x", "--reply", &rid, "--lines", "1"]
            ),
            Some(2)
        );
        assert_eq!(
            code(
                &db,
                &[
                    "comment",
                    "add",
                    &id,
                    "x",
                    "--reply",
                    &rid,
                    "--section",
                    "A"
                ]
            ),
            Some(2)
        );
        assert_eq!(
            code(&db, &["comment", "add", &id, "x", "--reply", &rep]),
            Some(2)
        );
        assert_eq!(
            code(&db, &["comment", "add", &id, "x", "--reply", "9999"]),
            Some(3)
        );
        let t = ok(&db, &["comment", "list", &id]);
        assert_eq!(t["threads"][0]["replies"].as_array().unwrap().len(), 1);
        assert_eq!(t["threads"][0]["replies"][0]["role"], "reviewer");
    }

    #[test]
    fn resolve_reopen_either_role_and_not_replies() {
        let (_d, db, id) = setup();
        let root = ok(&db, &["comment", "add", &id, "root"]);
        let rid = root["id"].as_i64().unwrap().to_string();
        let rep = ok(&db, &["comment", "add", &id, "r", "--reply", &rid]);
        let v = ok(&db, &["--role", "reviewer", "comment", "resolve", &rid]);
        assert_eq!(v["thread"]["status"], "resolved");
        assert!(v["highest_comment_id"].as_i64().unwrap() >= 2);
        assert_eq!(
            ok(&db, &["comment", "list"])["threads"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        let v = ok(&db, &["--role", "agent", "comment", "reopen", &rid]);
        assert_eq!(v["thread"]["status"], "open");
        ok(&db, &["--role", "agent", "comment", "resolve", &rid]);
        ok(&db, &["comment", "resolve", &rid]);
        let rep_id = rep["id"].as_i64().unwrap().to_string();
        assert_eq!(code(&db, &["comment", "resolve", &rep_id]), Some(2));
        assert_eq!(code(&db, &["comment", "reopen", "999"]), Some(3));
    }

    #[test]
    fn list_defaults_all_since_and_scoping() {
        let (d, db, id) = setup();
        let c1 = ok(&db, &["comment", "add", &id, "one"]);
        edit(&db, d.path(), &id, "# new\n");
        let c2 = ok(&db, &["comment", "add", &id, "two"]);
        let id1 = c1["id"].as_i64().unwrap();
        let id2 = c2["id"].as_i64().unwrap();
        // stale open thread is included by default
        let v = ok(&db, &["comment", "list"]);
        let ts = v["threads"].as_array().unwrap();
        assert_eq!(ts.len(), 2);
        assert_eq!(ts[0]["stale"], true);
        assert_eq!(ts[1]["stale"], false);
        assert_eq!(v["highest_comment_id"], id2);
        ok(&db, &["comment", "resolve", &id1.to_string()]);
        assert_eq!(
            ok(&db, &["comment", "list"])["threads"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            ok(&db, &["comment", "list", "--all"])["threads"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        // --since
        let v = ok(
            &db,
            &["comment", "list", "--all", "--since", &id1.to_string()],
        );
        assert_eq!(v["threads"].as_array().unwrap().len(), 1);
        assert_eq!(v["threads"][0]["id"], id2);
        let v = ok(&db, &["comment", "list", "--since", &id2.to_string()]);
        assert_eq!(v["threads"].as_array().unwrap().len(), 0);
        assert_eq!(v["highest_comment_id"], id2);
        // a new reply surfaces its (open) Thread, whole
        ok(
            &db,
            &["comment", "add", &id, "re", "--reply", &id2.to_string()],
        );
        let v = ok(&db, &["comment", "list", "--since", &id2.to_string()]);
        assert_eq!(v["threads"][0]["replies"].as_array().unwrap().len(), 1);
        // explicit id scoping and unknown id
        let other = submit(&db, d.path(), "# other\n");
        assert_eq!(
            ok(&db, &["comment", "list", &other])["threads"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            ok(&db, &["comment", "list", &id])["threads"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(code(&db, &["comment", "list", "nope"]), Some(3));
        assert_eq!(code(&db, &["comment", "list", &format!("{id}@1")]), Some(2));
        // session scoping: latest Revision's session
        assert_eq!(
            ok(&db, &["--session", "s9", "comment", "list"])["threads"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        let f = d.path().join("s.md");
        std::fs::write(&f, "# s\n").unwrap();
        base(&db)
            .args(["--session", "s9", "edit", &id])
            .arg(&f)
            .output()
            .unwrap();
        assert_eq!(
            ok(&db, &["--session", "s9", "comment", "list"])["threads"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        // another Session editing does not hide the Thread from s9
        base(&db)
            .args(["--session", "s8", "edit", &id])
            .arg(&f)
            .output()
            .unwrap();
        assert_eq!(
            ok(&db, &["--session", "s9", "comment", "list"])["threads"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        // the env fallback session does not filter
        let o = base(&db)
            .env("CLAUDE_CODE_SESSION_ID", "zzz")
            .args(["comment", "list"])
            .output()
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&o.stdout).unwrap()["threads"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        // --text renders
        let o = run(&db, &["comment", "list", "--text"]);
        assert!(String::from_utf8_lossy(&o.stdout).contains("highest_comment_id"));
    }

    #[test]
    fn highest_comment_id_is_monotonic_across_artifacts_and_deletes() {
        let (d, db, a) = setup();
        let b = submit(&db, d.path(), "# b\n");
        assert_eq!(ok(&db, &["comment", "list"])["highest_comment_id"], 0);
        assert_eq!(
            ok(&db, &["comment", "add", &a, "x"])["highest_comment_id"],
            1
        );
        assert_eq!(
            ok(&db, &["comment", "add", &b, "y"])["highest_comment_id"],
            2
        );
        ok(&db, &["delete", &b, "--yes"]);
        assert_eq!(ok(&db, &["comment", "list"])["highest_comment_id"], 2);
        let v = ok(&db, &["comment", "add", &a, "z"]);
        assert_eq!(v["id"], 3);
        assert_eq!(v["highest_comment_id"], 3);
    }

    #[test]
    fn list_open_thread_counts_and_delete_cascade() {
        let (_d, db, id) = setup();
        let t1 = ok(&db, &["comment", "add", &id, "one"]);
        ok(&db, &["comment", "add", &id, "two"]);
        ok(
            &db,
            &[
                "comment",
                "add",
                &id,
                "re",
                "--reply",
                &t1["id"].to_string(),
            ],
        );
        let rows = ok(&db, &["list"]);
        assert_eq!(rows["artifacts"][0]["open_threads"], 2);
        ok(&db, &["comment", "resolve", &t1["id"].to_string()]);
        assert_eq!(ok(&db, &["list"])["artifacts"][0]["open_threads"], 1);
        assert_eq!(
            ok(&db, &["list", "--open"])["artifacts"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        ok(&db, &["comment", "resolve", "2"]);
        assert_eq!(
            ok(&db, &["list", "--open"])["artifacts"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        ok(&db, &["delete", &id, "--yes"]);
        let n: i64 = rusqlite::Connection::open(&db)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM comments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn body_from_stdin() {
        use std::io::Write;
        use std::process::Stdio;
        let (_d, db, id) = setup();
        let mut c = base(&db)
            .args(["comment", "add", &id, "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        c.stdin.take().unwrap().write_all(b"from stdin").unwrap();
        assert_eq!(c.wait_with_output().unwrap().status.code(), Some(0));
        assert_eq!(
            ok(&db, &["comment", "list"])["threads"][0]["body"],
            "from stdin"
        );
    }

    fn highest(db: &std::path::Path) -> i64 {
        ok(db, &["comment", "list"])["highest_comment_id"]
            .as_i64()
            .unwrap()
    }

    #[test]
    fn wait_returns_when_a_comment_arrives_from_another_process() {
        use std::process::Stdio;
        let (_d, db, id) = setup();
        let start = std::time::Instant::now();
        let child = base(&db)
            .args(["comment", "list", &id, "--wait", "--timeout", "20"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        ok(
            &db,
            &["--role", "reviewer", "comment", "add", &id, "new one"],
        );
        let o = child.wait_with_output().unwrap();
        assert_eq!(o.status.code(), Some(0));
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v["threads"][0]["body"], "new one");
    }

    #[test]
    fn wait_with_max_u64_timeout_does_not_panic() {
        use std::process::Stdio;
        let (_d, db, id) = setup();
        let child = base(&db)
            .args([
                "comment",
                "list",
                &id,
                "--wait",
                "--timeout",
                "18446744073709551615",
            ])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        ok(
            &db,
            &["--role", "reviewer", "comment", "add", &id, "new one"],
        );
        let o = child.wait_with_output().unwrap();
        assert_eq!(o.status.code(), Some(0));
        let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v["threads"][0]["body"], "new one");
    }

    #[test]
    fn wait_timeout_exits_five_with_empty_result() {
        let (_d, db, id) = setup();
        ok(&db, &["comment", "add", &id, "old"]);
        let before = highest(&db);
        let o = run(&db, &["comment", "list", "--wait", "--timeout", "1"]);
        assert_eq!(o.status.code(), Some(5));
        assert!(o.stderr.is_empty());
        let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v["threads"].as_array().unwrap().len(), 0);
        assert_eq!(v["highest_comment_id"], before);
    }

    #[test]
    fn wait_with_satisfied_since_returns_at_once_and_unknown_artifact_is_3() {
        let (_d, db, id) = setup();
        ok(
            &db,
            &["--author", "someone-else", "comment", "add", &id, "a"],
        );
        let t = std::time::Instant::now();
        let o = run(&db, &["comment", "list", "--wait", "--since", "0"]);
        assert_eq!(o.status.code(), Some(0));
        assert!(t.elapsed() < std::time::Duration::from_secs(5));
        assert_eq!(
            code(
                &db,
                &["comment", "list", "nope", "--wait", "--timeout", "1"]
            ),
            Some(3)
        );
        assert_eq!(code(&db, &["comment", "list", "--timeout", "1"]), Some(2));
    }

    #[test]
    fn wait_ignores_own_addresses_replies_but_wakes_on_others() {
        let (d, db, id) = setup();
        let a = ok(
            &db,
            &[
                "--author", "rev", "--role", "reviewer", "comment", "add", &id, "fix",
            ],
        )["id"]
            .as_i64()
            .unwrap();
        let since = highest(&db).to_string();
        let f = d.path().join("n.md");
        std::fs::write(&f, "# new\n").unwrap();
        let o = base(&db)
            .args(["--author", "bot", "edit", &id])
            .arg(&f)
            .args(["--addresses", &a.to_string()])
            .output()
            .unwrap();
        assert!(o.status.success());
        let wait = |author: &str| {
            run(
                &db,
                &[
                    "--author",
                    author,
                    "comment",
                    "list",
                    &id,
                    "--wait",
                    "--since",
                    &since,
                    "--timeout",
                    "1",
                ],
            )
        };
        assert_eq!(wait("bot").status.code(), Some(5));
        // Someone else sees the reply as new.
        assert_eq!(wait("other").status.code(), Some(0));
        // A different author's later comment wakes the editor.
        ok(
            &db,
            &[
                "--author", "rev", "--role", "reviewer", "comment", "add", &id, "more",
            ],
        );
        assert_eq!(wait("bot").status.code(), Some(0));
    }

    fn thread_replies(db: &std::path::Path, id: &str) -> Vec<serde_json::Value> {
        ok(db, &["comment", "list", id, "--all"])["threads"]
            .as_array()
            .unwrap()
            .clone()
    }

    #[test]
    fn addresses_replies_in_each_thread_and_leaves_them_open() {
        let (d, db, id) = setup();
        let a = ok(&db, &["comment", "add", &id, "one"])["id"]
            .as_i64()
            .unwrap();
        let b = ok(&db, &["comment", "add", &id, "two", "--section", "A > B"])["id"]
            .as_i64()
            .unwrap();
        let f = d.path().join("n.md");
        std::fs::write(&f, "# new\n").unwrap();
        let list = format!("{a},{b},{a}");
        let o = base(&db)
            .args(["--author", "bot", "edit", &id])
            .arg(&f)
            .args(["--addresses", &list])
            .output()
            .unwrap();
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        for t in thread_replies(&db, &id) {
            let r = t["replies"].as_array().unwrap();
            assert_eq!(r.len(), 1);
            assert_eq!(r[0]["body"], format!("addressed in {id}@2"));
            assert_eq!(r[0]["author"], "bot");
            assert_eq!(r[0]["role"], "agent");
            assert_eq!(t["status"], "open");
        }
        // with --section: the section was replaced, so use a fresh doc
        let (d2, db2, id2) = setup();
        let c = ok(&db2, &["comment", "add", &id2, "x"])["id"]
            .as_i64()
            .unwrap();
        let f2 = d2.path().join("s.md");
        std::fs::write(&f2, "## B\nnew body\n").unwrap();
        let o = base(&db2)
            .args(["edit", &id2, "--section", "A > B"])
            .arg(&f2)
            .args(["--addresses", &c.to_string()])
            .output()
            .unwrap();
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert_eq!(
            thread_replies(&db2, &id2)[0]["replies"][0]["body"],
            format!("addressed in {id2}@2")
        );
    }

    #[test]
    fn bad_addresses_create_no_revision() {
        let (d, db, id) = setup();
        let root = ok(&db, &["comment", "add", &id, "r"])["id"]
            .as_i64()
            .unwrap();
        let reply = ok(
            &db,
            &["comment", "add", &id, "re", "--reply", &root.to_string()],
        )["id"]
            .as_i64()
            .unwrap();
        let f = d.path().join("n.md");
        std::fs::write(&f, "# new\n").unwrap();
        // a comment of another Artifact in the same database
        let other = submit(&db, d.path(), "# another one\n");
        let foreign_here = ok(&db, &["comment", "add", &other, "f"])["id"]
            .as_i64()
            .unwrap();
        for (ids, want) in [
            (format!("{root},9999"), 3),
            (reply.to_string(), 2),
            (foreign_here.to_string(), 2),
        ] {
            let o = base(&db)
                .args(["edit", &id])
                .arg(&f)
                .args(["--addresses", &ids])
                .output()
                .unwrap();
            assert_eq!(o.status.code(), Some(want), "{ids}");
            let out = base(&db)
                .args(["get", &format!("{id}@2")])
                .output()
                .unwrap();
            assert_eq!(
                out.status.code(),
                Some(3),
                "{ids}: revision 2 must not exist"
            );
            assert!(
                thread_replies(&db, &id)[0]["replies"]
                    .as_array()
                    .unwrap()
                    .len()
                    == 1
            );
        }
    }
}

mod scoping {
    use super::cairn;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args([
                "-c",
                "protocol.file.allow=always",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A temp dir with a canonical path (rev-parse reports canonical paths).
    fn tmp() -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().canonicalize().unwrap();
        (d, p)
    }

    /// A repo with one commit at `<parent>/<name>`.
    fn repo(parent: &Path, name: &str) -> PathBuf {
        let p = parent.join(name);
        std::fs::create_dir_all(&p).unwrap();
        git(&p, &["init", "-q"]);
        git(&p, &["commit", "-q", "--allow-empty", "-m", "init"]);
        p
    }

    fn worktree(root: &Path, path: &Path) {
        git(root, &["worktree", "add", "-q", path.to_str().unwrap()]);
    }

    fn cmd(db: &Path, cwd: &Path) -> Command {
        let mut c = cairn();
        for v in [
            "CAIRN_ROLE",
            "CAIRN_AGENT",
            "CAIRN_SESSION",
            "CLAUDE_CODE_SESSION_ID",
            "CAIRN_DB",
            "CAIRN_PROJECT",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
        ] {
            c.env_remove(v);
        }
        c.arg("--db").arg(db).current_dir(cwd);
        c
    }

    fn run(db: &Path, cwd: &Path, args: &[&str]) -> std::process::Output {
        cmd(db, cwd).args(args).output().unwrap()
    }

    fn ok(db: &Path, cwd: &Path, args: &[&str]) -> serde_json::Value {
        let o = run(db, cwd, args);
        assert_eq!(
            o.status.code(),
            Some(0),
            "{args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    }

    /// Submit a one-line doc named `name` from `cwd`; returns its id.
    fn submit(db: &Path, cwd: &Path, name: &str, extra: &[&str]) -> String {
        let file = db.parent().unwrap().join(format!("{name}.md"));
        std::fs::write(&file, "# T\n\nbody\n").unwrap();
        let mut args = vec!["submit", file.to_str().unwrap()];
        args.extend_from_slice(extra);
        ok(db, cwd, &args)["id"].as_str().unwrap().to_string()
    }

    fn names(v: &serde_json::Value) -> Vec<String> {
        let mut n: Vec<String> = v["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["name"].as_str().unwrap().to_string())
            .collect();
        n.sort();
        n
    }

    fn list(db: &Path, cwd: &Path, extra: &[&str]) -> Vec<String> {
        let mut args = vec!["list"];
        args.extend_from_slice(extra);
        names(&ok(db, cwd, &args))
    }

    #[test]
    fn submit_stores_name_root_and_worktree() {
        let (_g, t) = tmp();
        let db = t.join("c.db");
        let root = repo(&t, "proj");
        let wt = t.join("wt1");
        worktree(&root, &wt);

        submit(&db, &root, "in-root", &[]);
        submit(&db, &wt, "in-wt", &[]);

        let v = ok(&db, &root, &["list", "--all-worktrees"]);
        let rows = v["artifacts"].as_array().unwrap();
        let find = |n: &str| {
            rows.iter()
                .find(|r| r["name"] == format!("{n}.md"))
                .unwrap()
        };
        let r = find("in-root");
        assert_eq!(r["project"], "proj");
        assert_eq!(r["project_root"], root.to_str().unwrap());
        assert!(r["worktree"].is_null());
        let w = find("in-wt");
        assert_eq!(w["project"], "proj");
        assert_eq!(w["project_root"], root.to_str().unwrap());
        assert_eq!(w["worktree"], wt.to_str().unwrap());
    }

    #[test]
    fn default_scope_excludes_sibling_worktrees_and_flags_widen() {
        let (_g, t) = tmp();
        let db = t.join("c.db");
        let root = repo(&t, "proj");
        let (wa, wb) = (t.join("wa"), t.join("wb"));
        worktree(&root, &wa);
        worktree(&root, &wb);
        let other = repo(&t, "other");

        submit(&db, &root, "r", &[]);
        submit(&db, &wa, "a", &[]);
        submit(&db, &wb, "b", &[]);
        submit(&db, &other, "o", &[]);

        // Root checkout: root Artifacts only.
        assert_eq!(list(&db, &root, &[]), ["r.md"]);
        // A linked worktree: root plus its own.
        assert_eq!(list(&db, &wa, &[]), ["a.md", "r.md"]);
        assert_eq!(list(&db, &wb, &[]), ["b.md", "r.md"]);
        // --all-worktrees: every worktree of this Project, not other Projects.
        assert_eq!(
            list(&db, &root, &["--all-worktrees"]),
            ["a.md", "b.md", "r.md"]
        );
        // --all-projects: other Projects, still strict on worktrees.
        assert_eq!(list(&db, &root, &["--all-projects"]), ["o.md", "r.md"]);
        assert_eq!(
            list(&db, &root, &["--all-projects", "--all-worktrees"]),
            ["a.md", "b.md", "o.md", "r.md"]
        );
        // Outside any repo nothing is filtered.
        assert_eq!(list(&db, &t, &[]).len(), 4);
        // The two widening/narrowing project flags conflict.
        let o = run(
            &db,
            &root,
            &["list", "--all-projects", "--project-root", "/x"],
        );
        assert_eq!(o.status.code(), Some(2));
    }

    #[test]
    fn relative_project_is_a_path_and_comment_list_rejects_ignored_scope_flags() {
        let (_g, t) = tmp();
        let db = t.join("c.db");
        let root = repo(&t, "proj");
        // `--project .` is the cwd's root, not a name.
        let id = submit(&db, &root, "d", &["--project", "."]);
        let v = ok(&db, &root, &["list", "--project", "."]);
        assert_eq!(names(&v), ["d.md"]);
        let v = ok(&db, &t, &["list", "--project", "./proj"]);
        assert_eq!(names(&v), ["d.md"]);

        // Scope flags that an explicit Artifact or Session would ignore are errors.
        for args in [
            vec!["comment", "list", &id, "--all-projects"],
            vec!["comment", "list", &id, "--all-worktrees"],
            vec!["--session", "s", "comment", "list", "--project-root", "x"],
        ] {
            let o = run(&db, &root, &args);
            assert_eq!(o.status.code(), Some(2), "{args:?}");
        }
    }

    #[test]
    fn name_matching_several_roots_is_narrowed_by_project_root() {
        let (_g, t) = tmp();
        let db = t.join("c.db");
        let one = repo(&t.join("x"), "same");
        let two = repo(&t.join("y"), "same");
        submit(&db, &one, "one", &[]);
        submit(&db, &two, "two", &[]);

        // From outside any repo, a name reads every Project of that name.
        assert_eq!(list(&db, &t, &["--project", "same"]), ["one.md", "two.md"]);
        assert_eq!(
            list(
                &db,
                &t,
                &["--project", "same", "--project-root", one.to_str().unwrap()]
            ),
            ["one.md"]
        );
        // A detected scope narrows too.
        assert_eq!(
            list(&db, &two, &["--project-root", one.to_str().unwrap()]),
            ["one.md"]
        );
        assert_eq!(list(&db, &t, &["--project", "nothing"]).len(), 0);
    }

    #[test]
    fn name_on_write_is_rejected_and_path_replaces_the_root() {
        let (_g, t) = tmp();
        let db = t.join("c.db");
        let root = repo(&t, "proj");
        let f = t.join("d.md");
        std::fs::write(&f, "# T\n").unwrap();

        let o = run(
            &db,
            &root,
            &["--project", "proj", "submit", f.to_str().unwrap()],
        );
        assert_eq!(o.status.code(), Some(2));

        let target = t.join("elsewhere");
        std::fs::create_dir(&target).unwrap();
        let o = run(
            &db,
            &root,
            &[
                "--project",
                target.to_str().unwrap(),
                "submit",
                f.to_str().unwrap(),
            ],
        );
        assert_eq!(o.status.code(), Some(0));
        let v = ok(&db, &t, &["list", "--project", "none", "--all-worktrees"]);
        let row = &v["artifacts"][0];
        assert_eq!(row["project"], "elsewhere");
        assert_eq!(row["project_root"], target.to_str().unwrap());
        // Reading that root back finds it, from anywhere.
        assert_eq!(
            list(&db, &t, &["--project", target.to_str().unwrap()]),
            ["d.md"]
        );
        // "none" and the empty string disable detection on a write.
        for off in ["none", ""] {
            let o = run(
                &db,
                &root,
                &["--project", off, "submit", f.to_str().unwrap()],
            );
            assert_eq!(o.status.code(), Some(0));
        }
        let v = ok(&db, &t, &["list", "--all-worktrees"]);
        let nulls = v["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["project"].is_null())
            .count();
        assert_eq!(nulls, 2);
    }

    #[test]
    fn no_repo_missing_git_bare_repo_and_submodule() {
        let (_g, t) = tmp();
        let db = t.join("c.db");
        let plain = t.join("plain");
        std::fs::create_dir(&plain).unwrap();

        // No repo: succeeds, no scope.
        submit(&db, &plain, "plain", &[]);
        let v = ok(&db, &plain, &["list"]);
        assert!(v["artifacts"][0]["project"].is_null());

        // git missing from PATH.
        let empty = t.join("emptybin");
        std::fs::create_dir(&empty).unwrap();
        let root = repo(&t, "proj");
        let f = t.join("g.md");
        std::fs::write(&f, "# T\n").unwrap();
        let o = cmd(&db, &root)
            .env("PATH", &empty)
            .args(["submit", f.to_str().unwrap()])
            .output()
            .unwrap();
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        let o = cmd(&db, &root)
            .env("PATH", &empty)
            .arg("list")
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(0));

        // Bare repo: no scope, no error.
        let bare = t.join("bare.git");
        std::fs::create_dir(&bare).unwrap();
        git(&bare, &["init", "-q", "--bare"]);
        let id = submit(&db, &bare, "inbare", &[]);
        let v = ok(&db, &bare, &["list"]);
        assert!(
            v["artifacts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"] == id.as_str() && a["project"].is_null())
        );

        // A submodule is its own Project.
        let lib = repo(&t, "lib");
        let sup = repo(&t, "super");
        git(
            &sup,
            &["submodule", "add", "-q", lib.to_str().unwrap(), "lib"],
        );
        let sub = sup.join("lib");
        submit(&db, &sub, "insub", &[]);
        let v = ok(&db, &sub, &["list"]);
        let row = &v["artifacts"][0];
        assert_eq!(row["name"], "insub.md");
        assert_eq!(row["project"], "lib");
        assert_eq!(row["project_root"], sub.to_str().unwrap());
        assert!(row["worktree"].is_null());
    }

    #[test]
    fn comment_list_scope() {
        let (_g, t) = tmp();
        let db = t.join("c.db");
        let root = repo(&t, "proj");
        let wt = t.join("wt");
        worktree(&root, &wt);
        let other = repo(&t, "other");

        let a = submit(&db, &root, "r", &["--session", "s1"]);
        let b = submit(&db, &wt, "w", &["--session", "s2"]);
        let c = submit(&db, &other, "o", &["--session", "s3"]);
        for id in [&a, &b, &c] {
            ok(&db, &t, &["comment", "add", id, "hello"]);
        }
        let ids = |v: &serde_json::Value| -> Vec<String> {
            let mut n: Vec<String> = v["threads"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x["artifact_id"].as_str().unwrap().to_string())
                .collect();
            n.sort();
            n
        };
        let sorted = |mut v: Vec<String>| {
            v.sort();
            v
        };
        // No session: the same scope as `list`.
        assert_eq!(
            ids(&ok(&db, &root, &["comment", "list"])),
            std::slice::from_ref(&a)
        );
        assert_eq!(
            ids(&ok(&db, &wt, &["comment", "list"])),
            sorted(vec![a.clone(), b.clone()])
        );
        assert_eq!(
            ids(&ok(&db, &root, &["comment", "list", "--all-worktrees"])),
            sorted(vec![a.clone(), b.clone()])
        );
        assert_eq!(
            ids(&ok(&db, &root, &["comment", "list", "--all-projects"])),
            sorted(vec![a.clone(), c.clone()])
        );
        // An explicit Session ignores the Project.
        assert_eq!(
            ids(&ok(&db, &root, &["--session", "s3", "comment", "list"])),
            std::slice::from_ref(&c)
        );
        // So does an explicit Artifact.
        assert_eq!(ids(&ok(&db, &root, &["comment", "list", &c])), [c]);
    }

    #[test]
    fn list_text_has_project_and_worktree_columns() {
        let (_g, t) = tmp();
        let db = t.join("c.db");
        let root = repo(&t, "proj");
        let wt = t.join("wt");
        worktree(&root, &wt);
        submit(&db, &wt, "w", &[]);
        let o = run(&db, &wt, &["list", "--text"]);
        let text = String::from_utf8(o.stdout).unwrap();
        let mut lines = text.lines();
        let header = lines.next().unwrap();
        assert!(
            header.contains("PROJECT") && header.contains("WORKTREE"),
            "{header}"
        );
        let row = lines.next().unwrap();
        assert!(
            row.contains("proj") && row.contains(wt.to_str().unwrap()),
            "{row}"
        );
    }
}

mod bundles {
    use super::cairn;
    use std::path::{Path, PathBuf};

    fn run(db: &Path, args: &[&str]) -> std::process::Output {
        let mut c = cairn();
        for v in [
            "CAIRN_ROLE",
            "CAIRN_AGENT",
            "CAIRN_SESSION",
            "CLAUDE_CODE_SESSION_ID",
            "CAIRN_DB",
            "CAIRN_PROJECT",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
        ] {
            c.env_remove(v);
        }
        c.current_dir(std::env::temp_dir());
        c.arg("--db").arg(db).args(args);
        c.output().unwrap()
    }

    fn code(db: &Path, args: &[&str]) -> Option<i32> {
        run(db, args).status.code()
    }

    fn ok(db: &Path, args: &[&str]) -> serde_json::Value {
        let o = run(db, args);
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    }

    fn site(root: &Path) -> PathBuf {
        let s = root.join("site");
        std::fs::create_dir_all(s.join("css")).unwrap();
        std::fs::write(s.join("index.html"), "<h1>hi</h1>").unwrap();
        std::fs::write(s.join("css/a.css"), "body{}").unwrap();
        std::fs::write(s.join("logo.png"), [0x89, b'P', b'N', b'G', 1, 2, 3]).unwrap();
        s
    }

    fn blobs(db: &Path) -> i64 {
        rusqlite::Connection::open(db)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM blobs", [], |r| r.get(0))
            .unwrap()
    }

    fn s(p: &Path) -> &str {
        p.to_str().unwrap()
    }

    #[test]
    fn directory_without_entry_page_exits_four() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("c.db");
        let dir = d.path().join("nosite");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("page.html"), "x").unwrap();
        assert_eq!(code(&db, &["submit", s(&dir)]), Some(4));
    }

    #[test]
    fn too_many_files_and_oversize_name_the_limit() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("c.db");
        let dir = d.path().join("many");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "x").unwrap();
        for i in 0..200 {
            std::fs::write(dir.join(format!("f{i}.txt")), "x").unwrap();
        }
        let o = run(&db, &["submit", s(&dir)]);
        assert_eq!(o.status.code(), Some(4));
        assert!(String::from_utf8_lossy(&o.stderr).contains("200 files"));

        let big = d.path().join("big");
        std::fs::create_dir(&big).unwrap();
        std::fs::write(big.join("index.html"), "x").unwrap();
        for i in 0..3 {
            std::fs::write(big.join(format!("b{i}.txt")), vec![b'a'; 9 * 1024 * 1024]).unwrap();
        }
        let o = run(&db, &["submit", s(&big)]);
        assert_eq!(o.status.code(), Some(4));
        assert!(String::from_utf8_lossy(&o.stderr).contains("25 MiB"));
    }

    #[test]
    fn svg_and_disallowed_types_are_rejected() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("c.db");
        let svg = d.path().join("x.svg");
        std::fs::write(&svg, "<svg/>").unwrap();
        assert_eq!(code(&db, &["submit", s(&svg)]), Some(4));
        assert_eq!(code(&db, &["submit", "--kind", "image", s(&svg)]), Some(4));
        let dir = site(d.path());
        std::fs::write(dir.join("x.exe"), "MZ").unwrap();
        assert_eq!(code(&db, &["submit", s(&dir)]), Some(4));
        std::fs::remove_file(dir.join("x.exe")).unwrap();
        assert_eq!(code(&db, &["submit", s(&dir)]), Some(0));
    }

    #[test]
    #[cfg(unix)]
    fn symlinks_are_rejected() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("c.db");
        let dir = site(d.path());
        std::os::unix::fs::symlink("/etc/hostname", dir.join("link.txt")).unwrap();
        assert_eq!(code(&db, &["submit", s(&dir)]), Some(4));
    }

    #[test]
    fn get_needs_out_and_writes_files() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("c.db");
        let dir = site(d.path());
        let r = ok(&db, &["submit", s(&dir)]);
        let id = r["id"].as_str().unwrap();
        assert_eq!(code(&db, &["get", id]), Some(2));
        let out = d.path().join("out");
        let l = ok(&db, &["get", id, "--out", s(&out)]);
        let paths: Vec<&str> = l["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, ["css/a.css", "index.html", "logo.png"]);
        assert_eq!(l["files"][0]["media_type"], "text/css");
        assert_eq!(
            std::fs::read(out.join("logo.png")).unwrap(),
            std::fs::read(dir.join("logo.png")).unwrap()
        );
        assert_eq!(
            std::fs::read_to_string(out.join("css/a.css")).unwrap(),
            "body{}"
        );
    }

    fn submit_site(d: &Path, db: &Path) -> String {
        let dir = site(d);
        ok(db, &["submit", s(&dir)])["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[cfg(unix)]
    #[test]
    fn get_out_refuses_symlinked_file() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("c.db");
        let id = submit_site(d.path(), &db);
        let target = d.path().join("victim.txt");
        std::fs::write(&target, "precious").unwrap();
        let out = d.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        std::os::unix::fs::symlink(&target, out.join("index.html")).unwrap();
        assert_eq!(code(&db, &["get", &id, "--out", s(&out)]), Some(1));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "precious");
        assert!(!out.join("logo.png").exists());
    }

    #[cfg(unix)]
    #[test]
    fn get_out_refuses_symlinked_directory() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("c.db");
        let id = submit_site(d.path(), &db);
        let elsewhere = d.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let out = d.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        std::os::unix::fs::symlink(&elsewhere, out.join("css")).unwrap();
        assert_eq!(code(&db, &["get", &id, "--out", s(&out)]), Some(1));
        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
        assert!(!out.join("index.html").exists());
    }

    #[cfg(unix)]
    #[test]
    fn get_out_allows_symlinked_out_dir() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("c.db");
        let id = submit_site(d.path(), &db);
        let real = d.path().join("real");
        std::fs::create_dir_all(&real).unwrap();
        let out = d.path().join("out");
        std::os::unix::fs::symlink(&real, &out).unwrap();
        ok(&db, &["get", &id, "--out", s(&out)]);
        assert!(real.join("index.html").exists());
    }

    #[test]
    fn get_out_file_dir_clash_in_revision_writes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("c.db");
        let id = submit_site(d.path(), &db);
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute(
            "INSERT INTO revision_files SELECT artifact_id, revision, 'css', media_type, size, sha256 \
             FROM revision_files WHERE path = 'index.html'",
            [],
        )
        .unwrap();
        drop(conn);
        let out = d.path().join("out");
        assert_eq!(code(&db, &["get", &id, "--out", s(&out)]), Some(1));
        assert!(!out.exists() || std::fs::read_dir(&out).unwrap().count() == 0);
    }

    #[test]
    fn get_out_overwrites_existing_regular_file() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("c.db");
        let id = submit_site(d.path(), &db);
        let out = d.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("index.html"), "old").unwrap();
        ok(&db, &["get", &id, "--out", s(&out)]);
        assert_eq!(
            std::fs::read_to_string(out.join("index.html")).unwrap(),
            "<h1>hi</h1>"
        );
    }

    #[test]
    fn get_out_refuses_existing_directory_where_file_goes() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("c.db");
        let id = submit_site(d.path(), &db);
        let out = d.path().join("out");
        std::fs::create_dir_all(out.join("index.html")).unwrap();
        assert_eq!(code(&db, &["get", &id, "--out", s(&out)]), Some(1));
        assert!(!out.join("logo.png").exists());
    }

    #[test]
    fn single_html_is_stored_as_index_and_image_roundtrips() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("c.db");
        let h = d.path().join("page.html");
        std::fs::write(&h, "<p>x").unwrap();
        let id = ok(&db, &["submit", s(&h)])["id"]
            .as_str()
            .unwrap()
            .to_string();
        let out = d.path().join("o1");
        let l = ok(&db, &["get", &id, "--out", s(&out)]);
        assert_eq!(l["files"][0]["path"], "index.html");
        assert!(out.join("index.html").exists());

        let png = d.path().join("p.png");
        std::fs::write(&png, [1u8, 2, 3]).unwrap();
        let id = ok(&db, &["submit", s(&png)])["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(code(&db, &["get", &id]), Some(2));
        let out = d.path().join("o2");
        let l = ok(&db, &["get", &id, "--out", s(&out)]);
        assert_eq!(l["files"][0]["media_type"], "image/png");
        assert_eq!(std::fs::read(out.join("p.png")).unwrap(), [1, 2, 3]);
        // --out is not for markdown
        let md = d.path().join("a.md");
        std::fs::write(&md, "# x\n").unwrap();
        let mid = ok(&db, &["submit", s(&md)])["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(code(&db, &["get", &mid, "--out", s(&out)]), Some(2));
    }

    #[test]
    fn editing_a_bundle_with_one_changed_file_stores_one_blob() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("c.db");
        let dir = site(d.path());
        let id = ok(&db, &["submit", s(&dir)])["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(blobs(&db), 3);
        std::fs::write(dir.join("css/a.css"), "body{color:red}").unwrap();
        let r = ok(&db, &["edit", &id, s(&dir)]);
        assert_eq!(r["revision"], 2);
        assert_eq!(blobs(&db), 4);
        let n: i64 = rusqlite::Connection::open(&db)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM revision_files WHERE revision = 2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 3);
        // a markdown Artifact cannot be edited with a directory
        let md = d.path().join("a.md");
        std::fs::write(&md, "# x\n").unwrap();
        let mid = ok(&db, &["submit", s(&md)])["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(code(&db, &["edit", &mid, s(&dir)]), Some(4));
    }
}

mod diagrams {
    use super::cairn;
    use std::path::Path;

    const FLOW: &str = "flowchart LR\n A[Start] --> B{Ok?}\n B -->|yes| C[Done]\n";
    const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\"><script>x</script></svg>";

    fn run(db: &Path, args: &[&str]) -> std::process::Output {
        let mut c = cairn();
        for v in [
            "CAIRN_ROLE",
            "CAIRN_AGENT",
            "CAIRN_SESSION",
            "CLAUDE_CODE_SESSION_ID",
            "CAIRN_DB",
            "CAIRN_PROJECT",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
        ] {
            c.env_remove(v);
        }
        c.current_dir(std::env::temp_dir());
        c.arg("--db").arg(db).args(args);
        c.output().unwrap()
    }

    fn id_of(o: &std::process::Output) -> String {
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        v["id"].as_str().unwrap().to_string()
    }

    fn file(dir: &Path, name: &str, text: &str) -> String {
        let p = dir.join(name);
        std::fs::write(&p, text).unwrap();
        p.to_str().unwrap().to_string()
    }

    #[test]
    fn mermaid_submit_get_prints_source_and_list_shows_diagram() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let f = file(dir.path(), "flow.mmd", FLOW);
        let id = id_of(&run(&db, &["submit", &f]));
        let g = run(&db, &["get", &id]);
        assert_eq!(g.status.code(), Some(0));
        assert_eq!(g.stdout, FLOW.as_bytes());
        let l = run(&db, &["list"]);
        let v: serde_json::Value = serde_json::from_slice(&l.stdout).unwrap();
        assert_eq!(v["artifacts"][0]["kind"], "diagram");
    }

    #[test]
    fn invalid_mermaid_exits_four_with_pele_code_and_message() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let f = file(dir.path(), "bad.mmd", "flowchart LR\n A -->\n");
        let o = run(&db, &["submit", &f]);
        assert_eq!(o.status.code(), Some(4));
        let v: serde_json::Value = serde_json::from_slice(&o.stderr).unwrap();
        assert_eq!(v["code"], "rejected_input");
        assert!(v["message"].as_str().unwrap().starts_with("mermaid: "));
        let l = run(&db, &["list"]);
        let v: serde_json::Value = serde_json::from_slice(&l.stdout).unwrap();
        assert!(v["artifacts"].as_array().unwrap().is_empty());
    }

    #[test]
    fn invalid_mermaid_edit_exits_four_and_adds_no_revision() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let id = id_of(&run(&db, &["submit", &file(dir.path(), "a.mmd", FLOW)]));
        let bad = file(dir.path(), "bad.mmd", "flowchart LR\n A -->\n");
        assert_eq!(run(&db, &["edit", &id, &bad]).status.code(), Some(4));
        assert_eq!(
            run(&db, &["get", &format!("{id}@2")]).status.code(),
            Some(3)
        );
        let good = file(dir.path(), "b.mmd", "flowchart TD\n X --> Y\n");
        assert_eq!(run(&db, &["edit", &id, &good]).status.code(), Some(0));
    }

    #[test]
    fn svg_roundtrips_unsanitised_and_malformed_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let f = file(dir.path(), "d.svg", SVG);
        let id = id_of(&run(&db, &["submit", &f, "--format", "svg"]));
        assert_eq!(run(&db, &["get", &id]).stdout, SVG.as_bytes());
        let bad = file(dir.path(), "bad.svg", "<svg><g></svg>");
        assert_eq!(
            run(&db, &["submit", &bad, "--format", "svg"]).status.code(),
            Some(4)
        );
        assert_eq!(run(&db, &["edit", &id, &bad]).status.code(), Some(4));
    }

    #[test]
    fn svg_without_format_and_format_on_other_kinds_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let f = file(dir.path(), "d.svg", SVG);
        assert_eq!(run(&db, &["submit", &f]).status.code(), Some(4));
        assert_eq!(
            run(&db, &["submit", &f, "--kind", "diagram"]).status.code(),
            Some(4)
        );
        let md = file(dir.path(), "a.md", "# x\n");
        assert_eq!(
            run(&db, &["submit", &md, "--format", "svg"]).status.code(),
            Some(4)
        );
        assert_eq!(
            run(
                &db,
                &["submit", &md, "--kind", "markdown", "--format", "svg"]
            )
            .status
            .code(),
            Some(4)
        );
    }

    #[test]
    fn diagrams_have_no_sections_or_out() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        let id = id_of(&run(&db, &["submit", &file(dir.path(), "a.mmd", FLOW)]));
        assert_eq!(run(&db, &["outline", &id]).status.code(), Some(4));
        let out = dir.path().join("o");
        assert_eq!(
            run(&db, &["get", &id, "--out", out.to_str().unwrap()])
                .status
                .code(),
            Some(2)
        );
    }
}
