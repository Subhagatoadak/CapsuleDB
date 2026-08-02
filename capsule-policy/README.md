# capsule-policy

Policy-as-code (Phase 10): deployment-specific rules layered on top of
`capsule_core::validate`'s protocol-level checks.

```rust
let engine = capsule_policy::PolicyEngine::load("policy.json")?;
let violations = engine.evaluate(&capsule)?;
if !violations.is_empty() {
    // refuse to register/publish
}
```

`capsule_core::validate` enforces what KCP 0.1 itself requires — every
capsule, in every deployment (a `fact` needs evidence, every basis has a
hard confidence-ceiling cap, and so on). `PolicyEngine` enforces what
*this* deployment additionally requires. A capsule can be protocol-valid
and still violate policy.

Policy is data, not code — a JSON file of rules, not a fork of this
crate:

```json
{
  "rules": [
    { "type": "allowed_domains", "domains": ["policy", "legal"] },
    { "type": "max_confidence_ceiling", "basis": "vendor_estimate", "max": 0.6 },
    { "type": "owner_pattern", "pattern": "^.+@company\\.example$" },
    { "type": "min_evidence_for_type", "capsule_type": "fact", "min_evidence": 2 },
    { "type": "forbidden_statement_phrase", "phrase": "guaranteed" }
  ]
}
```

- `allowed_domains` — `governance.domain` must be in the list.
- `max_confidence_ceiling` — for capsules with the matching
  `confidence.basis`, `confidence.ceiling` must not exceed `max`. This is
  a *tightening*: the protocol already caps each basis (spec §5); this
  lets a deployment cap it further, not loosen it.
- `owner_pattern` — `governance.owner` must match a regex (e.g. requiring
  an internal email domain).
- `min_evidence_for_type` — a capsule of `capsule_type` must cite at
  least this many evidence entries, stricter than the protocol's blanket
  "at least one" for `fact`/`observed`.
- `forbidden_statement_phrase` — `statement` must not contain a phrase
  (case-insensitive), for banning specific overclaiming language beyond
  `capsule-compiler::detect_inflation`'s built-in pattern list.

Exposed via the CLI as `capsule registry register --policy policy.json`
(see `capsule-cli/README.md`) — a violation refuses the registration
entirely, with every violated rule printed to stderr.

Adding a new rule type means adding a `PolicyRule` variant and its match
arm in `PolicyEngine::evaluate`; there is no plugin mechanism, since a
rule that needs one isn't expressible as JSON data in the first place and
belongs in `capsule-evaluator` or a purpose-built check instead.
