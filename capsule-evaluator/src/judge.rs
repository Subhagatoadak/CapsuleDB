//! The bounded LLM-as-a-judge plug point.
//!
//! There is no LLM integration in this repository — `NullJudge` always
//! abstains. What's real here is the *boundary*: a judge is only ever
//! consulted for claims the deterministic layer left genuinely ambiguous
//! (weak-but-not-strong lexical support; see `mapping::ClaimMapping::is_ambiguous`),
//! never for claims it already has a verdict on, and [`DiscretionLevel`]
//! controls how wide that ambiguous band is allowed to be before a judge
//! gets asked at all. Wiring in a real judge means implementing [`Judge`]
//! against an LLM call; nothing else in this crate needs to change.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DiscretionLevel {
    /// Never consult a judge; ambiguous claims are reported as
    /// unsupported.
    None,
    Low,
    Medium,
    High,
}

impl DiscretionLevel {
    /// The weak-support threshold widens as discretion increases: at
    /// `None` it equals the strong threshold (no ambiguous band at all,
    /// so nothing is ever "referred"); at `High` the band is widest, so
    /// more borderline claims get a chance at a judge verdict instead of
    /// being flatly reported as unsupported.
    pub fn weak_threshold(self, strong_threshold: f64) -> f64 {
        match self {
            DiscretionLevel::None => strong_threshold,
            DiscretionLevel::Low => (strong_threshold - 0.10).max(0.0),
            DiscretionLevel::Medium => (strong_threshold - 0.20).max(0.0),
            DiscretionLevel::High => (strong_threshold - 0.30).max(0.0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResidualQuestion {
    pub claim_index: usize,
    pub claim_text: String,
    pub candidate_capsules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JudgeVerdict {
    /// No judge was consulted, or the judge declined to rule.
    Abstain,
    Uphold(String),
    Overrule(String),
}

pub trait Judge {
    fn judge(&self, question: &ResidualQuestion) -> JudgeVerdict;
}

/// The default, and currently only, judge implementation: always
/// abstains. A real judge (LLM-backed or otherwise) implements [`Judge`]
/// and is passed to [`crate::evaluate`] in its place.
pub struct NullJudge;

impl Judge for NullJudge {
    fn judge(&self, _question: &ResidualQuestion) -> JudgeVerdict {
        JudgeVerdict::Abstain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discretion_widens_the_ambiguous_band() {
        assert_eq!(DiscretionLevel::None.weak_threshold(0.4), 0.4);
        assert!(DiscretionLevel::Low.weak_threshold(0.4) < 0.4);
        assert!(DiscretionLevel::High.weak_threshold(0.4) < DiscretionLevel::Low.weak_threshold(0.4));
    }

    #[test]
    fn null_judge_always_abstains() {
        let question = ResidualQuestion { claim_index: 0, claim_text: "x".to_string(), candidate_capsules: vec![] };
        assert_eq!(NullJudge.judge(&question), JudgeVerdict::Abstain);
    }
}
