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
