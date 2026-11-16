# capsule-resolver

Resolves Knowledge Capsules for a purpose into a **resolution bundle**. The bundle contains:

- the resolved set, with each capsule's role and rank
- contradiction bundles
- known gaps
- an inspectable plan saying why every candidate was kept or dropped

Bundles carry digests of the request, the candidate set, and the result, so they can be replayed later to detect drift.

```text
candidates (capsule-registry or capsule files)
  → identity / permission filter        governance.permissions ∩ requester, optional domains
  → required status                     the registry's lifecycle state, not the document's
  → temporal version resolution         valid on `as_of`; newest eligible version; explicit supersession
  → discovery                           lexical index (optional pluggable SemanticIndex)
  → capsule-type planning               from purpose, unless RESOLVE names types
  → applicability                       jurisdiction; exclusions vs context facts; conditions reported
  → evidence-sufficiency gate           any | secondary | primary
  → authority ordering                  planned type, evidence authority, epistemic status, relevance, ceiling
  → dependency completion               requires / refines, transitively; ceiling propagation (spec §6.3)
  → exceptions and contradiction bundles
  → known gaps                          no match, uncovered terms, missing types, unmet deps, evidence-gated
```

## Try it

The CQL `RESOLVE` form from the top-level README runs as written:

```bash
capsule resolve --capsules examples/capsules --cql 'RESOLVE policy_rule
ABOUT "customer data retention"
FOR purpose "privacy review"
IN jurisdiction "India"
AS OF "2026-08-01"
REQUIRE status PUBLISHED
INCLUDE exceptions
INCLUDE contradictions
WITH dependencies'
```

The output has three parts:

- the 2.0.0 (180-day) policy, with its unverified condition reported
- a plan showing that 1.0.0 lost to the newer version and that the marketing capsules were dropped as irrelevant or out of window
- a result digest

To read from a registry database, use `--registry registry.db` instead of `--capsules`. Save `--json` output and run `capsule resolve --replay bundle.json --capsules …` to check whether the same request still resolves the same way. It exits 3 on drift.

Extra CQL clauses: `GIVEN "<fact>"` (applicability context), `REQUIRE evidence primary|secondary|any`, `LIMIT n`. JSON requests follow `request::ResolutionRequest` (KCP §5 plus requester, types, gate, context).

## Limits

- **Lexical discovery.** Discovery is lexical. `SemanticIndex` is a trait for plugging in embeddings or a search service; none ships here.
- **Phrase matching.** Context facts are matched to conditions and exclusions by term coverage, not by a logic engine.
- **No index service.** The dependency and contradiction indexes are built per request from the candidate set. There is no persistent index service yet.
