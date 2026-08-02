//! Answer claim decomposition: reuses `capsule-compiler`'s sentence
//! segmentation rather than reimplementing it — an answer's claims are
//! extracted the same way a source document's claims are.

use capsule_compiler::split_sentences;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerClaim {
    pub text: String,
    /// Byte offsets into the original answer text.
    pub start: usize,
    pub end: usize,
}

pub fn decompose_claims(answer: &str) -> Vec<AnswerClaim> {
    split_sentences(answer)
        .into_iter()
        .filter_map(|(start, end)| {
            let text = answer[start..end].trim();
            if text.is_empty() {
                None
            } else {
                let offset = answer[start..end].len() - answer[start..end].trim_start().len();
                Some(AnswerClaim { text: text.to_string(), start: start + offset, end: start + offset + text.len() })
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_answer_into_claims() {
        let claims = decompose_claims("Data must be retained for 90 days. This does not apply to minors.");
        assert_eq!(claims.len(), 2);
        assert_eq!(claims[0].text, "Data must be retained for 90 days.");
    }
}
