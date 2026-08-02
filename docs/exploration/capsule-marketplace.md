# Exploring a capsule marketplace

Status: exploration notes (Phase 9 roadmap item — "explore," not build).
No code in this repository implements any of this; it's the analysis a
future decision would start from.

## What would actually be exchanged

Not capsules in isolation — a bare capsule with no provenance a
consumer trusts is just an assertion. The unit that could plausibly be
"sold" or "shared" is closer to a **Capsule Pack** (see
`capsule-cli`'s `pack-create`, Phase 9) from a **signed publisher**
(Phase 5/9's Ed25519 signing): a named, versioned bundle with an
identifiable, verifiable author. A marketplace is really a marketplace
for *publisher reputation applied to packs*, not for individual facts.

## Why this is harder than a package registry

npm/crates.io/PyPI work because correctness is checkable by running the
code. A capsule's correctness is epistemic, not executable — "is this
still true, still applicable, still adequately supported" can't be
unit-tested by the platform. That pushes most of the hard problem onto
governance questions a registry alone doesn't solve:

- **Staleness liability.** Who is responsible when a purchased pack's
  claims go stale (a policy pack citing since-repealed law, a metrics
  pack from a methodology later shown biased)? `capsule-observatory`'s
  freshness/expiry checks (Phase 8) are a buyer-side mitigation, not a
  marketplace-side guarantee.
- **Trust transitivity.** If pack A `requires` capsules from pack B by
  a different publisher (spec §8, `capsule-resolver`'s dependency
  completion), does a consumer trust B because they trust A? KCP has no
  answer for this yet — it's exactly what "trust federation policies"
  (a separate, unchecked Phase 9 item) would need to define before a
  marketplace could compose packs across publishers safely.
- **Verifying a claim vs. verifying a signature.** Signing (implemented)
  proves *who* published a capsule and that its content hasn't been
  tampered with since. It proves nothing about whether the claim is
  *true* or *current*. A marketplace built on signature verification
  alone would give a false sense of epistemic assurance — the two are
  orthogonal and the UI/UX would need to make that distinction
  unmissable.
- **Incentive misalignment with confidence laundering.** The project's
  own core concern (README: "Confidence laundering controls") gets
  worse under commercial incentives — a paid pack has a structural
  pull toward overclaiming certainty to justify its price.
  `capsule-evaluator`'s deterministic detectors (Phase 6) and
  `capsule-observatory`'s within-capsule laundering check (Phase 8)
  would need to run *before* a pack is listed, not just at consumption
  time, or the marketplace becomes the thing it's supposed to guard
  against.

## What a first, honest version could look like

Not a payment platform. A **public index of signed packs** — closer to
crates.io's index than to a storefront: publisher public keys,
pack manifests, dependency graphs, and `capsule-observatory` health
reports, all fetchable and independently re-verifiable. No claim about
correctness beyond "this is what was signed and by whom, and here is
what our deterministic checks found." Payment/licensing, if ever
warranted, is a layer on top of that index, not a prerequisite for it.

## Open questions this exploration surfaced, not answered

- Does `capsule-resolver`'s authority ordering (spec: primary >
  secondary > derived > unverified) need a marketplace-specific tier
  for "third-party signed but institutionally unaffiliated," or does
  that collapse into existing evidence authority?
- Should a pack's price (if any) correlate with anything the platform
  can measure (`epistemic_fidelity`, freshness, dependency depth), or
  is that an invitation to game the metric?
- Is federation (Phase 9's "external registry federation") a
  prerequisite for a marketplace, or can a single centrally-hosted
  index come first and federate later?

None of these block the rest of the roadmap; they're recorded here so
the next person picking up Phase 9 doesn't have to re-derive them.
