# capsule-registry

Status: **not started** — Phase 2 in the [roadmap](../README.md#phase-2--capsule-registry).

Will own capsule identity, immutable versions, lifecycle transitions,
ownership, bitemporal validity, approval/publication/revocation records,
content-addressed deduplication, dependency records, evidence-family
tracking, and audit events, on top of PostgreSQL (with a local SQLite mode).

Depends on `capsule-core` (Phase 1, done) for the document model, hashing,
and lifecycle rules it will persist and enforce.
