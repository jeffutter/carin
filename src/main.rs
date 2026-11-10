use std::process::ExitCode;

fn main() -> ExitCode {
    let result = cairn::cli::parse(std::env::args_os())
        .map(|cli| cli.resolve(std::env::var("CLAUDE_CODE_SESSION_ID").ok()))
        .and_then(|(ctx, command)| cairn::run(command, &ctx));
    match result {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("{}", err.to_json());
            ExitCode::from(err.exit_code())
        }
    }
}
