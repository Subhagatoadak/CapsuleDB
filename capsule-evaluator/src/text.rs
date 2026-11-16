//! Deterministic text handling: claim decomposition, normalization,
//! lexical similarity, number and period extraction, and the cue
//! lexicons the detectors use.
//!
//! Everything here is lexical and explainable on purpose. A finding cites
//! the exact cue that triggered it, so a reviewer can see why, and the
//! same input always produces the same report.

use regex::Regex;
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// One decomposed answer claim with its byte span in the answer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AnswerClaim {
    pub index: usize,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// Splits an answer into claims: sentences (via the compiler's
/// segmenter), then independent clauses joined by `;`. Questions and
/// fragments with fewer than two content words are not claims.
pub fn decompose(answer: &str) -> Vec<AnswerClaim> {
    let mut out = Vec::new();
    for (s, e) in capsule_compiler::split_sentences(answer) {
        let sentence = &answer[s..e];
        let mut offset = 0;
        for part in sentence.split(';') {
            let lead = part.len() - part.trim_start().len();
            let text = part.trim();
            let start = s + offset + lead;
            offset += part.len() + 1;
            if text.is_empty() || text.ends_with('?') || content_tokens(text).len() < 2 {
                continue;
            }
            out.push(AnswerClaim { index: out.len(), text: text.to_string(), start, end: start + text.len() });
        }
    }
    out
}

const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "of", "to", "in", "on", "for", "by", "with", "at", "from", "as", "is", "are", "was", "were",
    "be", "been", "being", "this", "that", "these", "those", "it", "its", "their", "there", "than", "then", "so", "such", "into",
    "about", "over", "after", "before", "during", "based", "per", "which", "who", "whom", "what", "will", "would", "can", "could",
    "should", "has", "have", "had", "do", "does", "did", "we", "our", "you", "your", "they", "them", "he", "she", "his", "her",
    "i", "me", "my", "also", "any", "all", "some", "more", "most", "very", "only", "not", "no",
];

/// Light suffix stripping so "campaigns"/"campaign" and
/// "estimated"/"estimate" compare equal.
pub fn stem(word: &str) -> String {
    let w = word.trim_matches('\'');
    let n = w.chars().count();
    for (suffix, min) in [("ations", 7), ("ation", 6), ("ings", 6), ("ing", 5), ("ies", 5), ("ied", 5), ("ed", 4), ("es", 4), ("ly", 5)] {
        if n >= min && w.ends_with(suffix) {
            let base = &w[..w.len() - suffix.len()];
            return match suffix {
                "ies" | "ied" => format!("{base}y"),
                _ => base.trim_end_matches('e').to_string(),
            };
        }
    }
    if n >= 4 && w.ends_with('s') && !w.ends_with("ss") && !w.ends_with("us") {
        return w[..w.len() - 1].trim_end_matches('e').to_string();
    }
    w.trim_end_matches('e').to_string()
}

fn synonym(w: &str) -> &str {
    match w {
        "one" => "single",
        "hasn't" | "hasnt" | "haven't" | "isn't" | "aren't" | "wasn't" | "never" => "not",
        "unreplicated" => "replicat",
        "replication" | "replicated" | "replicate" | "replicates" => "replicat",
        "estimate" | "estimates" | "estimated" | "estimation" => "estimat",
        "retain" | "retained" | "retention" | "kept" | "keep" => "retain",
        "deleted" | "delete" | "deletion" | "erased" | "purged" => "delet",
        "uplift" => "lift",
        _ => w,
    }
}

/// Lowercased word tokens (letters, digits, apostrophes).
pub fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Normalized content tokens: stopwords removed, synonyms folded, stemmed.
pub fn content_tokens(text: &str) -> BTreeSet<String> {
    words(text)
        .into_iter()
        .filter(|w| !STOPWORDS.contains(&w.as_str()) && !w.chars().all(|c| c.is_ascii_digit()))
        .map(|w| {
            let s = synonym(&w);
            if s != w {
                s.to_string()
            } else {
                stem(&w)
            }
        })
        .filter(|w| w.len() > 1)
        .collect()
}

/// Overlap coefficient of content tokens: |A ∩ B| / min(|A|, |B|).
pub fn overlap(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    let small = a.len().min(b.len());
    if small == 0 {
        return 0.0;
    }
    a.intersection(b).count() as f64 / small as f64
}

/// Share of `needle` tokens present in `hay`.
pub fn coverage(needle: &BTreeSet<String>, hay: &BTreeSet<String>) -> f64 {
    if needle.is_empty() {
        return 1.0;
    }
    needle.intersection(hay).count() as f64 / needle.len() as f64
}

fn number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d+(?:\.\d+)?\b").expect("valid regex"))
}

/// Numbers mentioned as standalone figures (e.g. `6`, `9`, `0.6`, `90`),
/// excluding four-digit years and digits inside labels such as `Q1`.
pub fn numbers(text: &str) -> BTreeSet<String> {
    number_re()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .filter(|n| !(n.len() == 4 && (n.starts_with("19") || n.starts_with("20"))))
        .collect()
}

/// A calendar period mentioned in text, as inclusive ISO dates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Period {
    pub label: String,
    pub from: String,
    pub until: String,
}

fn period_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(?:Q([1-4])\s*(20\d\d)|(?:H([12])\s*(20\d\d))|((?:19|20)\d\d))\b").expect("valid regex"))
}

/// Quarters (`Q3 2026`), halves (`H1 2026`), and bare years.
pub fn periods(text: &str) -> Vec<Period> {
    period_re()
        .captures_iter(text)
        .map(|c| {
            let label = c.get(0).map(|m| m.as_str().to_string()).unwrap_or_default();
            if let (Some(q), Some(y)) = (c.get(1), c.get(2)) {
                let q: u32 = q.as_str().parse().unwrap_or(1);
                let (m0, m1, d1) = match q {
                    1 => (1, 3, 31),
                    2 => (4, 6, 30),
                    3 => (7, 9, 30),
                    _ => (10, 12, 31),
                };
                Period { label, from: format!("{}-{m0:02}-01", y.as_str()), until: format!("{}-{m1:02}-{d1}", y.as_str()) }
            } else if let (Some(h), Some(y)) = (c.get(3), c.get(4)) {
                if h.as_str() == "1" {
                    Period { label, from: format!("{}-01-01", y.as_str()), until: format!("{}-06-30", y.as_str()) }
                } else {
                    Period { label, from: format!("{}-07-01", y.as_str()), until: format!("{}-12-31", y.as_str()) }
                }
            } else {
                let y = c.get(5).map(|m| m.as_str()).unwrap_or("2000");
                Period { label, from: format!("{y}-01-01"), until: format!("{y}-12-31") }
            }
        })
        .collect()
}

/// Finds the first cue from `cues` contained in `text` (lowercased,
/// space-padded so cues can carry word boundaries).
pub fn find_cue<'a>(text: &str, cues: &[&'a str]) -> Option<&'a str> {
    let padded = format!(" {} ", text.to_lowercase().replace(['.', ',', ';', ':', '(', ')', '"'], " "));
    cues.iter().copied().find(|c| padded.contains(c))
}

// ------------------------------------------------------------- lexicons

/// Hedges: language that keeps a claim tentative.
pub const HEDGE_CUES: &[&str] = &[
    " may ", " might ", " could ", " suggest", " appears ", " appear to ", " estimat", " approximately ", " approx ", " around ",
    " about ", " roughly ", " likely ", " probably ", " possibly ", " preliminary ", " tentative", " indicat", " uncertain",
    " not yet ", " unconfirmed", " modelled ", " modeled ", " projected ", " forecast", " ~",
];

/// Language that hardens a claim into established fact.
pub const FACT_CUES: &[&str] = &[
    " is a fact", " proves ", " proven ", " prove that ", " establishes ", " established ", " definitively ", " definitely ",
    " certainly ", " undeniabl", " conclusively ", " confirmed ", " guarantee", " always ", " without doubt", " no doubt ",
    " clearly shows", " we observed ", " was observed ", " were observed ", " measured ",
];

/// Causal claims.
pub const CAUSAL_CUES: &[&str] = &[
    " causes ", " caused ", " cause ", " causing ", " drives ", " drove ", " driven by ", " leads to ", " led to ", " results in ",
    " resulted in ", " because of ", " due to ", " is responsible for ", " makes ", " increases ", " boosts ",
];

/// Associative (non-causal) language in evidence.
pub const ASSOCIATIVE_CUES: &[&str] = &[
    " associated with", " association ", " correlat", " linked to ", " link between ", " related to ", " relationship between ",
    " co-occur", " observational ",
];

/// Designs that license causal language.
pub const CAUSAL_DESIGN_CUES: &[&str] = &[" experiment", " randomi", " holdout", " rct ", " controlled trial", " a/b test", " causal "];

/// Overgeneralization.
pub const GENERALIZATION_CUES: &[&str] = &[
    " all ", " every ", " always ", " any ", " across ", " universally ", " in general ", " generally ", " everywhere ",
    " regardless ", " in all cases ", " for all ", " going forward ", " in future ", " in the future ", " every quarter ",
    " other channels ", " all channels ", " each quarter ", " permanently ",
];

/// Language that limits scope (keeps an exclusion in force).
pub const LIMITING_CUES: &[&str] =
    &[" unless ", " except ", " excluding ", " only ", " not ", " other than ", " provided that ", " subject to ", " does not apply "];

/// Acknowledging dispute or contradiction.
pub const DISPUTE_CUES: &[&str] = &[
    " disputed", " contested", " however ", " but ", " challenged", " challenge", " conflicting", " contradict", " audit",
    " caveat", " questioned", " uncertain", " may be biased", " bias", " not fully randomi", " open question",
];

/// Claims that evidence is independent or multiplied.
pub const INDEPENDENCE_CUES: &[&str] = &[
    " multiple studies", " several studies", " many studies", " multiple sources", " several sources", " many sources",
    " independent", " independently", " replicated", " corroborated", " consistent across", " two studies", " both studies",
    " numerous ", " repeatedly shown", " widely confirmed",
];

/// Negation, used to tell "X is not Y" from "X is Y".
pub const NEGATION_CUES: &[&str] = &[" not ", " no ", " never ", " cannot ", " can't ", " isn't ", " doesn't ", " don't ", " won't ", " neither "];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decomposes_sentences_and_clauses() {
        let c = decompose("Paid search lifted conversions by 6-9% in Q1 2026; the result was replicated. Why? Ok.");
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].text, "Paid search lifted conversions by 6-9% in Q1 2026");
        assert_eq!(c[1].text, "the result was replicated.");
        let answer = "Paid search lifted conversions by 6-9% in Q1 2026; the result was replicated. Why? Ok.";
        assert_eq!(&answer[c[1].start..c[1].end], c[1].text);
    }

    #[test]
    fn tokens_fold_inflections_and_synonyms() {
        let a = content_tokens("Campaigns produced an estimated lift");
        let b = content_tokens("campaign estimate lifts");
        assert!(overlap(&a, &b) > 0.99, "{a:?} {b:?}");
        assert!(content_tokens("hasn't been replicated").contains("not"));
    }

    #[test]
    fn extracts_numbers_and_periods() {
        assert_eq!(numbers("6-9% in 2026, ceiling 0.6"), ["0.6", "6", "9"].into_iter().map(String::from).collect());
        assert!(numbers("in Q1 2026").is_empty());
        let p = periods("in Q3 2026 and 2027");
        assert_eq!(p[0], Period { label: "Q3 2026".into(), from: "2026-07-01".into(), until: "2026-09-30".into() });
        assert_eq!(p[1].from, "2027-01-01");
    }

    #[test]
    fn cues_respect_word_boundaries() {
        assert_eq!(find_cue("It causes harm.", CAUSAL_CUES), Some(" causes "));
        assert_eq!(find_cue("The because-clause", CAUSAL_CUES), None);
        assert!(find_cue("This may help", HEDGE_CUES).is_some());
        assert!(find_cue("mayor", HEDGE_CUES).is_none());
    }
}
