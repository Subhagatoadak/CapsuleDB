//! Semantic-boundary detection (sentence segmentation) and claim
//! extraction with exact source-span binding.
//!
//! Sentence boundaries are heuristic: punctuation-based with a small
//! abbreviation guard list. It will misfire on unusual prose; it is not a
//! substitute for a real sentence tokenizer, but it is deterministic,
//! dependency-free, and good enough for a first extraction pass that a
//! human reviews before publication.

use crate::source::{BlockType, Source};

const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "sr", "jr", "vs", "etc", "e.g", "i.e", "u.s", "inc", "ltd", "no", "fig", "approx",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateClaim {
    pub text: String,
    /// Byte offsets into the owning `Source.text`.
    pub start: usize,
    pub end: usize,
    pub block_index: usize,
}

/// Extracts one candidate claim per sentence from every non-heading block.
/// Headings are structural context, not claims.
pub fn extract_claims(source: &Source) -> Vec<CandidateClaim> {
    let mut claims = Vec::new();
    for (block_index, block) in source.blocks.iter().enumerate() {
        if matches!(block.block_type, BlockType::Heading(_)) {
            continue;
        }
        for (start, end) in split_sentences(&block.text) {
            let text = block.text[start..end].trim();
            if text.is_empty() {
                continue;
            }
            let trim_offset = block.text[start..end].len() - block.text[start..end].trim_start().len();
            claims.push(CandidateClaim {
                text: text.to_string(),
                start: block.start + start + trim_offset,
                end: block.start + start + trim_offset + text.len(),
                block_index,
            });
        }
    }
    claims
}

/// Returns byte-offset `(start, end)` spans of each sentence in `text`.
pub fn split_sentences(text: &str) -> Vec<(usize, usize)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut spans = Vec::new();
    let mut sentence_start = 0usize;
    let mut i = 0usize;

    while i < chars.len() {
        let (idx, c) = chars[i];
        if c == '.' || c == '!' || c == '?' {
            let mut j = i;
            while j + 1 < chars.len() && matches!(chars[j + 1].1, '.' | '!' | '?') {
                j += 1;
            }
            let punct_end = chars[j].0 + chars[j].1.len_utf8();
            let rest = &text[punct_end..];
            let next_non_space = rest.trim_start();
            let is_boundary_shape = rest.is_empty() || rest.starts_with(char::is_whitespace);
            let preceding_word = word_ending_at(text, idx);
            let is_abbreviation = ABBREVIATIONS.contains(&preceding_word.to_lowercase().as_str());
            let next_starts_new_sentence =
                next_non_space.is_empty() || next_non_space.chars().next().map(|c| !c.is_lowercase()).unwrap_or(true);

            if is_boundary_shape && !is_abbreviation && next_starts_new_sentence {
                spans.push((sentence_start, punct_end));
                let mut k = punct_end;
                while k < text.len() && text.as_bytes()[k].is_ascii_whitespace() {
                    k += 1;
                }
                sentence_start = k;
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }

    if sentence_start < text.len() && !text[sentence_start..].trim().is_empty() {
        spans.push((sentence_start, text.len()));
    }
    spans
}

fn word_ending_at(text: &str, punct_idx: usize) -> &str {
    let prefix = &text[..punct_idx];
    let start = prefix.rfind(|c: char| c.is_whitespace()).map(|p| p + 1).unwrap_or(0);
    &prefix[start..]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{BlockType, StructuralBlock};

    #[test]
    fn splits_simple_sentences() {
        let spans = split_sentences("Data must be retained. Except for legal holds. Is that clear?");
        let sentences: Vec<&str> = spans.iter().map(|(s, e)| "Data must be retained. Except for legal holds. Is that clear?"[*s..*e].trim()).collect();
        assert_eq!(sentences, vec!["Data must be retained.", "Except for legal holds.", "Is that clear?"]);
    }

    #[test]
    fn does_not_split_on_abbreviations() {
        let text = "The policy applies to the U.S. and its territories.";
        let spans = split_sentences(text);
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn handles_missing_trailing_punctuation() {
        let spans = split_sentences("A single sentence with no period at the end");
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn extract_claims_skips_headings_and_binds_spans() {
        let source = Source {
            id: "sha256:test".to_string(),
            format: crate::source::SourceFormat::Text,
            text: "Title\n\nFirst claim. Second claim.".to_string(),
            blocks: vec![
                StructuralBlock { block_type: BlockType::Heading(1), start: 0, end: 5, text: "Title".to_string() },
                StructuralBlock { block_type: BlockType::Paragraph, start: 7, end: 34, text: "First claim. Second claim.".to_string() },
            ],
        };
        let claims = extract_claims(&source);
        assert_eq!(claims.len(), 2);
        assert_eq!(claims[0].text, "First claim.");
        assert_eq!(&source.text[claims[0].start..claims[0].end], "First claim.");
        assert_eq!(claims[1].text, "Second claim.");
        assert_eq!(&source.text[claims[1].start..claims[1].end], "Second claim.");
    }
}
