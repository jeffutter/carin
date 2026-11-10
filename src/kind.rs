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

    /// The Kind named by its stored `as_str` form.
    pub fn parse(s: &str) -> Result<Kind, Error> {
        match s {
            "markdown" => Ok(Kind::Markdown),
            "html" => Ok(Kind::Html),
            "image" => Ok(Kind::Image),
            "diagram" => Ok(Kind::Diagram),
            other => Err(Error::Unexpected(format!("unknown stored kind {other:?}"))),
        }
    }

    /// Media type for one file of this Kind, from an allowlist keyed by the
    /// file's extension. Never sniffed from content.
    pub fn media_type(self, path: &str) -> Result<&'static str, Error> {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let found = match self {
            Kind::Markdown => Some("text/markdown"),
            Kind::Image => image_type(&ext),
            Kind::Html => match ext.as_str() {
                "html" => Some("text/html"),
                "css" => Some("text/css"),
                "js" => Some("text/javascript"),
                "json" => Some("application/json"),
                "txt" => Some("text/plain"),
                "svg" => Some("image/svg+xml"),
                "ico" => Some("image/x-icon"),
                "woff2" => Some("font/woff2"),
                "woff" => Some("font/woff"),
                _ => image_type(&ext),
            },
            Kind::Diagram => {
                return Err(Error::Rejected("kind diagram is not supported yet".into()));
            }
        };
        found.ok_or_else(|| {
            Error::Rejected(format!(
                "{path:?}: media type for extension {ext:?} is not allowed in a {} Artifact",
                self.as_str()
            ))
        })
    }
}

fn image_type(ext: &str) -> Option<&'static str> {
    match ext {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
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
    fn media_types_follow_the_per_kind_allowlist() {
        assert_eq!(Kind::Markdown.media_type("a.md").unwrap(), "text/markdown");
        assert_eq!(Kind::Image.media_type("a.JPEG").unwrap(), "image/jpeg");
        assert_eq!(
            Kind::Html.media_type("s/app.js").unwrap(),
            "text/javascript"
        );
        assert_eq!(Kind::Html.media_type("a.webp").unwrap(), "image/webp");
        for (k, p) in [
            (Kind::Image, "a.svg"),
            (Kind::Image, "a.html"),
            (Kind::Html, "x.exe"),
            (Kind::Html, "noext"),
            (Kind::Diagram, "a.mmd"),
        ] {
            assert_eq!(k.media_type(p).unwrap_err().exit_code(), 4, "{p}");
        }
    }
}
