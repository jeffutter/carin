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
