# capsule-compiler

Deterministic source-to-candidate-capsule pipeline (Phase 3 MVP — no LLM
step; that's the point, see below).

```text
bytes -> Source (structure-preserved text + blocks)
      -> claims (sentence-level, span-bound)
      -> classify (epistemic status, qualifications, exclusions, conditions)
      -> CandidateCapsule (draft Capsule + lineage + deterministic pre-validation)
```

```rust
let source = capsule_compiler::ingest_markdown(&bytes)?;
let options = capsule_compiler::CompileOptions { domain: "policy".into(), topic: "retention".into(), owner: "alice".into() };
let report = capsule_compiler::compile(&source, &options);
for candidate in &report.candidates {
    // candidate.capsule is a real, draft-lifecycle capsule_core::Capsule
    // candidate.validation_issues is capsule_core::validate() already run against it
}
```

## What's implemented

- **Adapters**: `text`, `markdown` (native `#`..`######` heading
  recognition), `html` (tag stripping, `<h1>`-`<h6>` promoted to `#`
  headings, entity decoding), and `pdf` (via `pdf-extract`, pure Rust, no
  system PDF library). PDF extraction is plain-text, not layout analysis:
  it emits a line per PDF text-showing operation, which does not always
  line up with real paragraph breaks, and it detects no headings at all
  (nothing in extracted PDF text reliably marks one). Encrypted,
  scanned/image-only, or malformed PDFs fail with
  `CompilerError::PdfExtraction` rather than returning empty or garbled
  text.
- **Structure preservation & semantic-boundary detection**: paragraphs and
  headings become `StructuralBlock`s; sentences within non-heading blocks
  become candidate claims, each with an exact byte-span into the source's
  normalized text (`segmentation::extract_claims`).
- **Lexical classification** (`classify.rs`): epistemic status
  (policy/observed/expert-judgement/modelled/disputed/estimated/unknown),
  qualifications ("unless...", "provided that..." → wired into the
  capsule's `evaluation_contract.mandatory_qualifications`), exclusions
  ("except...", "excluding..." → wired into `applicability.exclusions`),
  applicability conditions ("if...", "when..." → `applicability.conditions`),
  and bare `kc://` references (surfaced as `referenced_capsules`, **not**
  auto-wired into `dependencies[]` — a text mention doesn't tell us the
  relation, so a human classifies it).
- **Evidence-duplication detection**: exact-duplicate sentences (by
  content hash) within one source are flagged once and not turned into a
  second candidate.
- **Deterministic validation before LLM validation**: every candidate is
  run through `capsule_core::validate` immediately. Candidates are
  intentionally conservative — `evidence[].authority` is always
  `unverified` and `confidence.basis` is always `heuristic` with a capped
  `ceiling` — so a claim the classifier calls `observed` or
  `expert_judgement` will *fail* the mandatory-evidence rule until a human
  upgrades the evidence. That disagreement between compiler output and
  core validator is deliberate, not a bug.
- **Confidence-inflation detection** (`inflation.rs`): the README's
  may→does / suggests→proves / associated-with→causes /
  estimated→observed / moderate-confidence→certain /
  limited-evidence→established-evidence patterns, as a reusable two-string
  comparator (`detect_inflation(original, revised)`). The MVP pipeline
  extracts claims verbatim, so it never fires on its own output; it's here
  for human review workflows and for `capsule-evaluator` (Phase 6) to run
  when a statement is edited relative to its source.

## Deliberately out of scope for this MVP

- A human review *workflow* (queueing, UI) — the mechanism is
  `capsule-registry`'s `draft -> in_review -> approved -> published`
  lifecycle gate; this crate just always emits `draft`.
- Delta compilation for changed source spans.

Exposed via the CLI as `capsule compile <source> --domain D --topic T --owner O [--format text|markdown|html|pdf] [--out-dir DIR]` (see `capsule-cli`).
