//! Source adapters: text, Markdown, and HTML are fully implemented; PDF is
//! deliberately not — see [`ingest_pdf`].

use crate::error::CompilerError;
use crate::source::{BlockType, Source, SourceFormat, StructuralBlock};
use capsule_core::source_fingerprint;
use regex::Regex;
use std::sync::OnceLock;

pub fn ingest_text(bytes: &[u8]) -> Result<Source, CompilerError> {
    let raw = std::str::from_utf8(bytes)?;
    let (text, blocks) = split_into_blocks(raw);
    Ok(Source { id: source_fingerprint(bytes), format: SourceFormat::Text, text, blocks })
}

/// Recognizes `#`..`######` headings natively; does not strip inline
/// emphasis/link syntax (`**bold**`, `[text](url)`) since doing so would
/// require remapping spans — out of scope for the Phase 3 MVP. Inline
/// markdown noise rarely breaks sentence extraction badly enough to
/// matter for a first pass.
pub fn ingest_markdown(bytes: &[u8]) -> Result<Source, CompilerError> {
    let raw = std::str::from_utf8(bytes)?;
    let (text, blocks) = split_into_blocks(raw);
    Ok(Source { id: source_fingerprint(bytes), format: SourceFormat::Markdown, text, blocks })
}

/// Strips tags, converts `<h1>`-`<h6>` into Markdown-style `#` headings so
/// they flow through the same heading detection as the Markdown adapter,
/// and decodes a handful of common entities.
pub fn ingest_html(bytes: &[u8]) -> Result<Source, CompilerError> {
    let raw = std::str::from_utf8(bytes)?;
    let converted = html_to_blocks_text(raw);
    let (text, blocks) = split_into_blocks(&converted);
    Ok(Source { id: source_fingerprint(bytes), format: SourceFormat::Html, text, blocks })
}

/// PDF text extraction is not implemented in the Phase 3 MVP: robust PDF
/// text extraction needs font/encoding handling that is a project of its
/// own, and pulling in a heavy dependency for a partial job seemed worse
/// than being explicit that it's unsupported. Always returns
/// `CompilerError::UnsupportedFormat`.
pub fn ingest_pdf(_bytes: &[u8]) -> Result<Source, CompilerError> {
    Err(CompilerError::UnsupportedFormat(
        "PDF ingestion is not implemented yet; convert to text/Markdown/HTML first".to_string(),
    ))
}

fn blank_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\n[ \t]*\n[ \t\n]*").expect("valid regex"))
}

fn heading_open_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)<h([1-6])[^>]*>").expect("valid regex"))
}

fn block_close_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)</h[1-6]>|</p>|</div>|<br\s*/?>|</li>|</tr>").expect("valid regex"))
}

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("valid regex"))
}

fn html_to_blocks_text(raw: &str) -> String {
    let with_headings = heading_open_re().replace_all(raw, |caps: &regex::Captures| {
        let level: usize = caps[1].parse().unwrap_or(1);
        format!("\n\n{} ", "#".repeat(level))
    });
    let with_breaks = block_close_re().replace_all(&with_headings, "\n\n");
    let stripped = tag_re().replace_all(&with_breaks, "");
    decode_entities(&stripped)
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

/// Collapses runs of blank lines to a single block separator, splits on
/// them, and recognizes a leading `# `..`###### ` marker as a heading.
/// Returns the rebuilt normalized text (blocks rejoined with `\n\n`)
/// alongside the blocks, so block offsets are always valid slices of the
/// returned text.
fn split_into_blocks(raw: &str) -> (String, Vec<StructuralBlock>) {
    let normalized = raw.replace("\r\n", "\n");
    let collapsed = blank_line_re().replace_all(&normalized, "\n\n");

    let mut text = String::new();
    let mut blocks = Vec::new();
    for raw_block in collapsed.split("\n\n") {
        let trimmed = raw_block.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (block_type, content) = match strip_heading_marker(trimmed) {
            Some((level, rest)) => (BlockType::Heading(level), rest),
            None => (BlockType::Paragraph, trimmed),
        };
        if content.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        let start = text.len();
        text.push_str(content);
        let end = text.len();
        blocks.push(StructuralBlock { block_type, start, end, text: content.to_string() });
    }
    (text, blocks)
}

fn strip_heading_marker(s: &str) -> Option<(u8, &str)> {
    let hashes = s.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && s.as_bytes().get(hashes) == Some(&b' ') {
        Some((hashes as u8, s[hashes + 1..].trim_start()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_adapter_splits_paragraphs() {
        let source = ingest_text(b"First paragraph.\n\nSecond paragraph.").unwrap();
        assert_eq!(source.blocks.len(), 2);
        assert_eq!(source.blocks[0].text, "First paragraph.");
        assert_eq!(source.blocks[1].text, "Second paragraph.");
        assert_eq!(&source.text[source.blocks[1].start..source.blocks[1].end], "Second paragraph.");
    }

    #[test]
    fn text_adapter_collapses_ragged_blank_lines() {
        let source = ingest_text(b"A.\n\n\n\n   \n\nB.").unwrap();
        assert_eq!(source.blocks.len(), 2);
    }

    #[test]
    fn markdown_adapter_recognizes_headings() {
        let source = ingest_markdown(b"# Title\n\nBody text here.\n\n## Subsection\n\nMore text.").unwrap();
        assert_eq!(source.blocks[0].block_type, BlockType::Heading(1));
        assert_eq!(source.blocks[0].text, "Title");
        assert_eq!(source.blocks[1].block_type, BlockType::Paragraph);
        assert_eq!(source.blocks[2].block_type, BlockType::Heading(2));
    }

    #[test]
    fn html_adapter_strips_tags_and_detects_headings() {
        let html = b"<html><body><h1>Policy</h1><p>Data must be retained.</p><p>Except for legal holds.</p></body></html>";
        let source = ingest_html(html).unwrap();
        assert_eq!(source.blocks[0].block_type, BlockType::Heading(1));
        assert_eq!(source.blocks[0].text, "Policy");
        assert_eq!(source.blocks[1].text, "Data must be retained.");
        assert_eq!(source.blocks[2].text, "Except for legal holds.");
    }

    #[test]
    fn html_adapter_decodes_entities() {
        let html = b"<p>Alice &amp; Bob&#39;s policy</p>";
        let source = ingest_html(html).unwrap();
        assert_eq!(source.blocks[0].text, "Alice & Bob's policy");
    }

    #[test]
    fn pdf_adapter_is_explicitly_unsupported() {
        let err = ingest_pdf(b"%PDF-1.4").unwrap_err();
        assert!(matches!(err, CompilerError::UnsupportedFormat(_)));
    }

    #[test]
    fn source_id_fingerprints_raw_bytes() {
        let a = ingest_text(b"same content").unwrap();
        let b = ingest_text(b"same content").unwrap();
        let c = ingest_text(b"different content").unwrap();
        assert_eq!(a.id, b.id);
        assert_ne!(a.id, c.id);
        assert!(a.id.starts_with("sha256:"));
    }
}
