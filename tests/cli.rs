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
        ] {
            c.env_remove(v);
        }
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
        ] {
            c.env_remove(v);
        }
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
        ] {
            c.env_remove(v);
        }
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
        ] {
            c.env_remove(v);
        }
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
}
