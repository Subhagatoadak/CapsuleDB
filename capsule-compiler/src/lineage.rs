//! Source-to-capsule transformation lineage: which source, which block,
//! which exact span, and which deterministic rules produced a candidate.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lineage {
    pub source_id: String,
    pub block_index: usize,
    /// Byte offsets into the source's normalized text.
    pub span: (usize, usize),
    pub rules_applied: Vec<String>,
}
