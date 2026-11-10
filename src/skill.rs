//! `cairn skill`: one complete SKILL.md on stdout. Hand-written prose under
//! `skill/` is embedded in the binary; the command reference is read from the
//! clap tree at runtime, so it cannot drift from the CLI. Nothing is written
//! to disk (docs/skill.md).

use std::fmt::Write as _;

use clap::{Arg, ArgAction, Command as ClapCommand, CommandFactory};
use sha2::{Digest, Sha256};

use crate::artifact::hex;
use crate::cli::Cli;
use crate::error::{EXIT_CODES, Error};

const TEMPLATE: &str = include_str!("../skill/template.md");

/// Template slot name -> prose file. Each file is also scanned by the tests
/// for `cairn ...` command lines.
const PROSE: &[(&str, &str)] = &[
    ("purpose", include_str!("../skill/purpose.md")),
    ("workflow", include_str!("../skill/workflow.md")),
    ("kinds", include_str!("../skill/kinds.md")),
    ("embeds", include_str!("../skill/embeds.md")),
    ("sections", include_str!("../skill/sections.md")),
    ("serve", include_str!("../skill/serve.md")),
];

const DESCRIPTION: &str = "Submit documents, diagrams, images and html pages to cairn for human review, \
and read and answer the reviewer's Comments. Use when you want a human to review \
or comment on something you produced.";

/// Commands that are not part of the agent-facing reference.
const OMITTED: &[&str] = &["serve", "skill"];

/// The complete SKILL.md: frontmatter, then the body the hash covers.
pub fn generate() -> String {
    let body = body();
    let hash = hex(&Sha256::digest(body.as_bytes()));
    format!(
        "---\nname: cairn\ndescription: {DESCRIPTION}\ncairn_version: {}\nhash: {hash}\n---\n\n{body}",
        env!("CARGO_PKG_VERSION"),
    )
}

/// `cairn skill [--check <path>]`: returns the process exit code.
pub fn run(check: Option<&str>) -> Result<u8, Error> {
    let current = generate();
    match check {
        None => {
            print!("{current}");
            Ok(0)
        }
        Some(path) => {
            let installed = std::fs::read_to_string(path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Error::NotFound(format!("{path}: no such file"))
                } else {
                    Error::Unexpected(format!("{path}: {e}"))
                }
            })?;
            // Any difference is stale: a new version or hash, or a hand edit.
            Ok(u8::from(installed != current))
        }
    }
}

fn body() -> String {
    let mut out = TEMPLATE.to_string();
    for (slot, text) in PROSE {
        out = out.replace(&format!("{{{{{slot}}}}}"), text.trim());
    }
    let reference = reference();
    let out = out.replace("{{reference}}", reference.trim_end());
    format!("{}\n", out.trim_end())
}

fn reference() -> String {
    let root = Cli::command();
    let mut out = String::from("## Reference\n\n");

    out.push_str("### Global flags\n\nAccepted by every command.\n\n");
    for arg in visible_args(&root).filter(|a| a.is_global_set()) {
        out.push_str(&arg_line(arg));
    }

    out.push_str("\n### Exit codes\n\n| Code | Meaning |\n| --- | --- |\n");
    for (code, meaning) in EXIT_CODES {
        let _ = writeln!(out, "| {code} | {meaning} |");
    }
    out.push_str(
        "\n`cairn skill --check` also exits 1 when the installed skill is stale. \
Errors print `{\"code\": ..., \"message\": ...}` as JSON on stderr.\n",
    );

    out.push_str("\n### Commands\n");
    walk(&root, "cairn", &mut out);
    out
}

fn walk(cmd: &ClapCommand, path: &str, out: &mut String) {
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if sub.is_hide_set() || (path == "cairn" && OMITTED.contains(&name)) {
            continue;
        }
        let sub_path = format!("{path} {name}");
        let args: Vec<&Arg> = visible_args(sub).filter(|a| !a.is_global_set()).collect();
        let positionals: Vec<&Arg> = args.iter().copied().filter(|a| a.is_positional()).collect();
        let has_flags = args.iter().any(|a| !a.is_positional());

        let _ = write!(out, "\n#### `{sub_path}`\n\n");
        if let Some(about) = sub.get_long_about().or_else(|| sub.get_about()) {
            let _ = writeln!(out, "{}\n", about.to_string().trim());
        }
        let mut usage = sub_path.clone();
        if sub.has_subcommands() {
            usage.push_str(" <COMMAND>");
        }
        if has_flags {
            usage.push_str(" [OPTIONS]");
        }
        for p in &positionals {
            usage.push(' ');
            usage.push_str(&positional_usage(p));
        }
        let _ = writeln!(out, "```\n{usage}\n```");
        if !args.is_empty() {
            out.push('\n');
            for arg in &args {
                out.push_str(&arg_line(arg));
            }
        }
        walk(sub, &sub_path, out);
    }
}

fn visible_args(cmd: &ClapCommand) -> impl Iterator<Item = &Arg> {
    cmd.get_arguments()
        .filter(|a| !a.is_hide_set() && !matches!(a.get_id().as_str(), "help" | "version"))
}

fn value_name(arg: &Arg) -> String {
    arg.get_value_names()
        .and_then(|n| n.first().map(|s| s.to_string()))
        .unwrap_or_else(|| arg.get_id().as_str().to_uppercase().replace('-', "_"))
}

fn takes_value(arg: &Arg) -> bool {
    !matches!(
        arg.get_action(),
        ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count
    )
}

fn positional_usage(arg: &Arg) -> String {
    let name = value_name(arg);
    let many = matches!(arg.get_action(), ArgAction::Append);
    let dots = if many { "..." } else { "" };
    if arg.is_required_set() {
        format!("<{name}>{dots}")
    } else {
        format!("[{name}]{dots}")
    }
}

/// One Markdown list item: the argument's spelling, its help, then env,
/// default and allowed values.
fn arg_line(arg: &Arg) -> String {
    let spelling = if arg.is_positional() {
        positional_usage(arg)
    } else {
        let long = arg.get_long().unwrap_or_default();
        if takes_value(arg) {
            format!("--{long} <{}>", value_name(arg))
        } else {
            format!("--{long}")
        }
    };
    let help = arg
        .get_long_help()
        .or_else(|| arg.get_help())
        .map(|h| {
            h.to_string()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let mut line = format!("- `{spelling}`: {help}");
    if let Some(env) = arg.get_env() {
        let _ = write!(line, " [env: {}]", env.to_string_lossy());
    }
    let defaults: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|d| d.to_string_lossy().into_owned())
        .collect();
    if !defaults.is_empty() {
        let _ = write!(line, " [default: {}]", defaults.join(","));
    }
    let values: Vec<String> = arg
        .get_possible_values()
        .iter()
        .filter(|v| takes_value(arg) && !v.is_hide_set())
        .map(|v| v.get_name().to_string())
        .collect();
    if !values.is_empty() {
        let _ = write!(line, " [values: {}]", values.join(", "));
    }
    line.push('\n');
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk_all<'a>(cmd: &'a ClapCommand, f: &mut dyn FnMut(&'a ClapCommand)) {
        for sub in cmd.get_subcommands().filter(|s| !s.is_hide_set()) {
            f(sub);
            walk_all(sub, f);
        }
    }

    #[test]
    fn frontmatter_has_version_and_hash_of_the_body() {
        let text = generate();
        let rest = text.strip_prefix("---\n").expect("opens with ---");
        let (front, body) = rest.split_once("\n---\n\n").expect("closes with ---");
        let field = |k: &str| {
            front
                .lines()
                .find_map(|l| l.strip_prefix(&format!("{k}: ")))
                .unwrap_or_else(|| panic!("missing {k}"))
        };
        assert_eq!(field("name"), "cairn");
        assert!(!field("description").is_empty());
        assert_eq!(field("cairn_version"), env!("CARGO_PKG_VERSION"));
        assert_eq!(field("hash"), hex(&Sha256::digest(body.as_bytes())));
        assert_eq!(field("hash").len(), 64);
    }

    #[test]
    fn prose_command_lines_parse_with_the_clap_tree() {
        let mut checked = 0;
        for (slot, text) in PROSE {
            let mut in_fence = false;
            for line in text.lines() {
                if line.trim_start().starts_with("```") {
                    in_fence = !in_fence;
                } else if in_fence && line.starts_with("cairn ") {
                    let words = shlex::split(line)
                        .unwrap_or_else(|| panic!("{slot}: unbalanced quotes in {line:?}"));
                    Cli::command()
                        .try_get_matches_from(words)
                        .unwrap_or_else(|e| panic!("{slot}: {line:?} does not parse: {e}"));
                    checked += 1;
                }
            }
        }
        assert!(checked > 5, "only {checked} command lines found");
    }

    #[test]
    fn every_visible_command_and_argument_has_help() {
        let root = Cli::command();
        let mut check = |cmd: &ClapCommand| {
            assert!(
                cmd.get_about().is_some(),
                "command {} has no help",
                cmd.get_name()
            );
            for arg in visible_args(cmd) {
                assert!(
                    arg.get_help().is_some(),
                    "{} {}: no help",
                    cmd.get_name(),
                    arg.get_id()
                );
            }
        };
        check(&root);
        walk_all(&root, &mut check);
    }

    #[test]
    fn serve_and_skill_are_omitted() {
        let text = generate();
        assert!(!text.contains("`cairn serve`\n\n"));
        assert!(!text.contains("#### `cairn serve"));
        assert!(!text.contains("#### `cairn skill"));
        assert!(text.contains("#### `cairn submit`"));
    }

    #[test]
    fn exit_codes_match_the_shared_table() {
        let text = generate();
        for (code, meaning) in EXIT_CODES {
            assert!(text.contains(&format!("| {code} | {meaning} |")));
        }
    }

    #[test]
    fn check_exits_0_when_current_and_1_when_stale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("SKILL.md");
        let p = path.to_str().unwrap();
        std::fs::write(&path, generate()).unwrap();
        assert_eq!(run(Some(p)).unwrap(), 0);
        std::fs::write(
            &path,
            generate().replace("cairn_version: ", "cairn_version: 0.0."),
        )
        .unwrap();
        assert_eq!(run(Some(p)).unwrap(), 1);
        std::fs::write(&path, format!("{}x", generate())).unwrap();
        assert_eq!(run(Some(p)).unwrap(), 1);
        let missing = dir.path().join("nope.md");
        assert!(matches!(
            run(Some(missing.to_str().unwrap())),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn full_output_snapshot() {
        insta::assert_snapshot!(generate());
    }
}
