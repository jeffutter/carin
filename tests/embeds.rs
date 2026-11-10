use std::path::Path;
use std::process::{Command, Output};

fn run(db: &Path, args: &[&str]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_cairn"));
    for v in [
        "CAIRN_ROLE",
        "CAIRN_AGENT",
        "CAIRN_SESSION",
        "CLAUDE_CODE_SESSION_ID",
        "CAIRN_PROJECT",
        "CAIRN_FORMAT",
        "AI_AGENT",
        "CLAUDECODE",
        "CLAUDE_CODE_ENTRYPOINT",
        "PI_CODING_AGENT",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
    ] {
        c.env_remove(v);
    }
    c.env("CAIRN_DB", db).args(args).output().unwrap()
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

fn id_of(v: &serde_json::Value) -> String {
    v["id"].as_str().unwrap().to_string()
}

fn file(dir: &Path, name: &str, body: &[u8]) -> String {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    p.to_str().unwrap().to_string()
}

struct Fx {
    dir: tempfile::TempDir,
    db: std::path::PathBuf,
}

impl Fx {
    fn new() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("c.db");
        Fx { dir, db }
    }
    fn submit(&self, name: &str, body: &[u8]) -> String {
        let f = file(self.dir.path(), name, body);
        id_of(&ok(&self.db, &["submit", &f]))
    }
    fn md(&self, text: &str) -> Output {
        let f = file(self.dir.path(), "doc.md", text.as_bytes());
        run(&self.db, &["submit", &f])
    }
    fn edit(&self, id: &str, text: &str) -> Output {
        let f = file(self.dir.path(), "edit.md", text.as_bytes());
        run(&self.db, &["edit", id, &f])
    }
    fn embeds(&self, id: &str, rev: i64) -> Vec<cairn::embeds::Embed> {
        let conn = cairn::db::open(&self.db).unwrap();
        cairn::embeds::for_revision(&conn, id, rev).unwrap()
    }
    fn count(&self, sql: &str) -> i64 {
        let conn = cairn::db::open(&self.db).unwrap();
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }
}

fn msg(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

#[test]
fn latest_and_pinned_are_recorded_resolved_and_text_is_untouched() {
    let fx = Fx::new();
    let img = fx.submit("p.png", &[1, 2, 3]);
    let text = format!("![a](cairn:{img})\n\n![b](cairn:{img}@1)\n");
    let o = fx.md(&text);
    assert_eq!(o.status.code(), Some(0), "{}", msg(&o));
    let md = id_of(&serde_json::from_slice(&o.stdout).unwrap());
    let e = fx.embeds(&md, 1);
    assert_eq!(e.len(), 2);
    assert_eq!((e[0].target_revision, e[0].pinned), (1, false));
    assert_eq!((e[1].target_revision, e[1].pinned), (1, true));
    assert_eq!(e[0].target_id, img);
    assert_eq!(run(&fx.db, &["get", &md]).stdout, text.as_bytes());
}

#[test]
fn embeds_are_re_resolved_on_each_markdown_revision() {
    let fx = Fx::new();
    let img = fx.submit("p.png", &[1]);
    let text = format!("![a](cairn:{img})\n![b](cairn:{img}@1)\n");
    let md = id_of(&serde_json::from_slice(&fx.md(&text).stdout).unwrap());
    let f = file(fx.dir.path(), "p.png", &[2]);
    ok(&fx.db, &["edit", &img, &f]);
    assert_eq!(fx.edit(&md, &format!("{text}x\n")).status.code(), Some(0));
    let old = fx.embeds(&md, 1);
    assert_eq!(old[0].target_revision, 1);
    let new = fx.embeds(&md, 2);
    assert_eq!((new[0].target_revision, new[0].pinned), (2, false));
    assert_eq!((new[1].target_revision, new[1].pinned), (1, true));
}

#[test]
fn bad_targets_exit_four_and_leave_nothing_behind() {
    let fx = Fx::new();
    let img = fx.submit("p.png", &[1]);
    let md_target = fx.submit("t.md", b"# t\n");
    let html = fx.submit("h.html", b"<p>x");
    for (text, want) in [
        ("![a](cairn:nope)".to_string(), "no such artifact"),
        (format!("![a](cairn:{img}@9)"), "no revision 9"),
        (format!("![a](cairn:{md_target})"), "markdown Artifact"),
        (format!("![a](cairn:{html})"), "html Artifact"),
    ] {
        let before = fx.count("SELECT COUNT(*) FROM artifacts");
        let o = fx.md(&text);
        assert_eq!(o.status.code(), Some(4), "{text}: {}", msg(&o));
        assert!(msg(&o).contains(want), "{text}: {}", msg(&o));
        assert_eq!(fx.count("SELECT COUNT(*) FROM artifacts"), before);
    }
    let good = fx.submit("g.md", b"# g\n");
    let o = fx.edit(&good, "![a](cairn:nope)");
    assert_eq!(o.status.code(), Some(4));
    assert_eq!(
        fx.count(&format!(
            "SELECT COUNT(*) FROM revisions WHERE artifact_id = '{good}'"
        )),
        1
    );
}

#[test]
fn section_edit_with_bad_ref_exits_four() {
    let fx = Fx::new();
    let md = fx.submit("g.md", b"# A\n\nbody\n");
    let f = file(fx.dir.path(), "s.md", b"# A\n\n![x](cairn:nope)\n");
    let o = run(&fx.db, &["edit", &md, "--section", "A", &f]);
    assert_eq!(o.status.code(), Some(4), "{}", msg(&o));
}

#[test]
fn deleting_an_embedded_artifact_succeeds_and_embeds_still_list() {
    let fx = Fx::new();
    let img = fx.submit("p.png", &[1]);
    let o = fx.md(&format!("![a](cairn:{img})"));
    let md = id_of(&serde_json::from_slice(&o.stdout).unwrap());
    let d = run(&fx.db, &["delete", "--yes", &img]);
    assert_eq!(d.status.code(), Some(0), "{}", msg(&d));
    assert_eq!(run(&fx.db, &["get", &md]).status.code(), Some(0));
    let e = fx.embeds(&md, 1);
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].target_id, img);
}

#[test]
fn dangling_embed_does_not_block_later_edits() {
    let fx = Fx::new();
    let img = fx.submit("p.png", &[1]);
    let text = format!("# A\n\n![a](cairn:{img}@1)\n\n# B\n\nold\n");
    let md = id_of(&serde_json::from_slice(&fx.md(&text).stdout).unwrap());
    assert_eq!(
        run(&fx.db, &["delete", "--yes", &img]).status.code(),
        Some(0)
    );

    let o = fx.edit(&md, &format!("{text}more\n"));
    assert_eq!(o.status.code(), Some(0), "{}", msg(&o));
    let e = fx.embeds(&md, 2);
    assert_eq!(
        (e.len(), e[0].target_id.as_str(), e[0].target_revision),
        (1, img.as_str(), 1)
    );

    let f = file(fx.dir.path(), "s.md", b"# B\n\nnew\n");
    let o = run(&fx.db, &["edit", &md, "--section", "B", &f]);
    assert_eq!(o.status.code(), Some(0), "{}", msg(&o));
    assert_eq!(fx.embeds(&md, 3).len(), 1);

    // A newly added or changed reference to the deleted Artifact is rejected.
    let o = fx.edit(&md, &format!("![a](cairn:{img}@1)\n![b](cairn:{img})\n"));
    assert_eq!(o.status.code(), Some(4));
    assert!(msg(&o).contains("no such artifact"), "{}", msg(&o));
    let o = fx.edit(&md, &format!("![a](cairn:{img}@2)\n"));
    assert_eq!(o.status.code(), Some(4));
}

fn embeds_json(fx: &Fx, target: &str) -> serde_json::Value {
    ok(&fx.db, &["embeds", target])
}

#[test]
fn embeds_verb_reports_pinned_unpinned_inline_and_own_line_with_lines() {
    let fx = Fx::new();
    let img = fx.submit("p.png", &[1]);
    let text = format!(
        "# Doc\n\nInline ![small *pic*]({0}) here.\n\n![own](cairn:{1}@1)\n\n```\n![code](cairn:{1})\n```\n\n`![span](cairn:{1})`\n",
        format_args!("cairn:{img}"),
        img
    );
    let md = id_of(&serde_json::from_slice(&fx.md(&text).stdout).unwrap());
    // A second image Revision: the unpinned reference follows it, the pinned one does not.
    let f = file(fx.dir.path(), "p.png", &[2]);
    ok(&fx.db, &["edit", &img, &f]);
    let v = embeds_json(&fx, &md);
    let e = v["embeds"].as_array().unwrap();
    assert_eq!(e.len(), 2, "{v}");
    assert_eq!(e[0]["ordinal"], 0);
    assert_eq!(e[0]["line"], 3);
    assert_eq!(e[0]["alt"], "small pic");
    assert_eq!(e[0]["target_id"], img);
    assert_eq!(e[0]["pinned"], false);
    assert_eq!(e[0]["target_revision"], 2);
    assert_eq!(e[0]["kind"], "image");
    assert!(e[0]["format"].is_null());
    assert_eq!(e[0]["dangling"], false);
    assert_eq!(e[1]["line"], 5);
    assert_eq!(e[1]["alt"], "own");
    assert_eq!(e[1]["pinned"], true);
    assert_eq!(e[1]["target_revision"], 1);
    // The stored line matches the text a client would split.
    let stored = String::from_utf8(run(&fx.db, &["get", &md]).stdout).unwrap();
    let lines: Vec<&str> = stored.lines().collect();
    assert!(lines[2].contains("small *pic*") && lines[4].contains("![own]"));
    // A past Revision is addressable.
    assert_eq!(
        ok(&fx.db, &["embeds", &format!("{md}@1")])["embeds"][0]["target_revision"],
        2
    );
}

#[test]
fn embeds_verb_reports_deleted_targets_as_dangling() {
    let fx = Fx::new();
    let img = fx.submit("p.png", &[1]);
    let md = id_of(
        &serde_json::from_slice(
            &fx.md(&format!("![a](cairn:{img})\n![b](cairn:{img}@1)\n"))
                .stdout,
        )
        .unwrap(),
    );
    assert_eq!(
        run(&fx.db, &["delete", &img, "--yes"]).status.code(),
        Some(0)
    );
    let v = embeds_json(&fx, &md);
    for e in v["embeds"].as_array().unwrap() {
        assert_eq!(e["dangling"], true, "{e}");
        assert!(e["target_revision"].is_null() && e["kind"].is_null() && e["format"].is_null());
    }
    assert_eq!(v["embeds"].as_array().unwrap().len(), 2);
    let t = run(&fx.db, &["--text", "embeds", &md]);
    let t = String::from_utf8(t.stdout).unwrap();
    assert_eq!(t.lines().count(), 2, "{t}");
    assert!(
        t.lines()
            .all(|l| l.starts_with('L') && l.ends_with("dangling")),
        "{t}"
    );
}

#[test]
fn embeds_verb_text_form_and_non_markdown_rejected() {
    let fx = Fx::new();
    let img = fx.submit("p.png", &[1]);
    let md =
        id_of(&serde_json::from_slice(&fx.md(&format!("x\n![a](cairn:{img})\n")).stdout).unwrap());
    let t = String::from_utf8(run(&fx.db, &["--text", "embeds", &md]).stdout).unwrap();
    assert_eq!(t, format!("L2 cairn:{img} image -> @1\n"));
    assert_eq!(run(&fx.db, &["embeds", &img]).status.code(), Some(4));
    assert_eq!(run(&fx.db, &["embeds", "nosuch"]).status.code(), Some(3));
    let plain = fx.submit("t.md", b"# none\n");
    assert_eq!(
        ok(&fx.db, &["embeds", &plain])["embeds"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}
