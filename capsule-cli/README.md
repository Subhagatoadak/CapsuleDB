# capsule-cli

The `capsule` command line tool, built on `capsule-core`.

```bash
cargo run -p capsule-cli -- inspect examples/capsules/data-retention-90-days.json
cargo run -p capsule-cli -- validate examples/capsules/data-retention-90-days.json
cargo run -p capsule-cli -- pack examples/capsules/data-retention-90-days.json -o retention.kcap
cargo run -p capsule-cli -- unpack retention.kcap -o ./out
```

- `inspect` — pretty-prints a capsule's identity, epistemic status,
  confidence, evidence, applicability, contradictions, dependencies, and
  content hash.
- `validate` — runs every KCP 0.1 structural/epistemic check and exits
  non-zero if any check fails (warnings, e.g. unresolved contradictions,
  do not fail the exit code).
- `pack` — refuses to package an invalid capsule; produces a `.kcap`
  archive per `protocol/kcap-package-format.md`, optionally bundling an
  evidence directory.
- `unpack` — extracts a `.kcap` archive and verifies every file's checksum
  against the manifest and the capsule's own content hash, so
  archive-level or JSON-level tampering are both detected.
