//! File sets: reading a submitted path into files, and checking a file set
//! against its Kind (media-type allowlist, entry page, limits).

use std::fs;
use std::path::Path;

use crate::error::Error;
use crate::kind::Kind;
use crate::limits;

/// A file as read from disk or stdin, before Kind checks.
pub struct RawFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// What the caller submitted: one file (or stdin), or a directory.
pub struct Input {
    pub files: Vec<RawFile>,
    pub dir: bool,
}

/// A checked file ready to store.
pub struct FileInput {
    pub path: String,
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
}

/// Is a Bundle-relative path safe to write under an output directory?
pub fn safe_path(p: &str) -> bool {
    !p.is_empty()
        && !p.contains(['\\', '\0'])
        && p.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

/// Walk a directory. Symlinks are rejected, not followed. The file-count and
/// size limits are checked from metadata before any content is read.
pub fn read_dir(root: &Path) -> Result<Input, Error> {
    let mut found: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut total: u64 = 0;
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        let io = |e: std::io::Error| Error::Usage(format!("cannot read {}: {e}", dir.display()));
        for entry in fs::read_dir(&dir).map_err(io)? {
            let entry = entry.map_err(io)?;
            let name = entry.file_name().into_string().map_err(|n| {
                Error::Rejected(format!("non-UTF-8 file name {n:?} in {}", dir.display()))
            })?;
            let rel = format!("{prefix}{name}");
            let meta = fs::symlink_metadata(entry.path()).map_err(io)?;
            if meta.file_type().is_symlink() {
                return Err(Error::Rejected(format!("symlink {rel:?} is not allowed")));
            } else if meta.is_dir() {
                stack.push((entry.path(), format!("{rel}/")));
            } else if meta.is_file() {
                limits::check_file(meta.len())?;
                total += meta.len();
                limits::check_revision(total)?;
                found.push((rel, entry.path()));
                limits::check_bundle_files(found.len())?;
            } else {
                return Err(Error::Rejected(format!("{rel:?} is not a regular file")));
            }
        }
    }
    found.sort();
    let mut files = Vec::with_capacity(found.len());
    for (path, full) in found {
        let bytes = fs::read(&full)
            .map_err(|e| Error::Usage(format!("cannot read {}: {e}", full.display())))?;
        files.push(RawFile { path, bytes });
    }
    Ok(Input { files, dir: true })
}

/// Check a submitted file set against the Kind and turn it into storable
/// files. `single_path` is where a lone markdown file is stored (the artifact
/// name on submit, the previous path on edit).
pub fn prepare(kind: Kind, input: Input, single_path: &str) -> Result<Vec<FileInput>, Error> {
    let Input { mut files, dir } = input;
    limits::check_bundle_files(files.len())?;
    match kind {
        Kind::Html => {
            if !dir && files.len() == 1 {
                files[0].path = "index.html".into();
            }
        }
        Kind::Markdown | Kind::Image => {
            if dir || files.len() != 1 {
                return Err(Error::Rejected(format!(
                    "a {} Artifact is a single file, not a directory",
                    kind.as_str()
                )));
            }
            if kind == Kind::Markdown {
                files[0].path = single_path.into();
            }
        }
        Kind::Diagram => return Err(Error::Rejected("kind diagram is not supported yet".into())),
    }
    let mut total: u64 = 0;
    let mut out: Vec<FileInput> = Vec::with_capacity(files.len());
    for f in files {
        if !safe_path(&f.path) {
            return Err(Error::Rejected(format!("unsafe file path {:?}", f.path)));
        }
        if out.iter().any(|o| o.path == f.path) {
            return Err(Error::Rejected(format!("duplicate file path {:?}", f.path)));
        }
        limits::check_file(f.bytes.len() as u64)?;
        total += f.bytes.len() as u64;
        limits::check_revision(total)?;
        out.push(FileInput {
            media_type: kind.media_type(&f.path)?,
            path: f.path,
            bytes: f.bytes,
        });
    }
    if kind == Kind::Html && !out.iter().any(|f| f.path == "index.html") {
        return Err(Error::Rejected(
            "an html Bundle needs an index.html entry page at its root".into(),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(path: &str, b: &str) -> RawFile {
        RawFile {
            path: path.into(),
            bytes: b.as_bytes().to_vec(),
        }
    }

    #[test]
    fn safe_paths() {
        for ok in ["a", "a/b.js", "x.y/z"] {
            assert!(safe_path(ok), "{ok}");
        }
        for bad in ["", "/a", "a//b", "../a", "a/..", "./a", "a\\b", "a/"] {
            assert!(!safe_path(bad), "{bad}");
        }
    }

    #[test]
    fn single_html_becomes_index() {
        let files = prepare(
            Kind::Html,
            Input {
                files: vec![raw("page.html", "<p>")],
                dir: false,
            },
            "page.html",
        )
        .unwrap();
        assert_eq!(files[0].path, "index.html");
        assert_eq!(files[0].media_type, "text/html");
    }

    #[test]
    fn directory_without_entry_page_is_rejected() {
        let e = prepare(
            Kind::Html,
            Input {
                files: vec![raw("page.html", "<p>")],
                dir: true,
            },
            "x",
        )
        .err()
        .unwrap();
        assert_eq!(e.exit_code(), 4);
        assert!(e.to_string().contains("index.html"));
    }

    #[test]
    fn markdown_directory_is_rejected() {
        let e = prepare(
            Kind::Markdown,
            Input {
                files: vec![raw("a.md", "x")],
                dir: true,
            },
            "a.md",
        )
        .err()
        .unwrap();
        assert_eq!(e.exit_code(), 4);
    }
}
