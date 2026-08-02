# capsule-core

Rust implementation of the KCP 0.1 Knowledge Capsule document model:
types, JSON (de)serialization, canonical content hashing, lifecycle
transition enforcement, and structural/epistemic validation.

Targets **L0 (document conformance)** per `protocol/KCP-v0.1.md`. It does
not persist, resolve, or transport capsules — see `capsule-registry`,
`capsule-resolver`, and `capsule-runtime` for those (later phases).

```rust
let capsule = capsule_core::parse(&json)?;
let issues = capsule_core::validate(&capsule);
let capsule = capsule_core::stamp_content_hash(capsule)?;
```

Normative references: `docs/specification/knowledge-capsule-spec.md`,
`schemas/capsule.schema.json`.
