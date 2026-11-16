//! `capsule-evaluator`: evaluates answers and decisions against Knowledge
//! Capsule contracts.
//!
//! ```text
//! Answer
//!   → claim decomposition           (text::decompose)
//!   → capsule mapping               (lexical + numeric, against a CapsulePack)
//!   → deterministic validation      (unsupported / numeric / superseded / rules)
//!   → epistemic validation          (applicability, inference→fact,
//!                                    association→causation, confidence ceiling,
//!                                    qualifications, contradictions,
//!                                    evidence multiplication, overclaims)
//!   → bounded judge                 (residual questions only, fixed labels)
//!   → evaluation report + scores
//! ```
//!
//! What gets checked, with which thresholds, and which residual questions
//! a judge may answer is declared in an evaluation-surface manifest
//! ([`surface::SurfaceManifest`]).

pub mod anthropic;
pub mod evaluate;
pub mod judge;
pub mod pack;
pub mod surface;
pub mod text;

pub use evaluate::{evaluate, EvaluationReport, Finding, FindingKind, Severity, Verdict};
pub use judge::{Judge, NoJudge};
pub use pack::CapsulePack;
pub use surface::SurfaceManifest;
