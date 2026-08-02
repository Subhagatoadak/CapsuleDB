//! Source ingestion contracts (spec: "source structure preservation").
//!
//! Every adapter produces a [`Source`]: a normalized plain-text
//! representation plus the [`StructuralBlock`]s (paragraphs and headings)
//! it was split from. All downstream span binding (claim extraction,
//! evidence spans) is relative to `Source.text`, **not** the original raw
//! bytes — for Markdown and HTML that text is a derived, tag/syntax-free
//! view, since binding spans to raw markup offsets would make the
//! extracted excerpts unreadable. `Source.id` fingerprints the original
//! raw bytes, so re-ingesting byte-identical source is still recognizable
//! even though the derived text is what claims are bound to.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Text,
    Markdown,
    Html,
    Pdf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    Heading(u8),
    Paragraph,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralBlock {
    pub block_type: BlockType,
    /// Byte offsets into the owning `Source.text`.
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Source {
    /// `sha256:<hex>` fingerprint of the original raw bytes.
    pub id: String,
    pub format: SourceFormat,
    /// Normalized, structure-preserved plain text; all spans are relative
    /// to this string.
    pub text: String,
    pub blocks: Vec<StructuralBlock>,
}
