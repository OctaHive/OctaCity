# Phase 6 Vault and artifact contract test

The phase-six completion test crosses the real boundaries that unit fakes
cannot validate together:

1. `JobLifecycle` runs a signed lease through the real `JobExecutor` and runner
   supervisor;
2. an operator-owned JWT is copied into a private per-job identity lease;
3. the real `octa-runner` authenticates to Vault and resolves a KV v2 secret;
4. Octa redacts the resolved Vault secret, while the agent supervisor removes
   the verbatim workload JWT from runner events and terminal results before
   either can enter durable state;
5. the identity lease is revoked after runtime destruction and before output
   inspection starts;
6. the host validates and freezes an artifact and an opaque-format report;
7. the real coordinator HTTP client obtains fenced presigned PUT targets;
8. MinIO verifies the signed SHA-256 and receives the bytes without exposing
   S3 credentials to the agent;
9. `S3ArtifactStore` requires the SHA-256 on the signed PUT, verifies its HEAD
   checksum or, when the backend omits it, streams the exact object generation
   through server-side SHA-256 before publishing immutable bytes and
   authorizing downloads;
10. while output publication owns the retained workspace, the test scans the
    real event spool, journal, results, workspace, and upload metadata for both
    the Vault secret and workload JWT; terminal completion and cleanup are then
    required to succeed.

The contract uses a small host-process execution adapter so it can run on a
standard GitHub runner without delegated cgroups. It verifies that both the
signed job and local agent policy select a restricted, Vault-only network
allowlist, but the adapter does not claim to enforce that allowlist. It also
points the Octa Vault profile at the job-private host copy because there is no
mount namespace in this adapter. The fixed read-only `/run/octa-identity` mount
and restricted network-policy construction are covered by the dedicated
self-hosted Microsandbox backend contract. Native deliberately rejects
hostname allowlists because its Linux namespace backend cannot enforce
DNS-aware egress safely. Together the service and backend contracts cover the
real Vault protocol and the isolated-backend boundary without pretending that
the portable adapter itself is a sandbox. All orchestration above the execution
seam is production code.

The embedded HTTP coordinator is intentionally an in-memory protocol fixture.
It atomically reserves idempotency keys before object-store I/O, but it does not
claim server durability; PostgreSQL-backed upload records belong to the minimal
real server in Phase 8.

The test is ignored by the portable suite because it needs two external
services and built Octa executables. The `backend contracts` workflow runs it
with pinned Vault and MinIO containers. To reproduce it locally:

```sh
docker run --detach --rm --name octacity-phase6-vault --cap-add=IPC_LOCK \
  -p 127.0.0.1:18200:8200 \
  -e VAULT_DEV_ROOT_TOKEN_ID=octacity-root \
  -e VAULT_DEV_LISTEN_ADDRESS=0.0.0.0:8200 \
  hashicorp/vault:1.20.4@sha256:268bb80aa9c6d13d65fcfa05c0c268caca068952240a8087291a6ce0b66e3a10

docker run --detach --rm --name octacity-phase6-minio \
  -p 127.0.0.1:19000:9000 \
  -e MINIO_ROOT_USER=octacity \
  -e MINIO_ROOT_PASSWORD=octacity-secret \
  quay.io/minio/minio:RELEASE.2025-09-07T16-13-09Z@sha256:14cea493d9a34af32f524e538b8346cf79f3321eff8e708c1e2960462bd8936e server /data

OCTACITY_MINIO_ENDPOINT=http://127.0.0.1:19000 \
OCTACITY_MINIO_ACCESS_KEY=octacity \
OCTACITY_MINIO_SECRET_KEY=octacity-secret \
OCTACITY_VAULT_ENDPOINT=http://127.0.0.1:18200 \
OCTACITY_VAULT_ROOT_TOKEN=octacity-root \
OCTACITY_PHASE6_OCTA_RUNNER=/absolute/path/to/octa/target/debug/octa-runner \
OCTACITY_PHASE6_OCTA_PLUGINS_DIR=/absolute/path/to/octa/target/debug \
cargo test -p octacity-phase6-contract-tests --test protocol_minio \
  real_octa_vault_job_publishes_outputs_without_leaking_secrets \
  -- --ignored --exact --nocapture
```

The separate `s3_store_satisfies_the_minio_contract` test exercises upload,
independent size and SHA-256 verification even when object metadata lies,
idempotent publication, short-lived upload and download expiration, direct
download, deletion, a forced storage outage, full capability requalification,
and recovery against the same MinIO service. Failed verification and outage
paths assert that no download capability can be issued for unpublished bytes.
