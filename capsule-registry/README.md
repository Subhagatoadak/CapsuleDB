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

Exposed via the CLI as `capsule registry <register|transition|publish|get|list|history|dependencies|dependents|evidence|audit|export|import|backup|restore|audit-export>` (see `capsule-cli`).

## Tenant isolation

`TenantStore` (Phase 10) opens one SQLite file per tenant under a shared
base directory, rather than adding a `tenant_id` column to the shared
schema above:

```rust
let store = capsule_registry::TenantStore::new("./tenants")?;
let mut acme = store.registry_for("acme-corp")?;   // ./tenants/acme-corp.sqlite3
let mut globex = store.registry_for("globex")?;    // ./tenants/globex.sqlite3
```

Five other crates are built directly on `Registry`'s query surface;
retrofitting every one of those queries with `WHERE tenant_id = ?` is a
lot of surface area to get right, and a single missed clause anywhere is
a cross-tenant data leak. A file per tenant makes that class of bug
structurally impossible — there is no connection open to another
tenant's file, so there is no query that could read its rows. The
trade-off is real (no cross-tenant SQL joins, N open file handles
instead of one) and is judged worth it here. Tenant IDs are sanitized to
`[a-zA-Z0-9_-]` before being used as a filename, so an untrusted tenant
identifier (e.g. from an API request) can't path-traverse out of the
base directory.

## Backup and disaster recovery

`Registry::backup_to` uses SQLite's online backup API to write a
consistent point-in-time copy while the source stays open for
concurrent reads/writes — unlike copying the file's bytes directly,
which can capture a torn write mid-transaction. `Registry::audit_log_all`
returns the complete, unbounded audit ledger (oldest first) for
compliance export, as opposed to `audit_log`'s bounded newest-first page
for interactive display. Both are exposed via `capsule registry
backup`/`restore`/`audit-export`, with optional AES-256-GCM
encryption-at-rest for the backup file (see `capsule-cli/README.md`).
