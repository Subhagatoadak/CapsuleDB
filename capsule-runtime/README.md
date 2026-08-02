# capsule-runtime

The embeddable local runtime (Phase 5): serves capsules from a
`capsule-registry`, enforcing policy, applying consumer-specific views,
caching, recording usage, and verifying signed content hashes.

```rust
let registry = capsule_registry::Registry::open("capsules.sqlite3")?;
let runtime = capsule_runtime::Runtime::new(registry);

let access = capsule_runtime::AccessRequest { consumer_role: Some("legal-team".into()), ..Default::default() };
let view = runtime.get("kc://policy/x/y", "1.0.0", &access)?;
```

- **Policy enforcement / purpose-bound access** (`view::check_access`) —
  a capsule with a non-empty `governance.permissions` is visible only to
  a request presenting one of those roles; an anonymous request is
  treated as unprivileged, never as unrestricted.
- **Consumer-specific views** (`view::build_view`) — evidence excerpts
  are redacted (`"[redacted]"`) unless the access request grants
  `can_read_evidence`.
- **Local cache** (`cache::Cache`) — an in-memory, process-lifetime cache
  populated on `get()` misses; this is also the "offline mode" story —
  once cached, a capsule needs no further registry access.
- **Usage telemetry** (`telemetry::Telemetry`) — an in-process log of
  `get`/`resolve` calls, inspectable via `Runtime::telemetry()`.
- **Update polling / revocation propagation** (`Runtime::poll_updates`) —
  polls the registry's audit log since the last call and invalidates any
  cached capsule a lifecycle transition touched. Pull-based, not
  push-based: there is no subscription/webhook mechanism here.
- **Signed capsule verification** (`signing.rs`) — Ed25519 signatures over
  a capsule's *content hash* (see `capsule-core::hashing`), so a
  signature stays valid however the same epistemic payload is later
  registered or renamed. `KeyPair::generate`/`sign_content_hash` and the
  free function `verify_content_hash` are self-contained: no key
  distribution, revocation, or trust chain — a verifier needs the
  publisher's public key out of band. Exposed via `capsule keygen` in
  `capsule-cli`.

Not implemented: multi-registry replication ("sync with a central
registry" is polling against a *single* registry for cache invalidation,
not registry-to-registry sync). `capsule-api` (the remote/networked
counterpart) builds directly on this crate's `Runtime` for its `GET
/capsule` and `POST /resolve` routes, so policy enforcement, views,
caching, and telemetry apply over the network too, not only when
embedded in-process; its mutating routes (`register`/`transition`/
`publish`) go straight to `Runtime::registry_mut()` since those aren't
part of this crate's read-path story yet.
