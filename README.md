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

- [ ] Create the Rust workspace
- [ ] Implement capsule core types
- [ ] Add JSON Schema support
- [ ] Add capsule parsing and serialisation
- [ ] Add schema-version validation
- [ ] Add logical ID validation
- [ ] Add semantic version handling
- [ ] Add content hashing
- [ ] Add source fingerprinting
- [ ] Add lifecycle-transition enforcement
- [ ] Add epistemic-envelope validation
- [ ] Add mandatory evidence validation
- [ ] Add confidence-ceiling validation
- [ ] Add applicability validation
- [ ] Add deterministic contradiction markers
- [ ] Build the first `capsule` CLI
- [ ] Support `capsule inspect`
- [ ] Support `capsule validate`
- [ ] Support `capsule pack`
- [ ] Support `capsule unpack`

## Phase 2 — Capsule Registry

- [ ] Design the PostgreSQL schema
- [ ] Implement capsule metadata persistence
- [ ] Implement immutable version storage
- [ ] Implement bitemporal validity
- [ ] Add owners and domain ownership
- [ ] Add lifecycle status management
- [ ] Add approval records
- [ ] Add publication and revocation records
- [ ] Add content-addressed deduplication
- [ ] Add capsule dependency records
- [ ] Add evidence-family tracking
- [ ] Add audit events
- [ ] Add registry APIs
- [ ] Add local SQLite mode

## Phase 3 — Capsule Compiler

- [ ] Build source ingestion contracts
- [ ] Add PDF, text, Markdown, and HTML adapters
- [ ] Add source structure preservation
- [ ] Add semantic-boundary detection
- [ ] Add claim extraction
- [ ] Pair claims with qualifications and limitations
- [ ] Bind claims to exact source spans
- [ ] Classify epistemic status
- [ ] Detect applicability conditions
- [ ] Detect exceptions and exclusions
- [ ] Detect dependencies
- [ ] Detect source-derived evidence duplication
- [ ] Add source-to-capsule transformation lineage
- [ ] Add confidence-inflation detection
- [ ] Add deterministic validation before LLM validation
- [ ] Add human review workflow
- [ ] Add delta compilation for changed source spans

## Phase 4 — Capsule Resolver

- [ ] Define the resolution request contract
- [ ] Add purpose resolution
- [ ] Add identity and permission filtering
- [ ] Add capsule-type planning
- [ ] Add temporal version resolution
- [ ] Add applicability evaluation
- [ ] Add authority ordering
- [ ] Add evidence-sufficiency gates
- [ ] Add dependency completion
- [ ] Add contradiction bundles
- [ ] Add known-gap reporting
- [ ] Add lexical discovery index
- [ ] Add optional semantic discovery index
- [ ] Add dependency index
- [ ] Add contradiction index
- [ ] Generate inspectable resolution plans
- [ ] Support replayable resolution

## Phase 5 — Capsule Runtime

- [ ] Create local embedded runtime
- [ ] Create remote API runtime
- [ ] Add signed capsule verification
- [ ] Add policy enforcement
- [ ] Add purpose-bound access
- [ ] Add consumer-specific capsule views
- [ ] Add local cache
- [ ] Add offline mode
- [ ] Add synchronisation with a central registry
- [ ] Add update subscriptions
- [ ] Add revocation propagation
- [ ] Add usage telemetry
- [ ] Add SDKs for Python and TypeScript

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

CapsuleDB is at the concept and architecture stage. The first development milestone is the Knowledge Capsule specification and a small Rust-based capsule core that can parse, validate, hash, inspect, and package capsules.

---

**CapsuleDB preserves epistemic truth in transit.**
