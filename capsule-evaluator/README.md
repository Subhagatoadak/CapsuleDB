# capsule-evaluator

Evaluates an answer against the capsules that back it (Phase 6), per the
top-level README's target architecture:

```text
Answer -> Claim decomposition -> Capsule resolution -> Deterministic validation
       -> Epistemic validation -> Bounded LLM judge -> Evaluation report
```

```rust
let registry = capsule_registry::Registry::open("capsules.sqlite3")?;
let report = capsule_evaluator::evaluate(
    "Data retention is observed at ninety days.",
    &registry,
    &capsule_evaluator::NullJudge,
    &capsule_evaluator::EvaluateOptions::default(),
)?;
```

## Pipeline

1. **Claim decomposition** — reuses `capsule-compiler`'s sentence
   segmentation rather than reimplementing it: an answer's claims are
   extracted the same way a source document's are.
2. **Claim-to-capsule mapping** — an explicit `kc://` citation always
   counts as strong support; otherwise Jaccard word overlap against every
   published capsule's statement, split into a *strong* band (confident)
   and a *weak* band (ambiguous) — the gap between them is exactly the
   "residual semantic question" a judge exists for.
3. **Deterministic detectors** (`detectors.rs`) — all run before any
   judge:
   - unsupported claims (no capsule, even weakly, supports it)
   - applicability expansion (universal language — "always", "for
     everyone" — against a capsule with stated applicability limits)
   - inference-to-fact / association-to-causation (reuses
     `capsule_compiler::detect_inflation`, comparing the capsule's
     statement against the claim's phrasing)
   - confidence-ceiling violations (absolute-certainty language against
     a low-ceiling capsule)
   - missing mandatory qualifications (checked against the **whole
     answer**, not just the citing sentence — a qualification stated
     nearby still counts as disclosed)
   - contradiction suppression (an unresolved contradiction the answer
     never discloses)
   - evidence multiplication (two or more cited capsules sharing an
     evidence `source_id` — not independent evidence)
4. **Bounded judge** (`judge.rs`) — only for claims left ambiguous by
   step 2; [`DiscretionLevel`] controls how wide that ambiguous band is.
   **No LLM is wired in** — `NullJudge` always abstains. The boundary
   (deterministic-first, judge-only-for-genuine-ambiguity) is the
   deliverable here, not a judge implementation; wiring one in means
   implementing the `Judge` trait, nothing else changes.
5. **Evaluation report** — findings, judge verdicts, an evaluation-surface
   manifest (how many claims were resolved deterministically vs.
   referred vs. unsupported), and three scores:
   - `evaluation_surface_reduction` — share of claims resolved without a
     judge.
   - `epistemic_fidelity` — share of strongly-supported claims with zero
     findings against them.
   - `confidence_laundering_risk` — inflation/ceiling findings per claim.

Exposed via the CLI as `capsule evaluate <answer-file> --db <registry> [--discretion none|low|medium|high]`.
