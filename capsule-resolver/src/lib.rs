//! `capsule-resolver`: resolves Knowledge Capsules for a purpose into a
//! resolution bundle — the resolved set with roles and ranks,
//! contradiction bundles, unmet dependencies and other known gaps, and an
//! inspectable plan explaining every inclusion and exclusion. Bundles are
//! digest-bound and can be replayed to detect drift.
//!
//! Requests follow KCP §5 ([`request::ResolutionRequest`]) or the CQL
//! `RESOLVE` form ([`request::parse_cql`]). Candidates come from a
//! `capsule-registry` ([`index::RegistrySource`]) or a set of capsule
//! files ([`index::PackSource`]).

pub mod index;
pub mod request;
pub mod resolve;

pub use index::{CapsuleSource, PackSource, RegistrySource, SemanticIndex};
pub use request::{parse_cql, Include, ResolutionRequest};
pub use resolve::{ResolutionBundle, Resolver, Role};
