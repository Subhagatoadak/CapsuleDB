-- Draft PostgreSQL schema for capsule-registry (Phase 2 roadmap item
-- "Design the PostgreSQL schema").
--
-- This is a design artifact, not a wired-up backend: the Phase 2 MVP
-- implementation runs against embedded SQLite (see src/schema.sql), which
-- is trivial to stand up in CI, tests, and local CLI use without a
-- server. This file specifies the equivalent Postgres shape so a
-- `capsule-registry` Postgres backend can be added later without
-- redesigning the model — same tables, same columns, same semantics
-- (content-addressed payload dedup, append-only lifecycle/audit ledgers,
-- bitemporal validity columns), translated to Postgres types.

CREATE TABLE IF NOT EXISTS capsule_payloads (
    content_hash    TEXT PRIMARY KEY,
    payload_json    JSONB NOT NULL,
    first_seen_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS capsule_versions (
    id                  BIGSERIAL PRIMARY KEY,
    logical_id          TEXT NOT NULL,
    version             TEXT NOT NULL,
    content_hash        TEXT NOT NULL REFERENCES capsule_payloads(content_hash),
    capsule_type        TEXT NOT NULL,
    owner               TEXT NOT NULL,
    domain              TEXT NOT NULL,
    permissions         TEXT[] NOT NULL DEFAULT '{}',
    valid_from          DATE,
    valid_until         DATE,
    recorded_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(logical_id, version)
);

CREATE INDEX IF NOT EXISTS idx_capsule_versions_logical_id ON capsule_versions(logical_id);
CREATE INDEX IF NOT EXISTS idx_capsule_versions_content_hash ON capsule_versions(content_hash);
CREATE INDEX IF NOT EXISTS idx_capsule_versions_validity ON capsule_versions(logical_id, valid_from, valid_until);

CREATE TABLE IF NOT EXISTS lifecycle_events (
    id              BIGSERIAL PRIMARY KEY,
    logical_id      TEXT NOT NULL,
    version         TEXT NOT NULL,
    from_state      TEXT,
    to_state        TEXT NOT NULL,
    actor           TEXT NOT NULL,
    reason          TEXT,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY(logical_id, version) REFERENCES capsule_versions(logical_id, version)
);

CREATE INDEX IF NOT EXISTS idx_lifecycle_events_capsule ON lifecycle_events(logical_id, version, id);

CREATE TABLE IF NOT EXISTS dependency_records (
    id                      BIGSERIAL PRIMARY KEY,
    logical_id              TEXT NOT NULL,
    version                 TEXT NOT NULL,
    depends_on_logical_id   TEXT NOT NULL,
    depends_on_version      TEXT,
    relation                TEXT NOT NULL,
    recorded_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_dependency_records_source ON dependency_records(logical_id, version);
CREATE INDEX IF NOT EXISTS idx_dependency_records_target ON dependency_records(depends_on_logical_id, depends_on_version);

CREATE TABLE IF NOT EXISTS evidence_links (
    id              BIGSERIAL PRIMARY KEY,
    logical_id      TEXT NOT NULL,
    version         TEXT NOT NULL,
    source_id       TEXT NOT NULL,
    authority       TEXT NOT NULL,
    recorded_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_evidence_links_source ON evidence_links(source_id);
CREATE INDEX IF NOT EXISTS idx_evidence_links_capsule ON evidence_links(logical_id, version);

CREATE TABLE IF NOT EXISTS audit_events (
    id              BIGSERIAL PRIMARY KEY,
    event_type      TEXT NOT NULL,
    logical_id      TEXT,
    version         TEXT,
    actor           TEXT NOT NULL,
    detail          JSONB,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_audit_events_capsule ON audit_events(logical_id, version);

-- Differences from the SQLite MVP schema, deliberately:
--   * JSONB instead of TEXT for payload_json/detail, enabling indexed
--     queries over epistemic fields directly in Postgres.
--   * permissions as TEXT[] instead of a JSON-encoded string.
--   * DATE/TIMESTAMPTZ instead of ISO-8601 TEXT for the bitemporal
--     columns, enabling native range queries.
-- The table names, columns, and derivation rules (current lifecycle
-- state = latest lifecycle_events.to_state; dedup key = content_hash)
-- are identical, so `capsule-registry`'s Rust API is not expected to
-- change shape when a Postgres backend is added — only its SQL layer.
