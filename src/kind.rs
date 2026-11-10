//! Artifact Kind and its inference from a file extension.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Markdown,
    Html,
    Image,
    Diagram,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Markdown => "markdown",
            Kind::Html => "html",
            Kind::Image => "image",
            Kind::Diagram => "diagram",
        }
    }

    /// Media type stored with the file. Only markdown is accepted so far.
    pub fn media_type(self) -> Result<&'static str, Error> {
        match self {
            Kind::Markdown => Ok("text/markdown"),
            other => Err(Error::Rejected(format!(
                "kind {} is not supported yet",
                other.as_str()
            ))),
        }
    }
}

/// An explicit `--kind` wins; otherwise infer from the extension. Never sniffed.
pub fn infer(path: &str, explicit: Option<Kind>) -> Result<Kind, Error> {
    if let Some(k) = explicit {
        return Ok(k);
    }
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("md") => Ok(Kind::Markdown),
        Some("html") => Ok(Kind::Html),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp") => Ok(Kind::Image),
        Some("mmd") => Ok(Kind::Diagram),
        Some("svg") => Err(Error::Rejected(
            "`.svg` is ambiguous: pass --kind diagram --format svg".into(),
        )),
        _ => Err(Error::Rejected(format!(
            "cannot infer a Kind from {path:?}: pass --kind"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_from_extension() {
        assert_eq!(infer("a/b.md", None).unwrap(), Kind::Markdown);
        assert_eq!(infer("x.PNG", None).unwrap(), Kind::Image);
        assert_eq!(infer("x.mmd", None).unwrap(), Kind::Diagram);
        assert_eq!(infer("x.html", None).unwrap(), Kind::Html);
        assert_eq!(infer("x.svg", None).unwrap_err().exit_code(), 4);
        assert_eq!(infer("x.txt", None).unwrap_err().exit_code(), 4);
        assert_eq!(
            infer("x.txt", Some(Kind::Markdown)).unwrap(),
            Kind::Markdown
        );
    }

    #[test]
    fn only_markdown_has_media_type() {
        assert_eq!(Kind::Markdown.media_type().unwrap(), "text/markdown");
        assert_eq!(Kind::Image.media_type().unwrap_err().exit_code(), 4);
    }
}
