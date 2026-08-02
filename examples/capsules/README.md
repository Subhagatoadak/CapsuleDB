# Canonical sample capsules

These fixtures exercise the rules in
`docs/specification/knowledge-capsule-spec.md` and are used as test fixtures
by `capsule-core`.

| File | Demonstrates |
| --- | --- |
| `data-retention-90-days.json` | A published `rule` capsule, `policy_mandate` confidence basis, mandatory-qualification evaluation contract. |
| `data-retention-180-days.json` | Same logical ID, next major version, `supersedes` dependency on the 90-day capsule — the supersession pattern from spec §8. |
| `marketing-incrementality-estimate.json` | An `estimated` `metric` capsule with a capped confidence ceiling (0.6) pending replication — the running example from the README's evaluation section. |
| `marketing-incrementality-counterexample.json` | A `counterexample` capsule with `epistemic_status: disputed`, `confidence.basis: unsupported` (ceiling capped at 0.3), and an unresolved `contradictions` entry pointing back at the estimate above. |
