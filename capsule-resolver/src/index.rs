//! Candidate sources and the discovery indexes the resolver uses.
//!
//! - **Lexical index**: normalized term → capsules, with a coverage score
//!   of the query's terms. Deterministic and explainable.
//! - **Semantic index** (optional): any [`SemanticIndex`] implementation
//!   can add a similarity score; none is required.
//! - **Dependency index** and **contradiction index**: both directions of
//!   every `requires`/`refines`/`supersedes` and unresolved contradiction.

use capsule_core::{Capsule, DependencyRelation, LifecycleState, ResolutionStatus};
use std::collections::{BTreeMap, BTreeSet};

/// One capsule version offered to the resolver.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub capsule: Capsule,
    /// The registry's state (authoritative), not the document's field.
    pub lifecycle_state: LifecycleState,
    pub content_hash: String,
}

impl Candidate {
    pub fn key(&self) -> String {
        format!("{}@{}", self.capsule.logical_id, self.capsule.version)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("registry: {0}")]
    Registry(#[from] capsule_registry::RegistryError),
    #[error("{0}")]
    Other(String),
}

pub trait CapsuleSource {
    fn candidates(&self) -> Result<Vec<Candidate>, SourceError>;
}

/// A fixed set of capsule documents (files, an in-memory pack). The
/// document's own `lifecycle_state` is taken as its state.
pub struct PackSource(pub Vec<Capsule>);

impl CapsuleSource for PackSource {
    fn candidates(&self) -> Result<Vec<Candidate>, SourceError> {
        self.0
            .iter()
            .map(|c| {
                Ok(Candidate {
                    capsule: c.clone(),
                    lifecycle_state: c.lifecycle_state,
                    content_hash: capsule_core::content_hash(c).map_err(|e| SourceError::Other(e.to_string()))?,
                })
            })
            .collect()
    }
}

impl PackSource {
    pub fn load(paths: &[impl AsRef<std::path::Path>]) -> Result<Self, SourceError> {
        let mut out = Vec::new();
        for p in paths {
            let p = p.as_ref();
            let files: Vec<std::path::PathBuf> = if p.is_dir() {
                let mut v: Vec<_> = std::fs::read_dir(p)
                    .map_err(|e| SourceError::Other(format!("{}: {e}", p.display())))?
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|f| f.extension().is_some_and(|x| x == "json"))
                    .collect();
                v.sort();
                v
            } else {
                vec![p.to_path_buf()]
            };
            for f in files {
                let src = std::fs::read_to_string(&f).map_err(|e| SourceError::Other(format!("{}: {e}", f.display())))?;
                out.push(capsule_core::parse(&src).map_err(|e| SourceError::Other(format!("{}: {e}", f.display())))?);
            }
        }
        Ok(Self(out))
    }
}

/// Every registered version in a `capsule-registry`, with its derived
/// lifecycle state.
pub struct RegistrySource<'a>(pub &'a capsule_registry::Registry);

impl CapsuleSource for RegistrySource<'_> {
    fn candidates(&self) -> Result<Vec<Candidate>, SourceError> {
        let mut out = Vec::new();
        for id in self.0.list_logical_ids()? {
            for r in self.0.list_versions(&id)? {
                out.push(Candidate { capsule: r.capsule, lifecycle_state: r.lifecycle_state, content_hash: r.content_hash });
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------ terms

const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "of", "to", "in", "on", "for", "by", "with", "at", "from", "as", "is", "are", "was", "were", "be",
    "been", "this", "that", "these", "those", "it", "its", "than", "into", "about", "after", "before", "during", "per", "which", "will",
    "can", "should", "has", "have", "had", "do", "does", "not", "no", "all", "any", "more", "most",
];

fn stem(w: &str) -> String {
    let n = w.chars().count();
    for (suf, min) in [("ations", 7), ("ation", 6), ("ing", 5), ("ies", 5), ("ed", 4), ("es", 4), ("ly", 5)] {
        if n >= min && w.ends_with(suf) {
            let base = &w[..w.len() - suf.len()];
            return if suf == "ies" { format!("{base}y") } else { base.trim_end_matches('e').to_string() };
        }
    }
    if n >= 4 && w.ends_with('s') && !w.ends_with("ss") {
        return w[..w.len() - 1].trim_end_matches('e').to_string();
    }
    w.trim_end_matches('e').to_string()
}

/// Normalized terms of a text.
pub fn terms(text: &str) -> BTreeSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1 && !STOPWORDS.contains(w) && !w.chars().all(|c| c.is_ascii_digit()))
        .map(stem)
        .collect()
}

/// Share of `query` terms found in `doc`.
pub fn coverage(query: &BTreeSet<String>, doc: &BTreeSet<String>) -> f64 {
    if query.is_empty() {
        return 0.0;
    }
    query.intersection(doc).count() as f64 / query.len() as f64
}

/// A pluggable similarity scorer (embeddings, a search service, ...).
pub trait SemanticIndex {
    /// Similarity in `[0, 1]` between the query and a capsule.
    fn score(&self, query: &str, capsule: &Capsule) -> f64;
}

/// Inverted index over statement, conditions, domain, and logical id.
pub struct LexicalIndex {
    docs: BTreeMap<String, BTreeSet<String>>,
    postings: BTreeMap<String, BTreeSet<String>>,
}

impl LexicalIndex {
    pub fn build(cands: &[Candidate]) -> Self {
        let mut docs = BTreeMap::new();
        let mut postings: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for c in cands {
            let cap = &c.capsule;
            let mut text = format!("{} {} {}", cap.statement, cap.governance.domain, cap.logical_id.replace(['/', '-', ':'], " "));
            if let Some(a) = &cap.applicability {
                for cond in &a.conditions {
                    text.push(' ');
                    text.push_str(cond);
                }
            }
            let t = terms(&text);
            for term in &t {
                postings.entry(term.clone()).or_default().insert(c.key());
            }
            docs.insert(c.key(), t);
        }
        Self { docs, postings }
    }

    /// Capsules sharing at least one query term, with their coverage.
    pub fn search(&self, query: &str) -> BTreeMap<String, f64> {
        let q = terms(query);
        let mut hits = BTreeSet::new();
        for t in &q {
            if let Some(p) = self.postings.get(t) {
                hits.extend(p.iter().cloned());
            }
        }
        hits.into_iter().map(|k| {
            let s = coverage(&q, &self.docs[&k]);
            (k, (s * 1000.0).round() / 1000.0)
        }).collect()
    }

    pub fn terms_of(&self, key: &str) -> Option<&BTreeSet<String>> {
        self.docs.get(key)
    }
}

/// Edges of one kind between capsule references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: String,
    /// As written: a logical id, optionally `@version`.
    pub to: String,
    pub relation: DependencyRelation,
}

pub struct GraphIndex {
    pub dependencies: Vec<Edge>,
    /// (capsule key, referenced capsule as written, description)
    pub contradictions: Vec<(String, String, String)>,
}

impl GraphIndex {
    pub fn build(cands: &[Candidate]) -> Self {
        let mut dependencies = Vec::new();
        let mut contradictions = Vec::new();
        for c in cands {
            for d in &c.capsule.dependencies {
                dependencies.push(Edge { from: c.key(), to: d.capsule_id.clone(), relation: d.relation });
            }
            for x in &c.capsule.contradictions {
                if x.resolution_status == ResolutionStatus::Unresolved {
                    contradictions.push((c.key(), x.capsule_id.clone(), x.description.clone()));
                }
            }
        }
        Self { dependencies, contradictions }
    }

    pub fn dependencies_of<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Edge> + 'a {
        self.dependencies.iter().filter(move |e| e.from == key)
    }
}

/// Does a reference (`id` or `id@version`) name this candidate?
pub fn refers_to(reference: &str, c: &Candidate) -> bool {
    reference == c.capsule.logical_id || reference == c.key()
}
