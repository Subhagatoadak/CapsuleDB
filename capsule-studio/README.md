# capsule-studio

A single self-contained `index.html` — no build step, no npm install, no
framework — that drives `capsule-api` from a browser (Phase 7).

```bash
cargo run -p capsule-api -- --db capsules.sqlite3 --addr 127.0.0.1:8089
open capsule-studio/index.html   # or just double-click it
```

Point the address bar at the top of the page to wherever `capsule-api`
is running (defaults to `http://127.0.0.1:8089`). `capsule-api` sends
`Access-Control-Allow-Origin: *`, so opening `index.html` directly as a
`file://` URL works with no proxy or dev server.

## What's here

- **Inspect** — fetch a capsule by logical ID + version; renders
  identity, lifecycle state, epistemic status, confidence (with a
  ceiling bar), evidence, applicability, contradictions, dependencies,
  the evaluation contract, and governance. Also lists versions, the
  lifecycle history ledger, and dependencies/dependents.
- **Register & Approve** — paste a draft capsule's JSON to register it,
  then drive it through `in_review -> approved -> published` (or
  `deprecated`/`revoked`) from the Inspect tab's transition panel —
  the approval workflow.
- **Resolve** — run a resolution request (purpose/domain/about/
  jurisdiction/consumer role) and see the resolved capsules, the
  contradiction bundle, known gaps, and the full inclusion/exclusion
  plan.
- **Evaluate** — paste an answer and see `capsule-evaluator`'s findings,
  evaluation-surface manifest, and the three scores — the evaluation
  report viewer.
- **Audit** — the registry's recent audit events.

## What's not here

Capsule comparison, a dedicated dependency-impact graph view, a capsule
health dashboard (that's `capsule-observatory`, Phase 8), and
role-based access controls on the Studio UI itself (the underlying API
does enforce capsule-level permissions per request — see
`capsule-runtime/README.md` — but Studio has no login of its own; you
type a consumer role into a field and it's trusted).

## Testing

No test framework is wired in for a single static file, but the whole
flow (register → transition → publish → get → resolve → evaluate →
audit) has been driven end-to-end in a real headless-Chromium session
against a live `capsule-api` process. That session caught a real bug —
`GET /capsule` was serving a stale cached copy after a transition,
because the mutating endpoints wrote straight to the registry without
invalidating `capsule-runtime`'s cache — now fixed in `capsule-api` (see
its `poll_updates()` calls after register/transition/publish) and
covered by a Rust regression test in `capsule-api/tests/api.rs`.
