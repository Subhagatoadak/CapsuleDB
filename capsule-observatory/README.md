# capsule-observatory

Registry-wide health checks (Phase 8) — the things nobody notices until
a stale, contradicted, or overclaiming capsule has already been
resolved into an answer.

```rust
let registry = capsule_registry::Registry::open("capsules.sqlite3")?;
let report = capsule_observatory::run(&registry, &capsule_observatory::ObservatoryOptions::default())?;
```

## Checks

- **Freshness alerts** — a `published` capsule not touched in
  `freshness_threshold_days` (default 180).
- **Unresolved contradictions** — every capsule's declared
  contradictions with `resolution_status: unresolved`.
- **Orphaned capsules** — a `published` capsule nothing else declares a
  dependency on. Checked against *every* published version, not just
  the temporally-current one (`Registry::latest_published` also filters
  by today's applicability window, which would silently skip an
  expired-but-still-published capsule here — exactly the kind of
  capsule this check should flag, not hide). Informational, not
  inherently a problem: most leaf-level rules legitimately have zero
  dependents.
- **Broken dependencies** — a `requires`/`refines`/`supersedes` target
  that doesn't resolve to anything registered.
- **Confidence-laundering incidents** — reuses `capsule-compiler`'s
  inflation detector *within* a single capsule: its own statement
  compared against its own cited evidence excerpt. (Cross-capsule/
  answer-vs-source laundering is `capsule-evaluator`'s job.)
- **Knowledge debt** — a `draft`/`in_review` capsule that's sat
  unpublished for `knowledge_debt_threshold_days` (default 30).
- **Expiry alerts** — a `published` capsule whose `applicability.valid_until`
  has passed but which hasn't been deprecated/superseded/revoked.
- **Capsule reuse ratio** — `1 - (distinct payloads / total versions)`,
  making the content-addressed dedup from `capsule-registry` visible as
  a single number.

Not implemented: source availability tracking (no source carries a
URL/liveness contract to check), delta-update efficiency (there's no
delta compilation to measure yet), and evaluation-surface-reduction
trend tracking (`capsule-evaluator` doesn't persist past reports for
this to aggregate over).

Exposed via the CLI as `capsule observatory --db <registry> [--freshness-days N] [--knowledge-debt-days N]`.
