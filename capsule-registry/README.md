# capsule-registry

Capsule identity, immutable versions, lifecycle, ownership, dependency and
evidence-family tracking, and an audit trail (Phase 2, targets **L2
registry conformance** per `protocol/KCP-v0.1.md`).

The MVP backend is embedded SQLite (`rusqlite`, bundled — no server or
system library needed). `postgres-schema.sql` drafts the equivalent
Postgres shape (same tables, same derivation rules) for a later backend;
`src/schema.sql` is the SQLite DDL actually in use.

```rust
let mut registry = capsule_registry::Registry::open("capsules.sqlite3")?;
let record = registry.register(&capsule, "alice")?;       // must be lifecycle_state: draft
registry.transition(&record.logical_id, &record.version, LifecycleState::InReview, "alice", None)?;
registry.transition(&record.logical_id, &record.version, LifecycleState::Approved, "bob", None)?;
let outcomes = registry.publish(&record.logical_id, &record.version, "bob")?; // auto-supersedes `supersedes` deps
```

Design points:

- **Immutable versions.** `capsule_versions` rows are never updated, only
  inserted. Re-registering the same `(logical_id, version)` with identical
  content is a no-op; with different content it's a `Conflict` error.
- **Derived lifecycle state.** Current state is the latest row in the
  append-only `lifecycle_events` ledger, not a column that gets
  overwritten — that ledger *is* the approval/publication/revocation
  record the roadmap calls for.
- **Content-addressed dedup.** The content hash covers only the epistemic
  payload (see the `content_hash` fix in
  `docs/specification/knowledge-capsule-spec.md` §2.3): two capsules with
  identical claims under different logical IDs, owners, or lifecycle
  states share one `capsule_payloads` row.
- **Auto-supersession on publish.** Publishing a capsule that declares a
  `supersedes` dependency (spec §8) attempts to move the referenced
  capsule `published -> superseded` automatically, reporting what
  happened (or why it didn't) via `SupersessionOutcome`.
- **Bitemporal-lite.** `valid_from`/`valid_until` (application time, from
  `applicability`) are indexed columns; `recorded_at`/`occurred_at`
  (system time) are set once at insert and never changed.
  `latest_published(logical_id, as_of)` is the registry-level slice of
  bitemporal resolution — full authority ordering and dependency
  completion belong to `capsule-resolver` (Phase 4).

Exposed via the CLI as `capsule registry <register|transition|publish|get|list|history|dependencies|dependents|evidence|audit>` (see `capsule-cli`).
