# capsule-evaluator

Evaluates an answer against Knowledge Capsule contracts. Deterministic checks run first. A bounded judge is used only for residual questions the checks cannot settle, and only when the evaluation-surface manifest allows it.

```text
Answer
  → claim decomposition           sentences and `;` clauses, with byte spans
  → capsule mapping               lexical overlap + numeric agreement, per claim
  → deterministic validation      unsupported claims, numeric mismatch, superseded knowledge, manifest rules
  → epistemic validation          applicability expansion, inference→fact, association→causation,
                                  confidence-ceiling violations, missing mandatory qualifications,
                                  contradiction suppression, evidence multiplication, prohibited overclaims
  → bounded judge                 residual questions only; fixed labels; never scores the answer
  → report                        claims, findings (with the triggering cue), decisions, scores, verdict
```

## Try it

```bash
capsule evaluate --answer capsule-evaluator/examples/answers/overconfident.txt \
  --capsules examples/capsules/marketing-incrementality-estimate.json \
             examples/capsules/marketing-incrementality-counterexample.json \
  --surface capsule-evaluator/fixtures/marketing.surface.yaml
# verdict: Fail ... InferenceToFact, ConfidenceCeilingViolation, ProhibitedOverclaim,
#   MissingQualification ×2, ContradictionSuppression, EvidenceMultiplication

capsule evaluate --answer capsule-evaluator/examples/answers/faithful.txt --capsules examples/capsules
# verdict: Pass, surface reduction 1.00, fidelity 1.00, laundering risk 0.00
```

Add `--json` for the full report. Add `--judge anthropic` to refer residual questions to Claude. That needs `ANTHROPIC_API_KEY` or `ANTHROPIC_AUTH_TOKEN`, and a manifest that grants the judge discretion. The exit code is 2 when the verdict is `fail`.

## Detectors

| Detector | Fires when | Example cue |
|---|---|---|
| Unsupported claim | No capsule maps to the claim, or a judge says it is unsupported | — |
| Numeric mismatch | The claim maps to a capsule but its figures do not appear in it | `15%` vs `6-9%` |
| Superseded knowledge | The supporting capsule is superseded (explicitly, or by a newer published version) or revoked | 90-day policy after the 180-day one |
| Applicability expansion | A period outside the validity window, a jurisdiction not covered, an excluded case, or generalization language on a scoped capsule | `Q3 2026`, `United States`, `across all channels` |
| Inference → fact | A hedged capsule (estimated, inferred, modelled, …) stated as fact, an inflation pattern, or a dropped hedge | `definitely`, `estimated → observed` |
| Association → causation | Causal language over associative, non-experimental evidence | `causes` over `associated with` |
| Confidence ceiling | Expressed confidence above the capsule's ceiling, after `requires` propagation | `95% confident` vs ceiling 0.6 |
| Missing qualification | A mandatory qualification is not conveyed (partial matches are residual) | `unless a legal hold applies` |
| Contradiction suppression | A capsule with an unresolved contradiction is used, and the contradicting capsule is not surfaced | counterexample not mentioned |
| Evidence multiplication | The answer claims multiple or independent evidence, but the sources do not support that | `multiple independent studies`, one source |
| Prohibited overclaim | The claim restates one of the capsule's prohibited overclaims (negation-aware) | `more email reduces churn` |
| Rules | Manifest `forbid` / `require_any` phrases, optionally scoped to a capsule | — |

## Evaluation-surface manifest

A reviewed YAML file that declares which checks run, their thresholds, extra rules, and the judge policy. See [`fixtures/marketing.surface.yaml`](fixtures/marketing.surface.yaml).

- **Discretion levels:**
  - `none`: residual questions stay unresolved.
  - `classify`: one label from a fixed set.
  - `classify_with_rationale`: as `classify`, plus a recorded rationale.
- **Residual kinds:** `claim_support` (ambiguous mapping) and `qualification_conveyed` (partial match).
- **Cap:** `max_calls` limits judge calls per evaluation.
- **Off-label answers:** anything outside the allowed labels is discarded.

## Scores

- **Evaluation Surface Reduction:** the share of decisions settled deterministically (1.0 means the judge decided nothing).
- **Epistemic Fidelity:** per-claim penalties for fidelity findings, averaged, then scaled by answer-level findings.
- **Confidence Laundering Risk:** 1 − ∏(1 − wᵢ) over laundering-type findings (causation 0.6, ceiling 0.6, inference→fact 0.5, multiplication 0.5, overclaim 0.5, applicability 0.35, contradiction 0.35, missing qualification 0.25).

## The judge

`anthropic::AnthropicJudge` calls the Messages API over HTTP; there is no official Rust SDK. Its defaults are as follows:

- **Model:** `claude-opus-5-5`. Override it with `CAPSULE_JUDGE_MODEL`.
- **Effort:** `low`.
- **Output:** a JSON-schema output constrained to the allowed labels.
- **Refusals:** the server-side refusal fallback is enabled, and a refusal that survives it leaves the question unresolved.

Inputs are framed as data, not instructions. Each request is one claim against one capsule statement.

## Limits

- **Lexical cues.** Claim mapping and the cues are lexical and deterministic, not semantic. They are explainable and reproducible, but paraphrases with no shared vocabulary map as unrelated. That is why mapping has a residual band for the judge.
- **Manual pack.** The pack is built from files or directories. Resolving the pack with `capsule-resolver` before evaluating is a manual step for now (`capsule resolve --json`, then pass the resolved capsules).
