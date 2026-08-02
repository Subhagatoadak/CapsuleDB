# Knowledge Capsule Specification v0.1

Status: Draft (Phase 0)

This document defines the minimum specification for a **Knowledge Capsule**,
the core object of CapsuleDB. It is the normative reference implemented by
`capsule-core` in Phase 1.

## 1. Schema versioning

Every capsule document declares the protocol schema it conforms to:

```json
"schema_version": "kcp/0.1"
```

`capsule-core` rejects documents whose `schema_version` it does not
recognize. Schema versions are additive-only within a major line; breaking
changes increment the major segment (`kcp/1.0`).

## 2. Capsule identity

A capsule has two independent identities:

### 2.1 Logical identity

A stable, human-legible, namespaced identity that does **not** change across
versions of the same knowledge unit:

```text
kc://<domain>/<topic>/<name>
```

- `domain`, `topic`, `name` are each lowercase, `[a-z0-9]` with internal
  single hyphens (`^[a-z0-9]+(-[a-z0-9]+)*$`).
- Example: `kc://policy/data-retention/customer-data-90-days`

### 2.2 Version

A capsule carries a [SemVer 2.0](https://semver.org) `version`
(`MAJOR.MINOR.PATCH`). Within one logical ID:

- `PATCH` — wording/formatting clarification, no claim change.
- `MINOR` — added evidence, narrowed/widened applicability, non-breaking
  refinement.
- `MAJOR` — the claim itself changed. A major bump requires supersession
  (see §7).

### 2.3 Content-addressed identity

A capsule also has an immutable `content_hash`:

```text
sha256:<hex>
```

Computed over the canonical JSON serialization of the capsule's **epistemic
payload**: the full document with the identity and administrative fields
`content_hash`, `logical_id`, `version`, `lifecycle_state`, and `governance`
removed first (see `capsule-core::hashing` for the canonicalization
algorithm: UTF-8, keys sorted lexicographically, no insignificant
whitespace). Everything that describes *what is known* — `statement`,
`epistemic_status`, `confidence`, `evidence`, `applicability`,
`contradictions`, `dependencies`, `evaluation_contract` — participates in
the hash; everything that describes *who filed it, under what name, at
what stage* does not.

This means two capsules with identical epistemic content hash identically
even under different logical IDs, owners, or lifecycle states. That
equality is the deduplication key: `capsule-registry` stores a payload
once per distinct `content_hash` and lets any number of `capsule_versions`
rows reference it, which is also how it surfaces evidence or claims reused
verbatim across unrelated capsules.

## 3. Capsule types

```text
fact            observed or established state of the world
rule            a normative or policy constraint
procedure       an ordered set of steps to follow
metric          a defined measurement and its computation
interpretation  a reading or explanation of an underlying fact/rule
boundary        an applicability limit or exclusion condition
counterexample  a documented exception that challenges a claim
rubric          a standard used to evaluate answers or decisions
```

## 4. Lifecycle states

```text
draft → in_review → approved → published → {deprecated, superseded, revoked}
```

Rules enforced by `capsule-core::lifecycle`:

- Forward-only through `draft → in_review → approved → published`. No state
  may be skipped except an explicit `draft → revoked` (abandoning work).
- `published` is a terminal *entry* state — it may only transition to
  `deprecated`, `superseded`, or `revoked`. It can never return to `draft`,
  `in_review`, or `approved`.
- `deprecated`, `superseded`, and `revoked` are terminal. A new fact requires
  a new version, not a lifecycle reversal.
- Deleting a `published` capsule is not a valid operation. Use `SUPERSEDE`
  or `REVOKE` (see README "Native operations").

## 5. Epistemic status vocabulary

```text
observed          directly measured or witnessed
inferred          derived via reasoning from other claims
estimated         approximated, with stated method
modelled          produced by a model/simulation
policy_defined    true by definition/decree, not empirical
expert_judgement  asserted by a qualified authority without formal evidence
disputed          credible contradicting evidence exists
unknown           status could not be determined
```

A capsule with `epistemic_status: disputed` MUST have a non-empty
`contradictions` array (§7).

## 6. Evidence and confidence

### 6.1 Evidence entries

Each item in `evidence[]`:

```json
{
  "source_id": "string",
  "span": "string (page/section/offset locator)",
  "excerpt": "string (verbatim quotation)",
  "authority": "primary | secondary | derived | unverified",
  "retrieved_at": "RFC 3339 timestamp"
}
```

Authority vocabulary:

```text
primary     the originating/authoritative source itself
secondary   a reliable account of a primary source
derived     computed/aggregated from other capsules
unverified  provenance not established
```

### 6.2 Mandatory evidence rule

A capsule of type `fact`, `rule`, or `metric` with `epistemic_status` in
{`observed`, `expert_judgement`} MUST carry at least one `evidence` entry
whose `authority` is not `unverified`.

### 6.3 Confidence basis and ceiling

```json
"confidence": {
  "basis": "direct_observation | statistical_inference | expert_consensus | model_prediction | policy_mandate | heuristic | unsupported",
  "ceiling": 0.0,
  "rationale": "string"
}
```

`ceiling` is a float in `[0.0, 1.0]` — the maximum confidence any downstream
consumer may assign to this capsule's claim, regardless of how it is
phrased. It is not a point estimate of correctness; it is a cap.

Deterministic rule (enforced by `capsule-core::validation`):

- `basis == "unsupported"` ⇒ `ceiling <= 0.3`.
- A capsule's `ceiling` may never exceed the lowest `ceiling` among the
  capsules it depends on with relation `requires` (see §8) — "the confidence
  of a conclusion cannot exceed the confidence ceiling of its critical
  unsupported dependency."

## 7. Applicability, exclusions, and contradictions

```json
"applicability": {
  "jurisdictions": ["IN", "EU"],
  "valid_from": "2026-01-01",
  "valid_until": null,
  "conditions": ["string"],
  "exclusions": ["string"]
}
```

`valid_from <= valid_until` when both are present.

```json
"contradictions": [
  { "capsule_id": "kc://.../...", "description": "string", "resolution_status": "unresolved | resolved | acknowledged" }
]
```

## 8. Dependencies and supersession

```json
"dependencies": [
  { "capsule_id": "kc://.../...", "relation": "requires | refines | supersedes" }
]
```

- `supersedes` — this capsule replaces the referenced one. The referenced
  capsule's lifecycle must move to `superseded` as part of publishing this
  one.
- `requires` — this capsule's confidence ceiling is bounded by the
  dependency's ceiling (§6.3).
- `refines` — a non-binding refinement relationship used for lineage.

## 9. Governance and evaluation contract

```json
"governance": { "owner": "string", "domain": "string", "permissions": ["read", "write", "approve"] },
"evaluation_contract": {
  "mandatory_qualifications": ["string"],
  "prohibited_overclaims": ["string"],
  "evaluation_rules": ["string"]
}
```

## 10. Minimum required fields

A capsule document is structurally valid only if it contains:

`schema_version`, `logical_id`, `version`, `capsule_type`, `statement`,
`epistemic_status`, `confidence`, `lifecycle_state`, `governance`.

`evidence`, `applicability`, `contradictions`, `dependencies`,
`evaluation_contract` are optional but validated when present, and required
under the conditions in §6.2 and §5.

See `schemas/capsule.schema.json` for the machine-checkable JSON Schema and
`examples/capsules/` for canonical samples.
