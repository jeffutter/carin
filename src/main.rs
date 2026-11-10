use std::io::IsTerminal;
use std::process::ExitCode;

use cairn::error::Error;
use cairn::output::{self, Flags, Format};

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

fn fail(err: &Error, fmt: Format) -> ExitCode {
    eprintln!("{}", output::render_error(fmt, err));
    ExitCode::from(err.exit_code())
}

fn main() -> ExitCode {
    let stderr_tty = std::io::stderr().is_terminal();
    // The format for errors: the same decision on stderr's terminal-ness. If
    // the decision itself is invalid (bad CAIRN_FORMAT), report it as JSON.
    let err_format = |flags: Flags| output::decide(flags, &env, stderr_tty).unwrap_or(Format::Json);

    let cli = match cairn::cli::parse(std::env::args_os()) {
        Ok(cli) => cli,
        Err(e) => return fail(&e, err_format(Flags::default())),
    };
    let flags = Flags {
        json: cli.json,
        text: cli.text,
    };
    let fmt = match output::decide(flags, &env, std::io::stdout().is_terminal()) {
        Ok(f) => f,
        Err(e) => return fail(&e, err_format(flags)),
    };
    let (ctx, command) = cli.resolve(env("CLAUDE_CODE_SESSION_ID"));
    match cairn::run(command, &ctx, fmt) {
        Ok(code) => ExitCode::from(code),
        Err(e) => fail(&e, err_format(flags)),
    }
}
