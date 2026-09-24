# OctaCity

OctaCity is the control plane and self-hosted agent for running
[Octa](https://github.com/OctaHive/octa) jobs. The repository now includes the
local-execution, coordinator-transport, and durable job-lifecycle milestones.
The workspace includes a runnable PostgreSQL-backed coordinator server,
versioned management REST API, authenticated Agent ingress, atomic placement,
and durable Build/Attempt/Job lifecycle. Native, containerd, and Microsandbox
execution backends are covered by platform-specific contract runners.

The current workspace contains:

- `octacity-artifact-store`: backend-neutral server-side artifact persistence
  port;
- `octacity-artifact-s3`: S3-compatible implementation of the backend-neutral
  Artifact Store contract;
- `octacity-webhook-provider-protocol`: strict provider-neutral verification,
  normalization, managed-registration, cancellation, and failure messages;
- `octacity-server-adapter-host`: shared executable verification, bounded
  framing, cancellation, timeout, and process cleanup for server adapters;
- `octacity-server-webhook`: operator-installed webhook-adapter registry with
  provider capabilities and protocol-specific supervision;
- `octacity-vcs-protocol`: strict provider-neutral references, commits, trees,
  bounded file content, immutable resolution, cancellation, and failures;
- `octacity-server-vcs`: operator-installed VCS-adapter registry with capability
  gating, credential-handle redaction, and protocol-specific supervision;
- `octacity-vcs-git`: read-only Git VCS adapter that browses ephemeral bare
  object databases without checkout or repository-code execution;
- `octacity-cache-session`: agent-owned L1 placement and short-lived remote
  cache authority preparation without implementing cache semantics;
- `octacity-protocol`: strict, signed server-agent job types;
- `octacity-config`: parsing and intrinsic validation of agent configuration;
- `octacity-coordinator`: bounded HTTPS, idempotent retry, lease polling, and
  independent fenced heartbeats;
- `octacity-execution`: backend-neutral execution ports and shared types;
- `octacity-execution-oci`: fail-closed OCI platform/isolation dispatch;
- `octacity-execution-containerd`: Linux OCI process engine through containerd's
  official Rust gRPC client;
- `octacity-execution-microsandbox`: OCI hypervisor engine through the pinned
  official Microsandbox SDK;
- `octacity-execution-native`: Linux cgroup-v2 Native backend;
- `octacity-inventory`: verified registration inventory and advisory host
  snapshots;
- `octacity-identity`: private per-job workload identity leases and revocation;
- `octacity-job`: verified source-to-runner lifecycle and cleanup owner;
- `octacity-lifecycle`: fenced lease ownership, append-only event spooling,
  replay, completion, and interrupted-attempt recovery;
- `octacity-output`: host-side validation, immutable snapshots, and fenced
  artifact/report uploads;
- `octacity-private-fs`: cross-platform private filesystem and no-follow
  primitives shared by security-sensitive state owners;
- `octacity-runner`: verified Octa inventory, runner protocol, and supervision;
- `octacity-source`: trusted source-plugin registry and process host;
- `octacity-source-plugin`: the bounded pre-Octa source protocol;
- `octacity-source-git`: exact, detached Git revision materialization through
  the source-plugin protocol;
- `octacity-phase6-contract-tests`: real Vault, Octa runner, HTTP upload, and
  MinIO completion contract;
- `octacity-agent`: the CLI and composition root that wires these components.

Run all checks with:

Clean builds require `protoc` because `containerd-client` generates its Rust
gRPC bindings during compilation. CI installs the compiler explicitly on every
supported runner OS.

```shell
cargo fmt --all -- --check
python3 tools/check_architecture.py
python3 -m unittest discover -s tools/tests -v
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D missing-docs" cargo doc --workspace --all-features --no-deps
cargo llvm-cov --workspace --all-features --summary-only \
  --fail-under-lines 80
```

CI pins `cargo-llvm-cov` 0.9.1 so coverage semantics cannot change when a new
tool release appears. The 80% portable Linux floor measures production source;
the tool excludes test-support files by default. Privileged Native, containerd,
and Microsandbox paths are additionally exercised by the backend contract jobs
described in [`docs/backend-contract-tests.md`](docs/backend-contract-tests.md).

The command line is defined with `clap`. Agent internals emit structured
`tracing` events, so service managers and collectors can redirect stderr or
consume JSON without coupling the implementation to a logging backend:

```shell
octacity-agent --log-filter octacity_agent=debug --log-format json \
  validate /etc/octacity/agent.toml

octacity-agent --log-filter octacity_agent=info --log-format json \
  run /etc/octacity/agent.toml
```

The server becomes ready only after its PostgreSQL migrations, database,
mandatory S3-compatible bucket, and JobSpec signing material are usable. When
using Build-log search, PostgreSQL must provide the `pg_trgm` extension. The
migration role must either be allowed to run `CREATE EXTENSION pg_trgm`, or an
administrator must install that extension before the server applies migrations;
otherwise migration readiness remains false rather than silently disabling
literal-fragment search.

If the derived Build-log search tables are lost or deliberately recreated,
queue an atomic Project-scoped replay while a server is running:

```shell
octacity-server rebuild-log-search /etc/octacity/server.toml \
  --project 00000000-0000-0000-0000-000000000001
```

The command reconstructs deletion tombstones first, clears only derived search
state, and requeues existing authoritative log work. Normal workers then read
retained verified chunks and rebuild the projection in configured bounded
batches. It does not change archived bytes, Job outcomes, or event cursors.

When `agent_bind` is configured, the independently authenticated Agent listener
serves registration at `/api/v1/agents/register` and lease long polling at
`/api/v1/agents/{agent_id}/leases:acquire`. When `cache_bind` is configured, it
independently serves authenticated Octa HTTP cache v1 traffic;
the public HTTPS `cache.endpoint` may terminate TLS at a trusted reverse proxy.
When `webhook_bind` is configured,
the separate webhook listener durably admits bounded raw deliveries. A worker
then authenticates them through an operator-installed, SHA-256-pinned provider
adapter before dispatching normalized events to the Trigger Engine. The
management API supports both manual remote-hook setup and
create/observe/rotate/delete through adapters that advertise those optional
managed capabilities. Management commands persist retryable provider work
before returning; leased workers perform the remote operation and retain
bounded retry or terminal diagnostics, so a restart cannot lose accepted
intent. Delivery verification and managed-registration workers intentionally
share the `webhook_worker_*` polling, claim, and retry policy; their independent
batch-size settings bound each workload separately. Manual Trigger intent is
likewise persisted before a mutable VCS lookup, and transient resolution
failures resume from leased work without creating another Build. Exact
revisions remain a local fast path. The repository intentionally ships no
production GitHub or Gerrit webhook adapter. A minimal `server.toml` is:

```toml
management_bind = "127.0.0.1:8080"
agent_bind = "127.0.0.1:8081"
cache_bind = "127.0.0.1:8083"
webhook_bind = "127.0.0.1:8082"
webhook_public_base_url = "https://hooks.example.test"
webhook_adapter_registry = "/etc/octacity/webhook-adapters"
webhook_operation_timeout_milliseconds = 5000
webhook_cancellation_grace_milliseconds = 1000
vcs_adapter_registry = "/etc/octacity/vcs-adapters"
vcs_operation_timeout_milliseconds = 30000
vcs_cancellation_grace_milliseconds = 1000
trigger_evaluation_poll_interval_milliseconds = 1000
trigger_evaluation_claim_lifetime_milliseconds = 180000
trigger_evaluation_batch_size = 4
vcs_retry_max_attempts = 5
vcs_retry_initial_milliseconds = 1000
vcs_retry_maximum_milliseconds = 60000
shutdown_grace_milliseconds = 10000
readiness_check_interval_milliseconds = 5000
readiness_check_timeout_milliseconds = 2000
agent_registration_lifetime_milliseconds = 86400000
agent_enrollment_lifetime_milliseconds = 900000
agent_max_retry_delay_milliseconds = 30000
agent_lease_lifetime_milliseconds = 300000
lease_expiry_poll_interval_milliseconds = 1000
lease_expiry_claim_lifetime_milliseconds = 30000
lease_expiry_batch_size = 32
schedule_poll_interval_milliseconds = 1000
schedule_claim_lifetime_milliseconds = 30000
schedule_batch_size = 32
internal_trigger_poll_interval_milliseconds = 1000
internal_trigger_claim_lifetime_milliseconds = 30000
internal_trigger_batch_size = 32
webhook_worker_poll_interval_milliseconds = 1000
webhook_worker_claim_lifetime_milliseconds = 30000
webhook_delivery_batch_size = 4
managed_webhook_batch_size = 4
webhook_worker_max_attempts = 5
webhook_worker_initial_retry_milliseconds = 1000
webhook_worker_maximum_retry_milliseconds = 60000
ready_job_listener_reconnect_milliseconds = 1000
supported_pipeline_capabilities = ["native"]

[[vcs_integrations]]
integration_id = "00000000-0000-0000-0000-000000000201"
adapter_id = "git"
adapter_sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
credential_handle = "secret:repository-main"

[postgres]
url_file = "/run/secrets/octacity-postgres-url"
max_connections = 10

[object_storage]
endpoint = "https://objects.example"
region = "us-east-1"
bucket = "octacity-artifacts"
prefix = "octacity/v1"
access_key_file = "/run/secrets/octacity-s3-access-key"
secret_key_file = "/run/secrets/octacity-s3-secret-key"
force_path_style = false
operation_timeout_milliseconds = 5000
capability_recheck_interval_milliseconds = 300000
upload_capability_lifetime_milliseconds = 300000
download_capability_lifetime_milliseconds = 60000

[signing]
key_id = "server-key-2026-09"
key_file = "/run/secrets/octacity-jobspec-ed25519-key"

[agent_credentials]
enrollment_key_file = "/run/secrets/octacity-agent-enrollment-key"

[cache]
endpoint = "https://cache.example"
credential_key_file = "/run/secrets/octacity-cache-credential-key"
session_lifetime_milliseconds = 300000
max_blob_bytes = 67108864

[job_spec]
policy_file = "/etc/octacity/job-spec-policy.json"
```

Credential files contain one value with an optional trailing newline. The
PostgreSQL file contains a connection URL, the object-store files contain the
two S3 credentials, and the signing file contains the standard-base64 encoding
of exactly 32 Ed25519 private-key bytes. The Agent enrollment key file likewise
contains standard-base64 encoding of an independently generated 32-byte key.
The cache credential key is another independent standard-base64 32-byte key;
it derives short-lived session bearers and is never shared with Agents.
Their contents are bounded, never
included in diagnostics, and loaded before the management listener binds.
`signing.key_id` is the non-secret identifier agents use to select the matching
verification key; rotate it together with the private key while retaining the
previous public key for the configured overlap window.
`job_spec.policy_file` is a strict bounded JSON document containing the
immutable source-plugin identity, Octa release and plugin digests, protocol
versions, and signed JobSpec validity for newly accepted Builds. See
[`docs/job-spec-policy.example.json`](docs/job-spec-policy.example.json).
Invalid policy stops startup before listeners bind.
Each credential path must name a regular file directly (symbolic links are rejected); on
Unix, set owner-only permissions such as `0400` or `0600`.
The file owner must also match the server process's effective user. The first
successful object-store readiness check performs a bounded zero-byte
PUT/GET/COPY/GET/DELETE probe below the configured prefix, so its identity
needs those object capabilities. Healthy recurring checks use `HeadObject` on
the process-owned marker and therefore do not require `s3:ListBucket` solely
for readiness. The full probe repeats after the configured capability recheck
interval and after any availability failure or failed artifact operation. For
a versioned bucket, lifecycle policy must expire noncurrent versions and delete
markers below `<prefix>/health/` as well as normal retained artifact objects.

Validate it without opening a listener, then run it with:

```shell
cargo run -p octacity-server -- validate server.toml
cargo run -p octacity-server -- run server.toml
```

It exposes `/health/live`, `/health/ready`, `/api/v1/openapi.json`, the
management API, and—when configured—the authenticated Agent API and webhook
callback `/webhooks/v1/integrations/{integration_id}` on their independent
listeners. Management v1 is
currently unauthenticated and belongs only on the configured trusted network;
Agent routes always require enrollment or current-registration credentials,
and webhook bodies always require provider verification.
Liveness is process-local. Readiness is a periodically refreshed, bounded
snapshot and can recover after PostgreSQL or object storage becomes available
again without restarting the process. Safe structured logs identify the failed
readiness dependency and whether it was unavailable or timed out, and emit only
initial state and transitions rather than one message per polling interval.

Native execution is deliberately Linux-only. It requires a delegated cgroup
v2 root and a dedicated, quota-sized filesystem mounted at `work_root`. Version
1 runs one job per agent, so that filesystem is the job's hard disk boundary;
the agent rejects Native execution when either boundary cannot be enforced.
Native processes inherit no service environment: operators provide the small
static environment, including `PATH`, through `native_environment`.

OCI hypervisor execution currently uses Microsandbox on Linux (`x86_64` and
`aarch64`) and Apple Silicon macOS. A macOS agent runs a `linux/arm64` OCI guest;
source plugins remain native macOS executables because source acquisition runs
on the host. Its OCI root overlay is RAM-backed and charged to the VM memory
limit, leaving the quota-limited workspace as the only disk-backed writable
mount. Intel macOS is rejected. Linux OCI process execution uses a
configured containerd socket, namespace, snapshotter, and runc/crun runtime
through gRPC; it currently accepts only `network = "disabled"`. Windows and
host-native macOS execution are not advertised until strict implementations
pass the same real backend contract. Every installed Octa
release includes `octa-runner-capabilities.json`, generated alongside the
runner with `octa-runner capabilities`. Inventory reads this bounded manifest
instead of trying to execute a guest Linux binary on the host; the runner still
proves its protocol versions with `Hello` after the selected backend starts it.

The implementation sequence and security boundaries are documented in
[`docs/agent-implementation-plan.md`](docs/agent-implementation-plan.md).
Release archives, provenance verification, enrollment, service installation,
runtime-specific privileges, rotation, drain, upgrade, recovery, and removal
are documented in [`docs/operations.md`](docs/operations.md). Packaged services
run as dedicated non-root identities; Native cgroup access, containerd control,
and Microsandbox virtualization access are separate explicit operator choices.
Dependency auditing and protocol-fuzzing ownership are documented in
[`docs/security-testing.md`](docs/security-testing.md).
Language-neutral wire specifications are indexed in
[`docs/protocols/README.md`](docs/protocols/README.md).
Canonical coordination terminology is defined in
[`CONTEXT.md`](CONTEXT.md), while server crate ownership, dependency direction,
and public seams are mapped in
[`docs/server-architecture.md`](docs/server-architecture.md).
The implemented coordinator transport is specified in
[`docs/protocols/server-agent-v1.md`](docs/protocols/server-agent-v1.md).
The source-plugin process model and complete protocol v1 lifecycle are
documented in
[`agent/octacity-source-plugin/README.md`](agent/octacity-source-plugin/README.md).
An operator-facing configuration shape is available in
[`docs/agent.example.toml`](docs/agent.example.toml).
Provisioning and exact commands for the non-emulated execution contract suite
are documented in
[`docs/backend-contract-tests.md`](docs/backend-contract-tests.md).
