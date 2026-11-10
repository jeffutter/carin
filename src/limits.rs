//! Hard-coded storage limits (docs/data-model.md "Limits").

use crate::error::Error;

pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_REVISION_BYTES: u64 = 25 * 1024 * 1024;
pub const MAX_BUNDLE_FILES: usize = 200;

pub fn check_file(size: u64) -> Result<(), Error> {
    if size > MAX_FILE_BYTES {
        return Err(Error::Rejected(format!(
            "file is {size} bytes, over the per-file limit of 10 MiB ({MAX_FILE_BYTES} bytes)"
        )));
    }
    Ok(())
}

pub fn check_revision(total: u64) -> Result<(), Error> {
    if total > MAX_REVISION_BYTES {
        return Err(Error::Rejected(format!(
            "revision is {total} bytes, over the per-Revision limit of 25 MiB ({MAX_REVISION_BYTES} bytes)"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_limit() {
        assert!(check_file(MAX_FILE_BYTES).is_ok());
        let e = check_file(MAX_FILE_BYTES + 1).unwrap_err();
        assert_eq!(e.exit_code(), 4);
        assert!(e.to_string().contains("10 MiB"));
    }

    #[test]
    fn names_the_revision_limit() {
        assert!(check_revision(MAX_REVISION_BYTES).is_ok());
        let e = check_revision(MAX_REVISION_BYTES + 1).unwrap_err();
        assert_eq!(e.exit_code(), 4);
        assert!(e.to_string().contains("25 MiB"));
    }
}
