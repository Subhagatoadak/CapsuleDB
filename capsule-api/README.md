# capsule-api

A minimal HTTP JSON API over `capsule-runtime` — the "remote API
runtime" deployment mode (Phase 5).

```bash
cargo run -p capsule-api -- --db capsules.sqlite3 --addr 127.0.0.1:8089
```

```text
GET  /health
GET  /capsule?logical_id=...&version=...[&consumer_role=...][&can_read_evidence=true]
GET  /versions?logical_id=...
POST /resolve[?consumer_role=...]        body: ResolutionRequest JSON
POST /register?actor=...                 body: capsule JSON (must be draft)
POST /transition?logical_id=...&version=...&to=...&actor=...[&reason=...]
POST /publish?logical_id=...&version=...&actor=...
```

Routes use query parameters rather than path segments for capsule
identity, since a `logical_id` (`kc://domain/topic/name`) already
contains slashes that would collide with path routing.

`GET /capsule` and `POST /resolve` go through `capsule_runtime::Runtime`,
so policy enforcement (role-restricted capsules 403 for a non-matching or
anonymous `consumer_role`), consumer-specific views (evidence redaction),
local caching, and usage telemetry all apply over the network, not just
when the runtime is embedded in-process. The mutating routes
(`register`/`transition`/`publish`) go straight to the underlying
registry.

Built on `tiny_http` — a small, dependency-light blocking HTTP/1.1
server — rather than a full async web framework: this is a single
logical service with modest concurrency needs. It is single-process,
plaintext HTTP, with **no auth of its own** beyond whatever a reverse
proxy in front of it provides; fine for local/dev use and for
`sdk-python`/`sdk-typescript` to exercise, not yet a hardened
public-facing service (see `docs/security/threat-model.md`).

Tested with real end-to-end HTTP requests over `TcpStream` (see
`tests/api.rs`) rather than mocks — including a round trip through
`register → transition → publish → get → resolve`, a 403 for an
anonymous request against a role-restricted capsule, and a 400 for an
invalid capsule.
