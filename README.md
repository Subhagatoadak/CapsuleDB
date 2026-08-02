# CapsuleDB

> **Knowledge, with its uncertainty intact.**

CapsuleDB is an **epistemic knowledge database** designed to store, transport, resolve, update, and evaluate governed units of knowledge called **Knowledge Capsules**.

Traditional databases preserve data integrity. CapsuleDB is intended to preserve **epistemic integrity**: what is known, how it is known, how strongly it is supported, where it applies, what contradicts it, and how it changes over time.

## Why CapsuleDB

Conventional knowledge systems usually follow this pattern:

```text
Store → Chunk → Embed → Retrieve → Generate
```

That approach is useful for discovery, but it can lose source qualifications, uncertainty, authority, applicability, version history, and contradictions. It also creates a large evaluation surface for LLM-as-a-judge systems because the evaluator must reconstruct the truth conditions from raw documents and unstructured context.

CapsuleDB proposes a different model:

```text
Source
  → Compile
  → Verify
  → Govern
  → Package
  → Resolve
  → Use
  → Evaluate
  → Evolve
```

## Core object: Knowledge Capsule

A Knowledge Capsule is a portable, versioned, self-describing, and independently governed unit of knowledge.

A capsule contains more than text:

```text
Knowledge Capsule
├── Knowledge statement
├── Context and applicability
├── Evidence and provenance
├── Epistemic status
├── Confidence basis and ceiling
├── Assumptions and limitations
├── Contradictions and known gaps
├── Governance and permissions
├── Lifecycle and version history
├── Dependencies
└── Evaluation contract
```

A capsule should remain understandable and governable when moved between people, applications, AI agents, projects, and organisations.

## Founding principles

1. **Knowledge must remain portable without becoming contextless.**
2. **Uncertainty must not disappear during extraction, summarisation, retrieval, or generation.**
3. **No transformation may increase apparent certainty without new evidence and recorded justification.**
4. **Retrieval must resolve valid knowledge, not merely return semantically similar text.**
5. **Knowledge changes must propagate to dependent capsules, agents, answers, evaluations, and decisions.**
6. **LLM judgement should be bounded by explicit capsule contracts wherever possible.**
7. **Vector indexes, graphs, relational stores, and search engines are implementation indexes, not the source of epistemic truth.**

## Category

CapsuleDB is intended to establish a new category:

### Epistemic Knowledge Database

| System | Primary unit | Primary optimisation |
| --- | --- | --- |
| Relational database | Row | Transactions and structured queries |
| Document database | Document | Flexible structured persistence |
| Vector database | Embedding | Semantic similarity |
| Knowledge graph | Node and edge | Relationship traversal |
| Search engine | Indexed document | Lexical discovery |
| **CapsuleDB** | **Knowledge Capsule** | **Valid, governed knowledge resolution** |

Vector databases find similar information. CapsuleDB should resolve the smallest authoritative, applicable, current, and sufficient set of knowledge needed for a specific request.

## Core platform components

```text
Sources
  ↓
Capsule Compiler
  ↓
Capsule Registry
  ↓
Capsule Resolver
  ↓
Capsule Runtime
  ↓
Humans | AI agents | Applications | Evaluators
```

### Capsule Compiler

Transforms source material into candidate capsules while preserving claims, qualifications, evidence, uncertainty, and source structure.

### Capsule Registry

Manages capsule identity, immutable versions, lifecycle, ownership, authority, dependencies, signatures, and publication state.

### Capsule Resolver

Resolves knowledge by purpose, identity, applicability, authority, time, evidence sufficiency, dependencies, and contradictions.

### Capsule Runtime

Serves capsules locally or remotely, enforces policies, adapts capsule views, records usage, and distributes updates.

### Capsule Evaluator

Evaluates answers and decisions against capsule contracts using deterministic checks first and bounded LLM judgement only for residual semantic questions.

### Capsule Observatory

Monitors freshness, epistemic fidelity, contradictions, confidence laundering, usage, dependency health, and knowledge debt.

## Native operations

CapsuleDB should make the following operations first class:

```text
COMPILE
VALIDATE
APPROVE
PUBLISH
RESOLVE
VERIFY
COMPOSE
SUPERSEDE
REVOKE
EVOLVE
```

Deleting approved knowledge should not be a normal CRUD action. Published capsules should be superseded or revoked so their historical use remains auditable.

## Epistemic transparency

Each capsule should preserve:

- epistemic status: observed, inferred, estimated, modelled, policy-defined, expert judgement, disputed, or unknown
- exact evidence and source spans
- source authority
- confidence basis rather than only a numeric score
- confidence ceiling
- assumptions and limitations
- applicability and exclusions
- contradictory evidence
- independent evidence families
- transformation history
- temporal validity

## Confidence laundering controls

Confidence laundering occurs when tentative or weak evidence becomes increasingly authoritative as it moves through systems.

```text
“Early evidence suggests…”
        ↓
“Evidence shows…”
        ↓
“Established fact”
```

CapsuleDB should detect transformations such as:

- may → does
- suggests → proves
- associated with → causes
- estimated → observed
- moderate confidence → certain
- limited evidence → established evidence

A core rule is:

> The confidence of a conclusion cannot exceed the confidence ceiling of its critical unsupported dependency.

## Evaluation surface reduction

LLM-as-a-judge is useful, but a judge should not be asked to discover the evidence, interpret the standard, determine applicability, detect exceptions, and score the answer at the same time.

Capsules reduce evaluation surface risk by providing:

- canonical claims
- permitted interpretations
- mandatory qualifications
- prohibited overclaims
- applicability boundaries
- confidence ceilings
- contradiction status
- evaluation rules

The target evaluation architecture is:

```text
Answer
  → Claim decomposition
  → Capsule resolution
  → Deterministic validation
  → Epistemic validation
  → Bounded LLM judge
  → Evaluation report
```

## Capsule Query Language

A future Capsule Query Language, or CQL, should support explicit knowledge resolution.

```text
RESOLVE policy_rule
ABOUT "customer data retention"
FOR purpose "privacy review"
IN jurisdiction "India"
AS OF "2026-08-01"
REQUIRE status PUBLISHED
INCLUDE exceptions
INCLUDE contradictions
WITH dependencies
```

For evaluation:

```text
VERIFY answer
AGAINST PACK "marketing-incrementality"
CHECK factual_support
CHECK epistemic_fidelity
CHECK confidence_ceiling
CHECK applicability
CHECK unsupported_claims
```

## Local and remote use

CapsuleDB should support the same operating model across environments:

```text
Compile → Validate → Publish → Resolve → Use → Evaluate → Update
```

Deployment modes:

- local CLI with SQLite or embedded storage
- embedded runtime inside an application
- Docker-based team deployment
- remote enterprise service
- Kubernetes deployment
- centrally governed registry with synchronised local runtimes

Portable capsule packages may use the `.kcap` format and the Knowledge Capsule Protocol, or KCP.

## Proposed technology direction

- **Rust** for capsule core, validation, lifecycle, resolution, integrity, and high-throughput runtime
- **Python** for LLM integration, NLP, extraction, experimentation, and evaluation research
- **TypeScript** for Capsule Studio, dashboards, and web SDKs
- **PostgreSQL** for transactional metadata and temporal state during early development
- **Object storage** for sources, evidence, and `.kcap` packages
- optional lexical, semantic, dependency, and contradiction indexes

The initial goal is not to build a new disk engine. The initial innovation is the capsule object model, compiler, resolver, epistemic rules, protocol, and evaluation system.

# TODO Roadmap

## Phase 0 — Product and protocol definition

- [ ] Finalise the CapsuleDB product charter
- [x] Define the minimum Knowledge Capsule specification ([spec](docs/specification/knowledge-capsule-spec.md))
- [x] Define capsule identity conventions such as `kc://domain/topic/name`
- [x] Define immutable logical identity and content-addressed identity
- [x] Define capsule types: fact, rule, procedure, metric, interpretation, boundary, counterexample, and rubric
- [x] Define lifecycle states
- [x] Define epistemic status vocabulary
- [x] Define authority and evidence vocabularies
- [x] Define confidence basis and confidence ceiling semantics
- [x] Define applicability and exclusion semantics
- [x] Define contradiction and supersession semantics
- [x] Draft Knowledge Capsule Protocol v0.1 ([protocol](protocol/KCP-v0.1.md))
- [x] Draft `.kcap` package structure ([format](protocol/kcap-package-format.md))
- [x] Create canonical sample capsules ([examples](examples/capsules/))

## Phase 1 — Capsule Core MVP

- [x] Create the Rust workspace ([`Cargo.toml`](Cargo.toml))
- [x] Implement capsule core types ([`capsule-core/src/types.rs`](capsule-core/src/types.rs))
- [x] Add JSON Schema support ([`schemas/capsule.schema.json`](schemas/capsule.schema.json))
- [x] Add capsule parsing and serialisation
- [x] Add schema-version validation
- [x] Add logical ID validation
- [x] Add semantic version handling
- [x] Add content hashing ([`capsule-core/src/hashing.rs`](capsule-core/src/hashing.rs))
- [x] Add source fingerprinting
- [x] Add lifecycle-transition enforcement ([`capsule-core/src/lifecycle.rs`](capsule-core/src/lifecycle.rs))
- [x] Add epistemic-envelope validation ([`capsule-core/src/validation.rs`](capsule-core/src/validation.rs))
- [x] Add mandatory evidence validation
- [x] Add confidence-ceiling validation
- [x] Add applicability validation
- [x] Add deterministic contradiction markers
- [x] Build the first `capsule` CLI ([`capsule-cli`](capsule-cli))
- [x] Support `capsule inspect`
- [x] Support `capsule validate`
- [x] Support `capsule pack`
- [x] Support `capsule unpack`

## Phase 2 — Capsule Registry

- [x] Design the PostgreSQL schema ([draft DDL](capsule-registry/postgres-schema.sql))
- [x] Implement capsule metadata persistence
- [x] Implement immutable version storage
- [x] Implement bitemporal validity (`valid_from`/`valid_until` application time + immutable `recorded_at`/`occurred_at` system time)
- [x] Add owners and domain ownership
- [x] Add lifecycle status management
- [x] Add approval records
- [x] Add publication and revocation records
- [x] Add content-addressed deduplication (content hash covers the epistemic payload only, so identical claims dedupe across logical IDs)
- [x] Add capsule dependency records
- [x] Add evidence-family tracking
- [x] Add audit events
- [x] Add registry APIs (Rust crate API; `capsule registry ...` CLI surface — a networked `capsule-api` is still Phase 5)
- [x] Add local SQLite mode ([`capsule-registry`](capsule-registry), the Phase 2 MVP backend)

## Phase 3 — Capsule Compiler

- [x] Build source ingestion contracts ([`capsule-compiler/src/source.rs`](capsule-compiler/src/source.rs))
- [x] Add PDF, text, Markdown, and HTML adapters (PDF via `pdf-extract`, pure Rust — plain-text extraction only, no layout/heading analysis; see [`capsule-compiler/README.md`](capsule-compiler/README.md))
- [x] Add source structure preservation
- [x] Add semantic-boundary detection (heuristic sentence segmentation)
- [x] Add claim extraction
- [x] Pair claims with qualifications and limitations (wired into `evaluation_contract.mandatory_qualifications`)
- [x] Bind claims to exact source spans
- [x] Classify epistemic status (lexical-cue classifier)
- [x] Detect applicability conditions
- [x] Detect exceptions and exclusions
- [x] Detect dependencies (bare `kc://` reference mentions, surfaced for human review rather than auto-wired — the relation isn't inferable from a mention alone)
- [x] Detect source-derived evidence duplication
- [x] Add source-to-capsule transformation lineage
- [x] Add confidence-inflation detection ([`capsule-compiler/src/inflation.rs`](capsule-compiler/src/inflation.rs), the README's confidence-laundering patterns)
- [x] Add deterministic validation before LLM validation (every candidate runs through `capsule_core::validate` immediately; there is no LLM step yet)
- [ ] Add human review workflow (the `capsule-registry` lifecycle gate provides the mechanism; no queueing/UI yet)
- [ ] Add delta compilation for changed source spans

## Phase 4 — Capsule Resolver

- [x] Define the resolution request contract ([`capsule-resolver/src/types.rs`](capsule-resolver/src/types.rs), matches protocol §5)
- [ ] Add purpose resolution (`purpose` is accepted and carried through the bundle/plan for record-keeping, but there is no purpose vocabulary yet to filter against — currently just a label)
- [x] Add identity and permission filtering (consumer role vs. `governance.permissions`)
- [x] Add capsule-type planning
- [x] Add temporal version resolution (via `registry.latest_published(id, as_of)`)
- [x] Add applicability evaluation (jurisdiction here; validity window in the registry's `latest_published`)
- [x] Add authority ordering
- [x] Add evidence-sufficiency gates
- [x] Add dependency completion (`requires`, transitive, cycle-safe, propagates the confidence-ceiling rule from spec §6.3)
- [x] Add contradiction bundles
- [x] Add known-gap reporting
- [x] Add lexical discovery index (substring scan over statements — no separate index infrastructure)
- [ ] Add optional semantic discovery index (would need an embedding/vector index; out of scope for now)
- [x] Add dependency index (`capsule-registry`'s `dependency_records` table + indexes, used via `dependencies_of`/`dependents_of`)
- [x] Add contradiction index (evidence/contradiction lookups against the registry)
- [x] Generate inspectable resolution plans
- [x] Support replayable resolution

## Phase 5 — Capsule Runtime

- [x] Create local embedded runtime ([`capsule-runtime`](capsule-runtime))
- [x] Create remote API runtime ([`capsule-api`](capsule-api), built on `capsule-runtime`)
- [x] Add signed capsule verification (Ed25519 over the content hash; see `capsule-runtime/src/signing.rs`)
- [x] Add policy enforcement (role allowlist in `governance.permissions`; anonymous = unprivileged, never unrestricted)
- [ ] Add purpose-bound access (`purpose` is accepted on `AccessRequest`/`ResolutionRequest` but not yet enforced against any purpose vocabulary — same gap as the resolver's "purpose resolution")
- [x] Add consumer-specific capsule views (evidence redaction without `can_read_evidence`)
- [x] Add local cache
- [x] Add offline mode (trivial by construction: SQLite is local, so there's no server to lose contact with)
- [ ] Add synchronisation with a central registry (no multi-registry replication; only single-registry polling for local cache invalidation)
- [x] Add update subscriptions (pull/poll-based via `Runtime::poll_updates` against the registry's audit log, not push/webhook)
- [x] Add revocation propagation (cache invalidated on lifecycle-transition events found by polling)
- [x] Add usage telemetry (in-process; not yet persisted)
- [x] Add SDKs for Python and TypeScript ([`sdk-python`](sdk-python), [`sdk-typescript`](sdk-typescript) — both dependency-free HTTP clients, both smoke-tested against a real `capsule-api` process)

## Phase 6 — Epistemic Evaluation Engine

- [ ] Define evaluation contracts inside capsules
- [ ] Add answer claim decomposition
- [ ] Map answer claims to supporting capsules
- [ ] Detect unsupported claims
- [ ] Detect applicability expansion
- [ ] Detect inference-to-fact conversion
- [ ] Detect association-to-causation conversion
- [ ] Detect confidence-ceiling violations
- [ ] Detect missing mandatory qualifications
- [ ] Detect contradiction suppression
- [ ] Detect evidence multiplication from shared sources
- [ ] Add deterministic evaluation rules
- [ ] Add bounded LLM-as-a-judge mode
- [ ] Add judge-discretion levels
- [ ] Add evaluation-surface manifests
- [ ] Add Evaluation Surface Reduction Score
- [ ] Add Epistemic Fidelity Score
- [ ] Add Confidence Laundering Risk Score

## Phase 7 — Capsule Studio

- [ ] Design the capsule inspection interface
- [ ] Add source-to-capsule lineage viewer
- [ ] Add evidence viewer
- [ ] Add confidence and uncertainty viewer
- [ ] Add contradiction review interface
- [ ] Add capsule comparison
- [ ] Add approval workflow
- [ ] Add dependency impact viewer
- [ ] Add evaluation report viewer
- [ ] Add capsule health dashboard
- [ ] Add role-based access controls

## Phase 8 — Capsule Observatory

- [ ] Track capsule freshness
- [ ] Track source availability
- [ ] Track unresolved contradictions
- [ ] Track orphaned capsules
- [ ] Track broken dependencies
- [ ] Track confidence-laundering incidents
- [ ] Track evaluation-surface reduction
- [ ] Track capsule reuse ratio
- [ ] Track delta-update efficiency
- [ ] Track knowledge debt
- [ ] Trigger review and expiry alerts

## Phase 9 — Ecosystem and interoperability

- [ ] Publish KCP specification drafts
- [ ] Define `.kcap` compatibility rules
- [ ] Build import and export tools
- [ ] Build a Capsule Exchange
- [ ] Support signed publishers
- [ ] Define trust federation policies
- [ ] Add external registry federation
- [ ] Create public capsule examples
- [ ] Build reusable Capsule Packs
- [ ] Explore a capsule marketplace
- [ ] Create connector SDK
- [ ] Add GitHub, SharePoint, Confluence, object-storage, API, and database connectors

## Phase 10 — Scale and enterprise readiness

- [ ] Benchmark local and remote resolution latency
- [ ] Benchmark capsule compilation cost
- [ ] Benchmark deterministic versus LLM evaluation coverage
- [ ] Add tenant isolation
- [ ] Add enterprise identity integration
- [ ] Add encryption and key management
- [ ] Add regional deployment support
- [ ] Add backup and disaster recovery
- [ ] Add audit export
- [ ] Add policy-as-code support
- [ ] Add Kubernetes deployment manifests
- [ ] Add OpenTelemetry observability
- [ ] Add security threat model
- [ ] Add performance and resilience testing

## First demonstrator

The first end-to-end demonstrator should use policy and compliance knowledge:

1. Upload two versions of a policy.
2. Compile rules, definitions, exceptions, and limitations into capsules.
3. Preserve source language and epistemic status.
4. Detect contradictions and supersession.
5. Review and publish the approved capsule version.
6. Resolve the correct policy by purpose, date, and jurisdiction.
7. Generate an answer with capsule-native citations and confidence limits.
8. Evaluate an overconfident answer against the capsule contract.
9. Publish a policy update.
10. Identify every capsule, answer, agent, evaluation, and decision affected by the change.

## Initial repository direction

```text
CapsuleDB/
├── capsule-core/
├── capsule-compiler/
├── capsule-registry/
├── capsule-resolver/
├── capsule-runtime/
├── capsule-evaluator/
├── capsule-api/
├── capsule-cli/
├── sdk-python/
├── sdk-typescript/
├── capsule-studio/
├── schemas/
├── protocol/
├── examples/
└── docs/
```

## Current status

Phases 0-5 are implemented:

- The Knowledge Capsule specification, KCP v0.1 protocol draft, `.kcap` package format, and a machine-checkable JSON Schema are defined under `docs/specification/`, `protocol/`, and `schemas/`.
- `capsule-core` parses, validates, hashes, and enforces lifecycle transitions for capsule documents.
- `capsule-registry` persists capsules in embedded SQLite: immutable versions, an append-only lifecycle/approval/publication/revocation ledger, content-addressed deduplication, dependency and evidence-family tracking, auto-supersession on publish, and an audit trail. A Postgres schema is drafted for a later backend.
- `capsule-compiler` turns text/Markdown/HTML/PDF sources into draft candidate capsules deterministically: span-bound claim extraction, epistemic-status classification, qualification/exclusion/condition detection, evidence-duplication detection, and immediate `capsule_core::validate` pre-validation — no LLM step yet.
- `capsule-resolver` resolves the applicable capsule set for a request: filtering (domain/type/keyword/jurisdiction/permission/evidence-sufficiency), temporal version resolution, authority ordering, transitive `requires` dependency completion with confidence-ceiling propagation, contradiction bundling, known-gap reporting, and an inspectable, replayable resolution plan.
- `capsule-runtime` is the embeddable local runtime: policy enforcement and consumer-specific views on top of the resolver's permission model, a local cache with poll-based revocation propagation, usage telemetry, and Ed25519 signed-content-hash verification.
- `capsule-api` is a minimal HTTP JSON API built directly on `capsule-runtime`, so policy/views/caching/telemetry apply over the network too. Tested with real HTTP requests, not mocks.
- `sdk-python` and `sdk-typescript` are dependency-free HTTP clients for `capsule-api`, both smoke-tested end to end against a live server.
- `capsule-cli` (`capsule` binary) supports `inspect`, `validate`, `pack`, `unpack`, `compile`, `resolve`, `keygen`, and `registry <register|transition|publish|get|list|history|dependencies|dependents|evidence|audit>`. The whole compile → register → approve → publish → resolve loop works end to end against the canonical sample capsules in `examples/capsules/` and against ad hoc source documents.

Building all of this surfaced and fixed two real bugs from earlier phases: the content hash originally covered identity fields, making the spec's own cross-logical-ID dedup claim impossible (fixed in Phase 2); and the resolver treated an anonymous request as bypassing permission checks entirely rather than as unprivileged (fixed while building Phase 5, alongside correcting the Phase 0 example capsules and spec wording that had conflated permission *roles* with capability *verbs*).

Phase 6 (Epistemic Evaluation Engine) has not started. See the roadmap below for what's next.

---

**CapsuleDB preserves epistemic truth in transit.**
