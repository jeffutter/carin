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
}
