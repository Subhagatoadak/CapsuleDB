//! `connector-sdk`: the interface between an external source of
//! documents and `capsule-compiler` (Phase 9's "connector SDK").
//!
//! One connector is implemented here: [`FilesystemConnector`], a real,
//! tested reference implementation with no network dependency. Named
//! connectors for GitHub, SharePoint, Confluence, object storage, a
//! generic API, and a database are **not implemented** — each needs
//! real credentials, a real endpoint, and integration testing against
//! that live service that this environment can't do honestly. Rather
//! than ship untested network code, the trait is the deliverable for
//! those: implement [`SourceConnector`] against a real client and
//! everything downstream (`capsule-compiler`, and via it
//! `capsule-registry`) already works, as [`FilesystemConnector`]
//! demonstrates.

use capsule_compiler::{ingest_html, ingest_markdown, ingest_pdf, ingest_text, CompilerError, Source};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormatHint {
    Text,
    Markdown,
    Html,
    Pdf,
    /// The connector has no way to guess; the caller must pick a format
    /// (or fail) before compiling.
    Unknown,
}

/// A reference to one document a connector can fetch — cheap to list in
/// bulk, since it doesn't carry the document's bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRef {
    /// Stable within this connector (e.g. a relative path or a remote
    /// item ID) — not a `kc://` logical ID.
    pub id: String,
    pub display_name: String,
    pub format_hint: SourceFormatHint,
}

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("source not found: {0}")]
    NotFound(String),
    #[error("connector I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Compiler(#[from] CompilerError),
}

pub trait SourceConnector {
    /// Enumerates every document this connector currently sees.
    fn list(&self) -> Result<Vec<SourceRef>, ConnectorError>;
    /// Fetches one document's raw bytes.
    fn fetch(&self, source_ref: &SourceRef) -> Result<Vec<u8>, ConnectorError>;
}

/// Fetches `source_ref` from `connector` and runs it through the
/// `capsule-compiler` adapter matching its format hint — the glue a real
/// connector-to-compiler pipeline needs, so implementing
/// [`SourceConnector`] is enough to plug into compilation without also
/// writing this dispatch.
pub fn ingest(connector: &dyn SourceConnector, source_ref: &SourceRef) -> Result<Source, ConnectorError> {
    let bytes = connector.fetch(source_ref)?;
    let source = match source_ref.format_hint {
        SourceFormatHint::Text | SourceFormatHint::Unknown => ingest_text(&bytes)?,
        SourceFormatHint::Markdown => ingest_markdown(&bytes)?,
        SourceFormatHint::Html => ingest_html(&bytes)?,
        SourceFormatHint::Pdf => ingest_pdf(&bytes)?,
    };
    Ok(source)
}

/// Reads documents from a local directory tree — the one connector this
/// crate implements against a real, testable backend.
pub struct FilesystemConnector {
    root: PathBuf,
}

impl FilesystemConnector {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl fmt::Debug for FilesystemConnector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FilesystemConnector").field("root", &self.root).finish()
    }
}

impl SourceConnector for FilesystemConnector {
    fn list(&self) -> Result<Vec<SourceRef>, ConnectorError> {
        let mut refs = Vec::new();
        walk(&self.root, &self.root, &mut refs)?;
        refs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(refs)
    }

    fn fetch(&self, source_ref: &SourceRef) -> Result<Vec<u8>, ConnectorError> {
        let path = self.root.join(&source_ref.id);
        fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ConnectorError::NotFound(source_ref.id.clone())
            } else {
                ConnectorError::Io(e)
            }
        })
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<SourceRef>) -> Result<(), ConnectorError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            walk(root, &path, out)?;
            continue;
        }
        let Some(format_hint) = format_hint_for(&path) else { continue };
        let id = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        let display_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| id.clone());
        out.push(SourceRef { id, display_name, format_hint });
    }
    Ok(())
}

fn format_hint_for(path: &Path) -> Option<SourceFormatHint> {
    match path.extension().and_then(|e| e.to_str())?.to_lowercase().as_str() {
        "txt" => Some(SourceFormatHint::Text),
        "md" | "markdown" => Some(SourceFormatHint::Markdown),
        "html" | "htm" => Some(SourceFormatHint::Html),
        "pdf" => Some(SourceFormatHint::Pdf),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("connector-sdk-test-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lists_only_recognized_extensions_recursively() {
        let dir = tempdir();
        fs::write(dir.join("policy.md"), "# Policy\n\nBody.").unwrap();
        fs::write(dir.join("notes.txt"), "Plain text.").unwrap();
        fs::write(dir.join("image.png"), [0u8, 1, 2]).unwrap();
        let sub = dir.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("page.html"), "<p>Hi</p>").unwrap();

        let connector = FilesystemConnector::new(&dir);
        let refs = connector.list().unwrap();
        let ids: Vec<&str> = refs.iter().map(|r| r.id.as_str()).collect();

        assert!(ids.contains(&"policy.md"));
        assert!(ids.contains(&"notes.txt"));
        assert!(ids.contains(&"sub/page.html"));
        assert!(!ids.iter().any(|id| id.ends_with(".png")));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fetch_and_ingest_round_trip() {
        let dir = tempdir();
        fs::write(dir.join("policy.md"), "# Policy\n\nData must be retained for ninety days.").unwrap();

        let connector = FilesystemConnector::new(&dir);
        let refs = connector.list().unwrap();
        let source_ref = refs.into_iter().find(|r| r.id == "policy.md").unwrap();
        assert_eq!(source_ref.format_hint, SourceFormatHint::Markdown);

        let source = ingest(&connector, &source_ref).unwrap();
        assert!(source.text.contains("Data must be retained"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fetch_missing_file_is_not_found() {
        let dir = tempdir();
        let connector = FilesystemConnector::new(&dir);
        let err = connector.fetch(&SourceRef { id: "missing.md".to_string(), display_name: "missing.md".to_string(), format_hint: SourceFormatHint::Markdown }).unwrap_err();
        assert!(matches!(err, ConnectorError::NotFound(_)));
        fs::remove_dir_all(&dir).ok();
    }
}
