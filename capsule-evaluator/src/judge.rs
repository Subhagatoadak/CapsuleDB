//! The bounded judge interface.
//!
//! A judge is asked only residual questions the deterministic pass could
//! not settle, one at a time, each about one claim and one capsule (or one
//! qualification), with a fixed set of allowed labels. It never sees a
//! request to "score the answer". Anything outside the allowed labels is
//! treated as no answer.

use crate::surface::{JudgeDiscretion, ResidualKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JudgeQuestion {
    pub kind: ResidualKind,
    /// The single answer claim (or the whole answer, for qualifications).
    pub claim: String,
    /// The capsule statement the claim is checked against.
    pub capsule_statement: String,
    /// For `QualificationConveyed`: the qualification text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualification: Option<String>,
    /// The only acceptable answers.
    pub labels: Vec<String>,
}

impl JudgeQuestion {
    /// The fixed wording given to a model judge.
    pub fn prompt(&self) -> String {
        let task = match self.kind {
            ResidualKind::ClaimSupport => "Decide whether the CAPSULE STATEMENT supports the CLAIM. Supported means the claim asserts nothing beyond what the statement says, with no stronger certainty and no wider scope.".to_string(),
            ResidualKind::QualificationConveyed => format!(
                "Decide whether the CLAIM conveys this required qualification, in any wording: \"{}\".",
                self.qualification.as_deref().unwrap_or_default()
            ),
        };
        format!(
            "{task}\n\nCAPSULE STATEMENT:\n{}\n\nCLAIM:\n{}\n\nAnswer with exactly one label from: {}.",
            self.capsule_statement,
            self.claim,
            self.labels.join(", ")
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JudgeVerdict {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

pub trait Judge {
    /// Identifies the judge (and model) in reports.
    fn id(&self) -> String;
    fn ask(&self, q: &JudgeQuestion, discretion: JudgeDiscretion) -> Result<JudgeVerdict, String>;
}

/// No judge: every residual question stays unresolved.
pub struct NoJudge;

impl Judge for NoJudge {
    fn id(&self) -> String {
        "none".into()
    }
    fn ask(&self, _q: &JudgeQuestion, _d: JudgeDiscretion) -> Result<JudgeVerdict, String> {
        Err("no judge configured".into())
    }
}

/// Answers from a fixed function (tests, offline replays of recorded
/// verdicts).
pub struct FnJudge<F: Fn(&JudgeQuestion) -> String>(pub F);

impl<F: Fn(&JudgeQuestion) -> String> Judge for FnJudge<F> {
    fn id(&self) -> String {
        "scripted".into()
    }
    fn ask(&self, q: &JudgeQuestion, _d: JudgeDiscretion) -> Result<JudgeVerdict, String> {
        Ok(JudgeVerdict { label: (self.0)(q), rationale: None })
    }
}

/// Keeps a verdict only if its label is one the question allows.
pub fn bounded(q: &JudgeQuestion, v: Result<JudgeVerdict, String>, discretion: JudgeDiscretion) -> Option<JudgeVerdict> {
    let mut v = v.ok()?;
    v.label = v.label.trim().trim_matches(|c: char| !c.is_alphanumeric() && c != '_').to_lowercase();
    if !q.labels.contains(&v.label) {
        return None;
    }
    if discretion != JudgeDiscretion::ClassifyWithRationale {
        v.rationale = None;
    }
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_set_labels_are_discarded() {
        let q = JudgeQuestion {
            kind: ResidualKind::ClaimSupport,
            claim: "c".into(),
            capsule_statement: "s".into(),
            qualification: None,
            labels: vec!["supported".into(), "unsupported".into()],
        };
        let ok = bounded(&q, Ok(JudgeVerdict { label: " Supported. ".into(), rationale: Some("r".into()) }), JudgeDiscretion::Classify);
        assert_eq!(ok, Some(JudgeVerdict { label: "supported".into(), rationale: None }));
        assert_eq!(bounded(&q, Ok(JudgeVerdict { label: "8/10".into(), rationale: None }), JudgeDiscretion::Classify), None);
        assert!(q.prompt().contains("exactly one label"));
    }
}
