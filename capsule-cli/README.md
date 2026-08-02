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
  evidence directory. `--sign <secret-key-file>` (see `keygen` below)
  signs the package's content hash.
- `unpack` — extracts a `.kcap` archive and verifies every file's checksum
  against the manifest and the capsule's own content hash, so
  archive-level or JSON-level tampering are both detected.
  `--verify-key <public-key-file>` additionally requires the package's
  signature to verify against that key.

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
capsule registry --db capsules.sqlite3 export --out capsules.jsonl
capsule registry --db other.sqlite3 import capsules.jsonl --actor bulk-import
```

`register` requires the capsule to be `lifecycle_state: draft` — the
canonical examples in `examples/capsules/` are `published` fixtures for
illustrating the spec, so set `lifecycle_state` to `draft` before
registering a copy of one. `publish` auto-supersedes any `supersedes`
dependency target that is currently `published` (see
`capsule-registry/README.md`).

`export` dumps every registered capsule version as one JSON document per
line. `import` bulk-registers from that file into a (possibly different)
registry, replaying each capsule's original lifecycle state — a capsule
exported as `published` comes back `published`, not stuck at `draft` —
since lifecycle state and governance are excluded from the content hash
(spec §2.3) and can be re-derived without changing the capsule's
identity. Auto-supersession from `publish` is not replayed; import uses
plain transitions.

## Compile

```bash
capsule compile policy.md --domain policy --topic retention --owner alice --out-dir drafts/
```

Extracts one draft candidate capsule per sentence from a text/Markdown/HTML/PDF
source (`--format` overrides extension-based detection), writes each as
`drafts/NNN.json`, and prints per-candidate epistemic classification,
detected qualifications/exclusions/conditions, bare `kc://` references
(not auto-wired as dependencies), and the deterministic validation issues
already run against each candidate — see `capsule-compiler/README.md`.
A typical next step is `capsule registry register drafts/001.json --actor
you` once you've reviewed a candidate.

## Resolve

```bash
capsule resolve "privacy review" --db capsules.sqlite3 --domain policy --about retention --jurisdiction IN --plan
```

Resolves the applicable capsule set for a request: filters by domain,
keyword, jurisdiction, capsule type, and consumer role; picks the
temporally-applicable published version of each match; pulls in
`requires` dependencies (propagating the confidence-ceiling rule) and any
contradictions; and — with `--plan` — prints the full inclusion/exclusion
decision trail. See `capsule-resolver/README.md`.

Note: a request with no `--consumer-role` is anonymous/public and will
not see capsules whose `governance.permissions` is non-empty — see
`capsule-runtime/README.md`.

## Keygen

```bash
capsule keygen                        # print public_key / secret_key hex to stdout
capsule keygen --out-prefix my-key    # write my-key.pub and my-key.key
```

Generates an Ed25519 keypair for signing capsule content hashes (see
`capsule-runtime/README.md`). Signing/verification of `.kcap` packages
with this key is a `capsule-cli` `pack --sign`/`unpack` feature (Phase 9,
signed publishers).

## Evaluate

```bash
capsule evaluate answer.txt --db capsules.sqlite3 --discretion low
```

Decomposes `answer.txt` into claims, maps each to the capsules that
support it, and runs every deterministic detector: unsupported claims,
applicability expansion, inference-to-fact / association-to-causation
(confidence laundering), confidence-ceiling violations, missing
mandatory qualifications, contradiction suppression, and evidence
multiplication across shared sources. Prints the evaluation-surface
manifest and the three scores. `--discretion` widens the ambiguous-
support band referred to a judge; no real judge is wired in yet (see
`capsule-evaluator/README.md`), so referred claims always come back
"Abstain."

## Observatory

```bash
capsule observatory --db capsules.sqlite3 --freshness-days 180 --knowledge-debt-days 30
```

Registry-wide health checks: freshness, unresolved contradictions,
orphaned (no-dependent) capsules, broken dependencies, confidence-
laundering incidents (a capsule's own statement vs. its own evidence
excerpt), knowledge debt (stale unpublished drafts), expiry alerts
(published past `applicability.valid_until`), and the capsule reuse
ratio. See `capsule-observatory/README.md`.

## Packs

```bash
capsule pack-create retention.pack.json --name retention-policy \
  --description "Everything needed to answer a retention question" \
  kc://policy/data-retention/customer-data-90-days \
  kc://policy/data-retention/legal-hold-request-procedure

capsule pack-resolve retention.pack.json --db capsules.sqlite3
```

A Capsule Pack is a named, versioned bundle of capsule *references* —
the unit the README's Capsule Query Language sketches as `VERIFY answer
AGAINST PACK "..."`. Distinct from a `.kcap` package (one signed,
self-contained archive): a pack manifest is lightweight JSON pointing
into a registry, not a copy of the capsules. `pack-create` writes the
manifest; `pack-resolve` looks up every member (unpinned references
resolve to the latest published version) and fails if any is missing
or not published.
