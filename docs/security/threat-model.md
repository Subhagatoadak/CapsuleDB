# Security threat model

Referenced from `capsule-api/src/lib.rs` and `capsule-api/README.md` as
"see the security threat model" — this is that document. It describes
what the current implementation actually does and doesn't defend
against, not an aspirational target. Where a mitigation isn't built, that
is stated plainly rather than implied.

## Scope

The system as it exists in this repository today: `capsule-core`,
`capsule-registry` (embedded SQLite, one file per tenant via
`TenantStore`), `capsule-compiler`, `capsule-resolver`,
`capsule-runtime`, `capsule-api` (the HTTP surface), `capsule-cli`,
`capsule-policy`, `capsule-observatory`, and the SDKs. Deployment
artifacts (`deploy/Dockerfile`, `deploy/k8s/*`) are in scope for the
container/network boundary; a specific organization's cloud account,
IAM, and network topology are not — those are the deploying
organization's threat model, informed by this one.

## Assets

- **Capsule content** — statements, evidence citations, confidence
  claims. Integrity matters more than confidentiality for most
  deployments: a capsule silently altered to overclaim is the harm this
  entire project exists to prevent (see the root README's "confidence
  laundering" framing).
- **The audit ledger** (`lifecycle_events`, `audit_events` — append-only,
  per `capsule-registry/README.md`) — the record of who approved,
  published, or revoked what, and when.
- **Signing keys** (`capsule keygen`, Ed25519) — a compromised secret key
  lets an attacker forge a signature claiming a capsule or `.kcap`
  package came from a trusted publisher.
- **Backup encryption keys** (`capsule datakey`, AES-256) — a compromised
  key exposes every capsule and every audit event in that backup, since
  the whole SQLite file is encrypted as one unit, not row-by-row.
- **Tenant data** — under `TenantStore`, one SQLite file per tenant; a
  breach that crosses tenant boundaries (path traversal to another
  tenant's file, or a shared process reading the wrong `Registry` handle)
  is a distinct, worse failure than a single-tenant breach.

## Trust boundaries and current mitigations

### `capsule-api` (network boundary)

**No authentication, no authorization beyond capsule-level
`governance.permissions`, no TLS, no rate limiting — all by design, not
oversight, and all stated in `capsule-api/README.md` and its module
doc comment.** This is a `tiny_http`-based service meant to run behind a
reverse proxy that terminates TLS and handles auth (mTLS, an API
gateway, a service mesh sidecar — whatever the deploying organization
already uses), not to be exposed to an untrusted network directly.

What *is* enforced at this layer:
- `AccessRequest.consumer_role` is checked against a capsule's
  `governance.permissions` list in `capsule-resolver`/`capsule-runtime`
  — an empty `permissions` list is public, a non-empty one requires a
  matching role, and this applies to anonymous requests too (see the
  resolver permission-bypass bug fixed during Phase 5 development: an
  anonymous request used to skip the check entirely).
  `can_read_evidence` similarly gates evidence-detail visibility.
- CORS is deliberately wide open
  (`Access-Control-Allow-Origin: *`) so that a static page
  (`capsule-studio`) can call the API with no proxy — this is a
  statement that the API has no session/cookie-based trust to leak, not
  an oversight. If a future version adds cookie auth, this must change
  first.
- Every request is logged with a `tracing` span (method, path, status,
  latency) — see `capsule-api/README.md`'s Observability section — so a
  reverse-proxy-terminated deployment still gets structured
  application-level logs for anomaly detection, even without app-level
  auth.

**Known gap, accepted for this build:** anyone who can reach the port can
call every route, including `register`/`transition`/`publish`. A
production deployment MUST put an authenticating proxy in front of this
service; `capsule-policy` (below) is not a substitute for that.

### `capsule-registry` (storage boundary)

- **Append-only ledgers.** `lifecycle_events`, `audit_events`,
  `dependency_records`, `evidence_links` are insert-only; nothing this
  crate exposes can rewrite history. An attacker with write access to the
  registry through the intended API surface can add fraudulent events but
  cannot erase real ones — the ledger is tamper-evident, not
  tamper-proof, since direct filesystem access to the SQLite file
  bypasses this entirely (see "SQLite file access," below).
- **Tenant isolation via `TenantStore`** (Phase 10) — one SQLite file per
  tenant rather than a shared-schema `tenant_id` column. Tenant
  identifiers are sanitized to `[a-zA-Z0-9_-]` before being used as a
  filename (`tenant.rs::sanitize`), specifically to close a path-traversal
  route from an untrusted tenant ID (e.g. `../../etc/passwd`) to a file
  outside the tenants directory — this is tested
  (`tenant_ids_are_sanitized_against_path_traversal`). What this does
  *not* provide: isolation between tenants sharing one OS process/user —
  a bug that opens the wrong tenant's `Registry` handle is still possible
  at the application layer; only the filesystem-level "no query can touch
  another tenant's rows" property is structural.
- **SQLite file access.** Anyone with filesystem read access to a
  registry's `.sqlite3` file can read every capsule, including drafts and
  role-restricted ones — `governance.permissions` is enforced by
  `capsule-resolver`/`capsule-runtime` in application code, not by SQLite
  itself. File permissions on the database file and its containing
  directory are the actual enforcement boundary; the Kubernetes manifests
  in `deploy/k8s/` mount it on a `PersistentVolumeClaim`, and that
  volume's access should be restricted to the `capsule-api` pod alone.
- **Corrupted/malicious database files fail closed.** Opening a
  non-database file as a registry surfaces a `RegistryError`, not a panic
  or silent data loss (tested:
  `opening_a_corrupted_file_as_a_database_fails_cleanly_instead_of_panicking`).
- **Concurrent writers.** `rusqlite`'s SQLite handles its own
  file-level locking; `capsule-api` additionally wraps its `Runtime` in
  `Arc<Mutex<_>>` so concurrent HTTP requests serialize at the
  application layer too (tested:
  `concurrent_registration_from_multiple_threads_does_not_corrupt_or_panic`).
  This is a correctness property (no corruption), not a scalability one —
  see `deploy/README.md`'s note on why the Kubernetes Deployment is
  pinned to one replica.

### Backups (`registry backup`/`restore`)

- `Registry::backup_to` uses SQLite's *online* backup API, so a backup
  taken against a live, concurrently-written registry is a consistent
  snapshot, not a torn read.
- `--encrypt-key-file` encrypts the backup file with AES-256-GCM
  (authenticated encryption: a wrong key or a tampered file fails
  decryption loudly, tested in `capsule-cli/src/crypto.rs`, rather than
  silently returning garbage).
- **What this doesn't cover:** key distribution and rotation are entirely
  the operator's responsibility — `capsule datakey` generates a key and
  nothing more. Losing the key means losing the backup; there is no
  recovery mechanism, by design (a recoverable key is a weaker key).

### Signing (`capsule keygen`, `pack --sign`/`unpack --verify-key`)

- Signs a capsule's *content hash*, not the full document — so the
  signature is stable across administrative changes (lifecycle state,
  governance) that don't touch the epistemic payload, per the
  `content_hash` fix described in `capsule-registry/README.md`.
- Verification is opt-in (`unpack --verify-key`); an unsigned or
  unverified package is not rejected by default — a deploying
  organization that requires signed publishers must enforce
  `--verify-key` itself (e.g. in CI, or a policy rule) since this crate
  doesn't have a global "require signatures" switch.
- Secret keys are hex-encoded files on disk with no additional
  protection (no OS keychain integration, no HSM support). Treat a
  `capsule keygen --out-prefix` secret-key file exactly like any other
  private key material: restrictive file permissions, not committed to
  version control, rotated if exposed.

### Policy engine (`capsule-policy`)

`PolicyEngine` is an *advisory gate at register time*
(`capsule registry register --policy policy.json`), enforced by whichever
process runs that command — it is not a server-side invariant enforced
by `capsule-registry` itself. A caller with direct `Registry::register`
access (i.e., anything written against this crate directly, bypassing
`capsule-cli`) does not go through policy at all. Policy-as-code narrows
what gets into the registry through the sanctioned path; it does not
replace filesystem/network access control as the actual boundary.

## Explicitly out of scope / not implemented

Consistent with this project's own epistemic-honesty principle — stating
what's unbuilt rather than implying coverage:

- **No authentication or authorization on `capsule-api` itself** — see
  above; this is a stated architectural boundary, not a gap to be
  silently patched later.
- **No enterprise identity integration** (SSO/OIDC/SAML) — the roadmap
  item is left unchecked in the root README; nothing here validates a
  token from a real identity provider.
- **No secrets manager / KMS integration** — `capsule keygen` and
  `capsule datakey` produce raw key material on disk; wiring them to
  Vault/KMS/a cloud secrets manager is left to the deploying
  organization.
- **No intrusion detection or anomaly alerting** on the `tracing` output
  described above — the structured logs exist; consuming them into an
  alerting pipeline does not.
- **No penetration testing has been performed** against this codebase.
  This document is a design-time threat analysis, not a report of an
  external security review.

## Recommendations for a production deployment

1. Put `capsule-api` behind a reverse proxy or gateway that terminates
   TLS and performs authentication before any request reaches it.
2. Restrict filesystem access to each tenant's `.sqlite3` file (and its
   containing `PersistentVolumeClaim`, in the Kubernetes deployment) to
   the `capsule-api` process's own user/service account.
3. Store signing secret keys and backup data-encryption keys in a real
   secrets manager, not as bare files, and rotate them on any suspected
   exposure.
4. Take `registry backup` on a schedule and store the encrypted output
   somewhere other than the same host as the live registry.
5. Treat `capsule-policy` rules as a data-quality gate, not a security
   control — pair it with the access restrictions above, not instead of
   them.
