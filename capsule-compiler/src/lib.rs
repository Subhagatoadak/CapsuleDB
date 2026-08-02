//! `capsule-compiler`: deterministic source-to-candidate-capsule pipeline
//! (Phase 3 MVP — no LLM step yet).
//!
//! ```text
//! bytes -> Source (structure-preserved text + blocks)
//!       -> claims (sentence-level, span-bound)
//!       -> classify (epistemic status, qualifications, exclusions, conditions)
//!       -> CandidateCapsule (draft Capsule + lineage + deterministic pre-validation)
//! ```
//!
//! Every candidate is a real `capsule_core::Capsule` in `draft` lifecycle
//! state with `capsule_core::validate` already run against it — the
//! "deterministic validation before LLM validation" the roadmap calls
//! for. There is no LLM step in this crate: claims are extracted
//! verbatim, not paraphrased, and confidence is always a conservative
//! heuristic default pending human review. `detect_inflation` exists for
//! when a human or a future LLM step *does* edit a statement relative to
//! its source.

pub mod adapters;
pub mod classify;
pub mod error;
pub mod inflation;
pub mod lineage;
pub mod pipeline;
pub mod segmentation;
pub mod source;

pub use adapters::{ingest_html, ingest_markdown, ingest_pdf, ingest_text};
pub use error::CompilerError;
pub use inflation::{detect_inflation, InflationFinding};
pub use lineage::Lineage;
pub use pipeline::{compile, CandidateCapsule, CompilationReport, CompileOptions, DuplicateFinding};
pub use segmentation::{extract_claims, split_sentences, CandidateClaim};
pub use source::{BlockType, Source, SourceFormat, StructuralBlock};
