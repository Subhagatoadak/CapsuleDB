-- capsule-registry embedded SQLite schema (Phase 2 MVP backend).
-- See ../postgres-schema.sql for the drafted Postgres equivalent.
--
-- Design notes:
--   * capsule_payloads is content-addressed: one row per distinct
--     content_hash (the epistemic payload, see capsule-core::hashing),
--     shared by every capsule_versions row whose capsule hashes the same.
--   * capsule_versions rows are never updated after insertion, only
--     inserted — versions are immutable (spec §2.2). Current lifecycle
--     state is derived, not stored, from the latest lifecycle_events row.
--   * lifecycle_events is an append-only ledger: it doubles as the
--     approval/publication/revocation record the roadmap calls for.
--   * recorded_at / occurred_at are system (transaction) time, set by
--     the registry at insert time; valid_from / valid_until on
--     capsule_versions mirror the capsule's applicability window
--     (application/business time) for indexed bitemporal queries without
--     parsing the payload JSON.

CREATE TABLE IF NOT EXISTS capsule_payloads (
    content_hash    TEXT PRIMARY KEY,
    payload_json    TEXT NOT NULL,
    first_seen_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS capsule_versions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    logical_id      TEXT NOT NULL,
    version         TEXT NOT NULL,
    content_hash    TEXT NOT NULL REFERENCES capsule_payloads(content_hash),
    capsule_type    TEXT NOT NULL,
    owner           TEXT NOT NULL,
    domain          TEXT NOT NULL,
    permissions_json TEXT NOT NULL,
    valid_from      TEXT,
    valid_until     TEXT,
    recorded_at     TEXT NOT NULL,
    UNIQUE(logical_id, version)
);

CREATE INDEX IF NOT EXISTS idx_capsule_versions_logical_id ON capsule_versions(logical_id);
CREATE INDEX IF NOT EXISTS idx_capsule_versions_content_hash ON capsule_versions(content_hash);

CREATE TABLE IF NOT EXISTS lifecycle_events (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    logical_id      TEXT NOT NULL,
    version         TEXT NOT NULL,
    from_state      TEXT,
    to_state        TEXT NOT NULL,
    actor           TEXT NOT NULL,
    reason          TEXT,
    occurred_at     TEXT NOT NULL,
    FOREIGN KEY(logical_id, version) REFERENCES capsule_versions(logical_id, version)
);

CREATE INDEX IF NOT EXISTS idx_lifecycle_events_capsule ON lifecycle_events(logical_id, version, id);

CREATE TABLE IF NOT EXISTS dependency_records (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    logical_id              TEXT NOT NULL,
    version                 TEXT NOT NULL,
    depends_on_logical_id   TEXT NOT NULL,
    depends_on_version      TEXT,
    relation                TEXT NOT NULL,
    recorded_at             TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dependency_records_source ON dependency_records(logical_id, version);
CREATE INDEX IF NOT EXISTS idx_dependency_records_target ON dependency_records(depends_on_logical_id, depends_on_version);

CREATE TABLE IF NOT EXISTS evidence_links (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    logical_id      TEXT NOT NULL,
    version         TEXT NOT NULL,
    source_id       TEXT NOT NULL,
    authority       TEXT NOT NULL,
    recorded_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_evidence_links_source ON evidence_links(source_id);
CREATE INDEX IF NOT EXISTS idx_evidence_links_capsule ON evidence_links(logical_id, version);

CREATE TABLE IF NOT EXISTS audit_events (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type      TEXT NOT NULL,
    logical_id      TEXT,
    version         TEXT,
    actor           TEXT NOT NULL,
    detail_json     TEXT,
    occurred_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_events_capsule ON audit_events(logical_id, version);
