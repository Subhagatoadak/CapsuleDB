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
| `legal-hold-procedure.json` | A `procedure` capsule (ordered steps), `refines` dependency on the retention rule. |
| `legal-hold-interpretation.json` | An `interpretation` capsule: `expert_judgement` status, a lower confidence ceiling than the rule it interprets since it's a reading of ambiguous text, not the text itself. |
| `research-dataset-boundary.json` | A `boundary` capsule: an explicit applicability carve-out from the retention rule, with its own conditions/exclusions. |
| `retention-answer-rubric.json` | A `rubric` capsule: an evaluation standard (`evaluation_contract.evaluation_rules`) an answer about retention must satisfy, `requires` the current retention rule. |

Together these eight fixtures cover all eight `capsule_type` values from
spec §3 (`fact`, `rule`, `metric`, `counterexample`, `procedure`,
`interpretation`, `boundary`, `rubric`).
