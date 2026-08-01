# `.kcap` Package Format — Draft v0.1

Status: Draft (Phase 0)

A `.kcap` file is a portable, self-contained distribution unit for one
Knowledge Capsule and its supporting material. It is a **gzip-compressed
POSIX tar archive** (`tar.gz`) with a fixed internal layout:

```text
example.kcap
├── manifest.json      # package metadata (required)
├── capsule.json        # the capsule document (required)
└── evidence/           # referenced source excerpts, optional
    ├── <source_id>.txt
    └── ...
```

## manifest.json

```json
{
  "kcp_version": "0.1",
  "packaged_at": "RFC 3339 timestamp",
  "capsule_logical_id": "kc://domain/topic/name",
  "capsule_version": "1.0.0",
  "content_hash": "sha256:<hex of capsule.json canonical form>",
  "files": [
    { "path": "capsule.json", "sha256": "<hex>" },
    { "path": "evidence/source-1.txt", "sha256": "<hex>" }
  ]
}
```

Rules:

- `manifest.json` and `capsule.json` MUST be present at the archive root.
- `content_hash` in the manifest MUST equal the capsule's own
  `content_hash` (spec §2.3); `capsule pack` computes and stamps both from
  the same canonicalization routine, and `capsule unpack` / `capsule
  validate` re-derive and compare it as a tamper check.
- Every file listed under `evidence/` MUST appear in `manifest.files` with
  a matching SHA-256, so partial or corrupted extraction is detectable.
- Unknown top-level entries are ignored by L1-conformant readers but MUST
  be preserved on round-trip (pack → unpack → pack) where practical.

## Why tar.gz and not zip

Deterministic, streaming-friendly, ubiquitous tooling, and it keeps the
Rust dependency surface small (`tar` + `flate2`) for `capsule-core`/
`capsule-cli`. This is an implementation choice for v0.1, not a protocol
commitment — a future KCP revision may add a zip variant if a consuming
ecosystem needs it.
