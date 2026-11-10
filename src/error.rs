//! The single core error enum. Its stable `code` strings are shared with the
//! JSON API; its exit codes drive the CLI (docs/cli.md "Exit codes").

use serde::Serialize;

/// Exit code for `comment list --wait` timing out with nothing new. It is not
/// an error, so it has no `Error` variant.
pub const EXIT_WAIT_TIMEOUT: u8 = 5;

/// Exit code table: (code, meaning). The generated skill reads this too.
pub const EXIT_CODES: &[(u8, &str)] = &[
    (0, "Success"),
    (
        1,
        "Unexpected error, including database busy past the 5 s timeout",
    ),
    (2, "Usage error, including `delete` without `--yes`"),
    (3, "Not found, including a write to a deleted Artifact"),
    (
        4,
        "Rejected input: limits, bad Kind, missing embed target, invalid Mermaid, ambiguous or missing Section",
    ),
    (
        EXIT_WAIT_TIMEOUT,
        "`comment list --wait` timed out with nothing new",
    ),
    (6, "Conflict: `edit --base` named a stale Revision"),
];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Rejected(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Unexpected(String),
}

#[derive(Serialize)]
struct JsonError<'a> {
    code: &'a str,
    message: String,
}

impl Error {
    /// Stable snake_case identifier, shared with the API.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Usage(_) => "usage",
            Error::NotFound(_) => "not_found",
            Error::Rejected(_) => "rejected_input",
            Error::Conflict(_) => "conflict",
            Error::Unexpected(_) => "unexpected",
        }
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            Error::Unexpected(_) => 1,
            Error::Usage(_) => 2,
            Error::NotFound(_) => 3,
            Error::Rejected(_) => 4,
            Error::Conflict(_) => 6,
        }
    }

    /// HTTP status of the API's problem+json response (docs/api.md "Errors").
    pub fn http_status(&self) -> u16 {
        match self {
            Error::Usage(_) => 400,
            Error::NotFound(_) => 404,
            Error::Rejected(_) => 422,
            Error::Conflict(_) => 409,
            Error::Unexpected(_) => 500,
        }
    }

    /// The JSON line printed on stderr: `{"code":..., "message":...}`.
    pub fn to_json(&self) -> String {
        let body = JsonError {
            code: self.code(),
            message: self.to_string(),
        };
        serde_json::to_string(&body).expect("error body serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_map_to_documented_codes() {
        let cases = [
            (Error::Unexpected("x".into()), "unexpected", 1, 500),
            (Error::Usage("x".into()), "usage", 2, 400),
            (Error::NotFound("x".into()), "not_found", 3, 404),
            (Error::Rejected("x".into()), "rejected_input", 4, 422),
            (Error::Conflict("x".into()), "conflict", 6, 409),
        ];
        for (err, code, exit, status) in cases {
            assert_eq!(err.code(), code);
            assert_eq!(err.exit_code(), exit);
            assert_eq!(err.http_status(), status);
            assert!(EXIT_CODES.iter().any(|(c, _)| *c == exit));
        }
    }

    #[test]
    fn json_has_stable_code_and_message() {
        let v: serde_json::Value =
            serde_json::from_str(&Error::NotFound("no such artifact".into()).to_json()).unwrap();
        assert_eq!(v["code"], "not_found");
        assert_eq!(v["message"], "no such artifact");
    }

    #[test]
    fn wait_timeout_is_exit_five() {
        assert_eq!(EXIT_WAIT_TIMEOUT, 5);
        assert!(EXIT_CODES.iter().any(|(c, _)| *c == 5));
    }
}
