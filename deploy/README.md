# deploy

Container image and Kubernetes manifests for `capsule-api` (Phase 10).

## Docker

```bash
docker build -f deploy/Dockerfile -t capsuledb/capsule-api:latest .
docker run -p 8089:8089 -v capsuledb-data:/data capsuledb/capsule-api:latest
```

Multi-stage build: a `rust:1-bookworm` builder stage compiles
`capsule-api` in release mode (`rusqlite`'s `bundled` feature compiles
SQLite from source, so the builder needs a C toolchain — the runtime
image doesn't), then a `debian:bookworm-slim` runtime stage ships only
the resulting binary, running as a non-root `capsule` user, with `/data`
as the mount point for the SQLite file.

**This has been built and run in this environment** — `docker build`
against the exact `Dockerfile` in this directory, then `docker run` and a
`curl /health` against the running container, both succeeded. What
hasn't been done: pushing the image to a real registry, or applying the
manifests below against a real cluster (this environment has no
Kubernetes cluster to test against) — treat the YAML as a reviewed
starting point, not something verified end-to-end the way the Docker
build was.

## Kubernetes

```bash
kubectl apply -f deploy/k8s/namespace.yaml
kubectl apply -f deploy/k8s/pvc.yaml
kubectl apply -f deploy/k8s/deployment.yaml
kubectl apply -f deploy/k8s/service.yaml
```

- `namespace.yaml` — a dedicated `capsuledb` namespace.
- `pvc.yaml` — a 5Gi `ReadWriteOnce` volume for the SQLite file.
- `deployment.yaml` — **pinned to `replicas: 1`, `strategy: Recreate`.**
  `capsule-registry`'s backend is a single SQLite file (see
  `capsule-registry/README.md`); it is a single-writer database, not a
  clustered one. A second pod sharing the same file — which
  `RollingUpdate`'s overlap window or a naive `replicas: N` would both
  produce — means two processes writing the same file concurrently, which
  SQLite does not arbitrate for you. `Recreate` accepts a brief outage on
  rollout in exchange for never having two writers. Scaling this
  service beyond one pod is not a manifest change; it needs
  `capsule-registry` to grow a real multi-writer backend first (the
  `postgres-schema.sql` draft in that crate is the intended path, not yet
  wired up as a `Registry` backend).
- `service.yaml` — a `ClusterIP` Service in front of the single pod. Put
  a real Ingress/LoadBalancer and TLS termination in front of this
  yourself — `capsule-api` speaks plaintext HTTP with no auth of its own
  (see `docs/security/threat-model.md`), so it is not meant to be
  exposed directly to the internet.

Set `image:` in `deployment.yaml` to wherever you push the image built
above; it defaults to `capsuledb/capsule-api:latest`, which is a
placeholder tag, not a published image — this repo does not publish one.
