# Knowledge Capsule Protocol (KCP) v0.1 — Draft

Status: Draft (Phase 0)

KCP defines how Knowledge Capsules are represented, packaged, addressed, and
exchanged between compilers, registries, resolvers, runtimes, and
evaluators. KCP does not define storage engines or query execution — those
are implementation concerns of each component.

## 1. Scope

KCP v0.1 covers:

1. The capsule document format (delegates to
   `docs/specification/knowledge-capsule-spec.md`).
2. Capsule identity and addressing (`kc://` URIs, content hashes).
3. The `.kcap` portable package format (see `kcap-package-format.md`).
4. The minimal set of lifecycle transitions a conforming implementation must
   enforce.
5. The wire-level contract for resolution requests and responses (draft,
   subject to change in Phase 4).

Out of scope for v0.1: transport/auth, query language grammar (CQL is
sketched in the README and formalized in a later revision), federation
trust policy (Phase 9).

## 2. Conformance levels

- **L0 — Document conformance**: can parse, validate, and serialize a
  capsule document per the specification. No storage or resolution
  required. (`capsule-core` targets L0.)
- **L1 — Package conformance**: can produce and consume `.kcap` packages
  with correct manifests and content hashes.
- **L2 — Registry conformance**: enforces lifecycle transitions, immutable
  versioning, and supersession/revocation semantics over a persistent
  store.
- **L3 — Resolution conformance**: answers resolution requests (§5) with
  inspectable, replayable resolution plans.

A component declares which conformance levels it implements.

## 3. Identity

See spec §2. KCP additionally defines a canonical string form for
referencing a specific version:

```text
kc://policy/data-retention/customer-data-90-days@1.2.0
```

and a specific content-addressed artifact:

```text
kc://policy/data-retention/customer-data-90-days#sha256:9f3a...
```

Resolvers MUST treat the `@version` and `#hash` forms as pinning
constraints, not as separate identities.

## 4. Native operations

KCP defines the following operations as first-class verbs. Each maps to a
required lifecycle transition (spec §4) or a read-only action.

| Operation  | Effect                                                                 |
| ---------- | ----------------------------------------------------------------------|
| COMPILE    | Produce a candidate capsule (`draft`) from source material.           |
| VALIDATE   | Run structural + epistemic validation; does not change state.         |
| APPROVE    | `in_review → approved`, recording an approval record.                 |
| PUBLISH    | `approved → published`, assigning an immutable version.               |
| RESOLVE    | Read-only: return the applicable capsule set for a request.           |
| VERIFY     | Read-only: check an answer/decision against a capsule contract.       |
| COMPOSE    | Combine multiple capsules into a resolution bundle or pack.           |
| SUPERSEDE  | `published → superseded`, linking to the replacing capsule.           |
| REVOKE     | `* → revoked`, recording a revocation reason.                         |
| EVOLVE     | Produce a new version of a capsule, preserving lineage.               |

## 5. Resolution request contract (draft)

```json
{
  "purpose": "string",
  "about": "string",
  "jurisdiction": "string | null",
  "as_of": "RFC 3339 date | null",
  "require_status": ["published"],
  "include": ["exceptions", "contradictions", "dependencies"]
}
```

A resolution response is a **resolution bundle**: the resolved capsule set,
any contradictions found, any unmet dependencies, and an inspectable plan
describing why each capsule was included or excluded. Formalized in Phase
4.

## 6. Versioning of KCP itself

KCP versions as `kcp/<major>.<minor>`. `capsule-core` in Phase 1 targets
`kcp/0.1` only. Unknown schema versions are rejected, not best-effort
parsed, to avoid silently dropping epistemic fields.
