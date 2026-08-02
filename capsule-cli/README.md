# capsule-cli

The `capsule` command line tool, built on `capsule-core` and `capsule-registry`.

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

## Registry

```bash
capsule registry --db capsules.sqlite3 register draft-capsule.json --actor alice
capsule registry --db capsules.sqlite3 transition kc://policy/x/y 1.0.0 in_review --actor alice
capsule registry --db capsules.sqlite3 transition kc://policy/x/y 1.0.0 approved --actor bob
capsule registry --db capsules.sqlite3 publish kc://policy/x/y 1.0.0 --actor bob
capsule registry --db capsules.sqlite3 get kc://policy/x/y 1.0.0
capsule registry --db capsules.sqlite3 list kc://policy/x/y
capsule registry --db capsules.sqlite3 history kc://policy/x/y 1.0.0
capsule registry --db capsules.sqlite3 dependencies kc://policy/x/y 1.0.0
capsule registry --db capsules.sqlite3 dependents kc://policy/x/y 1.0.0
capsule registry --db capsules.sqlite3 evidence some-source-id
capsule registry --db capsules.sqlite3 audit --limit 20
```

`register` requires the capsule to be `lifecycle_state: draft` — the
canonical examples in `examples/capsules/` are `published` fixtures for
illustrating the spec, so set `lifecycle_state` to `draft` before
registering a copy of one. `publish` auto-supersedes any `supersedes`
dependency target that is currently `published` (see
`capsule-registry/README.md`).
