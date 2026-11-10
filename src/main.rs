use std::process::ExitCode;

fn main() -> ExitCode {
    let result = cairn::cli::parse(std::env::args_os())
        .map(|cli| cli.resolve(std::env::var("CLAUDE_CODE_SESSION_ID").ok()));
    match result {
        Ok(_ctx) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{}", err.to_json());
            ExitCode::from(err.exit_code())
        }
    }
}
