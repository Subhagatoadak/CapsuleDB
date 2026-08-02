# `.kcap` Package Format — Draft v0.1

Status: Draft (Phase 0); implemented by `capsule pack`/`capsule unpack`
(Phase 1) including optional signing (Phase 9).

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
  "content_hash": "sha256:<hex of the capsule's epistemic payload — see spec §2.3>",
  "files": [
    { "path": "capsule.json", "sha256": "<hex>" },
    { "path": "evidence/source-1.txt", "sha256": "<hex>" }
  ],
  "signature": "hex Ed25519 signature over content_hash (optional)",
  "public_key": "hex Ed25519 public key that produced signature (optional)"
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
- `signature`/`public_key` are optional and always appear together
  (signed publishers, spec-adjacent — see
  `capsule-runtime/README.md`'s signing section). A reader that does not
  care about signatures MUST still accept an unsigned package; a reader
  asked to verify one MUST reject a package where exactly one of the two
  fields is present, since that can only mean a truncated or hand-edited
  manifest.

## Compatibility rules

- **Readers ignore unknown top-level manifest fields.** A `.kcap` reader
  built against an earlier KCP minor version MUST NOT fail on a
  manifest field it doesn't recognize (this is how `signature`/
  `public_key` were added without a format break) — it just can't act
  on what it doesn't understand.
- **Writers always set `kcp_version`.** A reader that encounters a
  `kcp_version` newer than the highest minor version it implements
  SHOULD warn rather than refuse, since KCP minor versions are
  additive-only (spec §1); a *major* version bump is a genuine breaking
  change and MAY be refused outright.
- **`content_hash` is the compatibility anchor, not the archive
  layout.** Two `.kcap` files can differ in file ordering, gzip
  compression level, or which optional fields are present in
  `manifest.json`, and still be "the same package" as long as
  `content_hash` and `capsule.json`'s payload match — that's the
  property `capsule unpack` actually checks, not byte-for-byte archive
  equality.
- **Evidence files are additive, never required for validity.** A
  reader MUST be able to validate a capsule and verify its signature
  using only `manifest.json` and `capsule.json`; missing or corrupted
  `evidence/*` files are a data-completeness problem for whoever wanted
  the excerpts, not a reason to reject the package as malformed.

## Why tar.gz and not zip

Deterministic, streaming-friendly, ubiquitous tooling, and it keeps the
Rust dependency surface small (`tar` + `flate2`) for `capsule-core`/
`capsule-cli`. This is an implementation choice for v0.1, not a protocol
commitment — a future KCP revision may add a zip variant if a consuming
ecosystem needs it.
