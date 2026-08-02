# capsule-resolver

Resolves the smallest applicable, authoritative capsule set for a request
(Phase 4) instead of merely returning semantically similar text, per the
resolution request contract in `protocol/KCP-v0.1.md` §5.

```rust
let registry = capsule_registry::Registry::open("capsules.sqlite3")?;
let resolver = capsule_resolver::Resolver::new(&registry);

let mut request = capsule_resolver::ResolutionRequest::new("privacy review");
request.domain = Some("policy".to_string());
request.about = Some("retention".to_string());
request.jurisdiction = Some("IN".to_string());

let bundle = resolver.resolve(&request)?;
```

## Pipeline

1. **Lexical discovery** — scans every registered logical ID (no separate
   search index in this MVP: no optional semantic/embedding index either,
   deliberately left for later).
2. **Filtering** — domain prefix, capsule type, `about` keyword (substring
   match against the statement), jurisdiction (`applicability.jurisdictions`
   empty = global), consumer role vs. `governance.permissions` (identity
   and permission filtering), and an evidence-sufficiency gate that can
   exclude candidates whose evidence is entirely `unverified`.
3. **Temporal version resolution** — `registry.latest_published(id, as_of)`
   per candidate.
4. **Authority ordering** — survivors sorted by best evidence authority
   (`primary` > `secondary` > `derived` > `unverified`).
5. **Dependency completion** — `requires` dependencies are pulled in
   transitively (cycle-safe), and each capsule's
   `effective_confidence_ceiling` is capped by the lowest ceiling among
   its `requires` chain — the rule from
   `docs/specification/knowledge-capsule-spec.md` §6.3: "the confidence
   of a conclusion cannot exceed the confidence ceiling of its critical
   unsupported dependency." A missing `requires` target becomes a
   `KnownGap`, not a silent omission.
6. **Contradiction bundles** — every resolved capsule's declared
   contradictions are surfaced, with the contradicting capsule pulled
   into the bundle when it resolves to something registered.
7. **Inspectable, replayable plan** — every logical ID considered gets a
   `PlanStep` recording why it was included or excluded.
   `ResolutionBundle` (request + result) is `Serialize`/`Deserialize`, and
   `Resolver::replay(&bundle)` re-runs `bundle.request` against the
   registry's current state.

Exposed via the CLI as `capsule resolve "<purpose>" [--domain] [--about] [--jurisdiction] [--as-of YYYY-MM-DD] [--consumer-role] [--require-sufficient-evidence] [--plan]`.
