# Real execution-backend contract tests

The portable workspace suite tests orchestration with an in-memory backend.
The retained backend contract suite additionally runs a real signed job, real
`octa-runner`, and the same Octafile through direct Host execution, legacy
Native, containerd isolation, Apple VF isolation, and Microsandbox
virtualization. These tests are
`ignored` because they require an installed release and, except for Host,
privileged runtime provisioning. They never skip after an operator explicitly
selects one: absent or invalid provisioning fails the test.

The `host` suite runs on ordinary GitHub-hosted Linux, Apple Silicon macOS, and
Windows machines. It verifies the downloaded Octa release and packaged Agent,
runs the real Host backend contract, and then starts that released Agent
against a narrow coordinator fixture. The fixture negotiates v2, checks exact
host/target identity and zero isolation guarantees, completes one job, cancels
a second running job, loses one event acknowledgement after durable receipt,
verifies idempotent replay without duplicate events or completion, drains the
Agent, and retains bounded event, resource, and cleanup evidence.

The manual privileged `backend contracts` suites assign each exact test
to a runner with the required kernel or hypervisor. Linux Native runs on a
fresh GitHub-hosted `ubuntu-24.04` VM; the job downloads and verifies the
pinned Octa release, delegates an isolated cgroup-v2 subtree, and mounts
separate bounded loopback filesystems for workspaces and cache. Linux
containerd and Linux Microsandbox also run on fresh `ubuntu-24.04` machines:
the former downloads a checksum-pinned containerd 2.3.6 LTS bundle and starts
its daemon privately as root with the `overlayfs` snapshotter, while the latter
requires the hosted VM to expose KVM. GitHub does not guarantee nested
virtualization, so the Microsandbox job is an explicit fail-closed release
gate rather than a portable CI prerequisite. Apple VF isolation and Apple
Silicon Microsandbox remain explicit self-hosted suites. Standard
GitHub-hosted ARM64 macOS runners cannot provide the required nested
Virtualization.framework boundary, so neither suite is treated as portable CI
evidence. The portable CI matrix remains independent of privileged host
configuration.

### Release-matrix cadence and runner evidence

The workflow uses the following fixed cadence:

| Invocation | Required evidence |
| --- | --- |
| Pull request | The portable workspace checks in `ci.yml`; no released backend job |
| Push to `main` | Released Host on GitHub-hosted Linux, macOS, and Windows |
| Nightly schedule | Every GitHub-hosted release job: Host, the three Linux providers, failure, security, lifecycle, and performance |
| Manual `hosted` | The same GitHub-hosted set as the nightly schedule |
| Manual exact suite | Only the selected backend or hosted contract catalog |
| Manual `release-qualified` | The complete release gate, including both self-hosted macOS providers |
| Manual `all` | `release-qualified` plus the Vault and artifact service contract |
| Release workflow or `v*` tag | The complete release-qualified gate before packaging |

The manual `lifecycle` suite is intentionally a complete lifecycle gate and
therefore includes both self-hosted macOS providers. The Windows WHP
Microsandbox preview remains available only through its exact manual suite; it
is excluded from `all` and is not accepted as release evidence.

### Vault and artifact service contract

The opt-in `vault-artifact` suite crosses service boundaries that unit fakes
cannot validate together. A signed lease runs through the production
`JobLifecycle`, `JobExecutor`, and real Octa runner. An operator-owned JWT is copied into a
private workload-identity lease, used to authenticate to Vault KV v2, and
revoked after runtime destruction. Before completion, the contract scans
events, results, the durable spool, workspace, and upload metadata to prove
that neither the JWT nor the resolved Vault secret escaped redaction.

The same job freezes an artifact and report, obtains fenced presigned upload
targets, and publishes them to MinIO without exposing S3 credentials to the
Agent. The portable host adapter intentionally does not claim to enforce its
documented Vault-only network allowlist; the Microsandbox contract separately
proves the fixed read-only identity mount and restricted egress boundary. Its
in-process coordinator validates the HTTP and idempotency contract but does not
claim PostgreSQL durability, which is covered by the authoritative-store
contracts.

Run the complete workflow from `backend contracts` with suite
`vault-artifact`. To reproduce it locally, build `octa-runner`, `octa_plugin_shell`, and
`octa_plugin_tpl`, then start the pinned services:

```shell
docker run --detach --rm --name octacity-vault-contract --cap-add=IPC_LOCK \
  -p 127.0.0.1:18200:8200 \
  -e VAULT_DEV_ROOT_TOKEN_ID=octacity-root \
  -e VAULT_DEV_LISTEN_ADDRESS=0.0.0.0:8200 \
  hashicorp/vault:1.20.4@sha256:268bb80aa9c6d13d65fcfa05c0c268caca068952240a8087291a6ce0b66e3a10

docker run --detach --rm --name octacity-minio-contract \
  -p 127.0.0.1:19000:9000 \
  -e MINIO_ROOT_USER=octacity \
  -e MINIO_ROOT_PASSWORD=octacity-secret \
  docker.io/bitnamilegacy/minio:2025.7.23@sha256:8935e75fa5d11295c17171e4aa49efe390a1193cd7f12e4d21b92af9ffef09d7 \
  server /bitnami/minio/data
```

Run the lifecycle contract against those services:

```shell
OCTACITY_MINIO_ENDPOINT=http://127.0.0.1:19000 \
OCTACITY_MINIO_ACCESS_KEY=octacity \
OCTACITY_MINIO_SECRET_KEY=octacity-secret \
OCTACITY_VAULT_ENDPOINT=http://127.0.0.1:18200 \
OCTACITY_VAULT_ROOT_TOKEN=octacity-root \
OCTACITY_SERVICE_CONTRACT_OCTA_RUNNER=/absolute/path/to/octa/target/debug/octa-runner \
OCTACITY_SERVICE_CONTRACT_OCTA_PLUGINS_DIR=/absolute/path/to/octa/target/debug \
cargo test -p octacity-service-contract-tests --test vault_artifact \
  real_octa_vault_job_publishes_outputs_without_leaking_secrets \
  -- --ignored --exact --nocapture
```

The separate `s3_store_satisfies_the_minio_contract` test verifies checksums,
idempotent publication, expiring capabilities, deletion, storage outage, and
recovery against the same MinIO service. It is part of the failure matrix, so
an object-store contract cannot silently disappear from retained release
evidence.

Nightly and release invocations also require the hosted `failure-matrix` job.
It starts disposable pinned PostgreSQL and MinIO services and executes the
production fault contracts for Agent and server restart behavior, database
rollback and recovery, S3 outage and corrupt content, log-index retry and
rebuild, VCS and webhook crashes, secret-provider rejection, telemetry-exporter
loss, backend and guest failure, and network replay. The catalog in
`tools/failure_matrix.py` is validated before execution: all lifecycle phases
and the fencing, replay, index-rebuild, outbox-recovery, bounded-retry,
teardown, and no-false-success invariants must remain covered. Every command is
run even after an earlier failure, and the gate retains per-contract logs plus
`failure-matrix.json`; any failed command makes the release evidence fail.

Run this hosted gate alone from `backend contracts` with suite `failure`. For a
local run, provide the same `OCTACITY_POSTGRES_URL` and `OCTACITY_MINIO_*`
variables used by CI and execute:

```shell
python3 tools/failure_matrix.py run --evidence-dir /tmp/octacity-failure-matrix
```

### Replica contention and rolling restart

The PostgreSQL portion of the nightly failure matrix runs every ignored
`octacity-server-store-postgres` integration test against a disposable database.
Replica-safety coverage uses independent connection pools and reconstructs store
adapters between claim attempts, so passing does not depend on a shared Rust
mutex, process timer, or notification receiver.

| Durable responsibility | Executable PostgreSQL contract |
| --- | --- |
| Schedule cursor claim | `concurrent_schema_primitives_have_one_visible_winner` |
| Trigger acceptance | `concurrent_servers_accept_one_trigger_occurrence_once` |
| Trigger retry claim | `trigger_retry_claims_are_exclusive_and_survive_rolling_restart` |
| Orchestrator DAG transition | `concurrent_event_and_completion_replays_have_one_dag_transition` and `concurrent_fan_in_completions_cannot_leave_a_satisfied_child_blocked` |
| Transactional outbox | `terminal_build_outbox_is_claimed_once_and_restart_safe` |
| Ready queue | `concurrent_agents_lease_one_ready_job_once` |
| Lease expiry | `replicas_never_own_the_same_expired_lease_concurrently` |
| Build Result retention | `retention_deadlines_and_interrupted_cleanup_are_durable` |
| Hold versus expiration | `hold_and_first_visibility_transition_serialize_as_one_decision` and `time_bounded_hold_expires_at_the_boundary_and_cannot_revive_hidden_data` |

Each abandoned worker claim remains unavailable until its persisted deadline.
At the exact deadline, at most one replacement replica acquires it; the previous
owner is fenced from completion. Replayed Trigger, queue, and Orchestrator work
observes the committed result after adapter replacement rather than relying on
the process that first handled it.

### Schema migration and rollback rehearsal

The same disposable PostgreSQL gate runs the migration contract in
`tests/migrations.rs`. It creates a database at the declared previous-binary
schema boundary, seeds durable data, and captures a PostgreSQL template
snapshot. Separate restores prove the forward migration, atomic failure and
retry path, and previous-binary rollback path. Additional mutations of the
migration ledger prove that a future version, checksum mismatch, or history gap
keeps readiness incompatible. The operational rule and local command are in
[`schema-migrations.md`](../operations/schema-migrations.md).

### Coordinated backup and restore rehearsal

Restore qualification combines the PostgreSQL restore-inventory contracts,
real MinIO digest verification, and Build-log projection rebuild contracts.
The offline `octacity-server reconcile-restore` command pages through every
visible Artifact, report, committed log chunk, and published remote-cache blob;
it fails closed if any immutable object is missing or corrupt. Projection
rebuild keeps the committed watermark authoritative while the indexed watermark
starts behind it, so search remains honestly stale until durable replay catches
up. The capture, protected recovery set, restore ordering, and evidence rules
are documented in
[`backup-restore.md`](../operations/backup-restore.md).

Nightly and release invocations also require the hosted `security-matrix` job.
Its trust boundaries are every place where an untrusted caller, repository,
provider, plugin, workload, or stored object can influence authority or expose
sensitive data. The protected assets are Agent credentials and Pool authority,
immutable policy and signed execution intent, provider credentials, webhook
authenticity, logical secret references, verified executable and object
digests, Agent-owned filesystem roots, redacted logs, transfer capabilities,
and project-local cache data.

The catalog in `tools/security_matrix.py` maps those assets to executable
negative contracts for Agent enrollment, Pool and execution-mode admission,
Project policy, signed JobSpecs, provider configuration, webhook
authentication, malicious repository data, plugin digests, artifact paths,
log archive and search redaction, presigned URL redaction, and cache namespace
isolation. It refuses an incomplete catalog and requires authentication,
authorization, integrity, isolation, non-execution, redaction, and fail-closed
evidence. Every group runs even after an earlier failure. The retained
per-group logs and `security-matrix.json` therefore distinguish a complete
passing gate from missing or partial evidence.

Run this hosted gate alone from `backend contracts` with suite `security`. For
a local run, provide a disposable PostgreSQL database and execute:

```shell
export OCTACITY_POSTGRES_URL=postgres://octacity:octacity-secret@127.0.0.1:15432/postgres
python3 tools/security_matrix.py run --evidence-dir /tmp/octacity-security-matrix
```

Nightly and release invocations also require the `lifecycle-matrix` job. Its
catalog names only release-qualified platform/provider pairs: systemd with
Host, legacy Native, containerd, or Microsandbox; launchd with Host, Apple VF,
or Microsandbox; and Windows SCM with Host. The Windows WHP Microsandbox
preview is deliberately excluded until it becomes release-qualified.

The hosted portion verifies packaged service definitions, dedicated service
identities, configuration validation, restart and orphan recovery, graceful
and forced server-directed drain, provider teardown, and immutable release
installation. Package contracts build consecutive Linux, macOS, and Windows
candidates, install them into separate version directories, revalidate every
checksum, and prove the previous candidate remains intact for rollback. The
catalog covers every install, validate, start, reboot, drain, upgrade,
rollback-window, and uninstall phase both without a Pipeline and with an active
Pipeline where work can exist.

Lifecycle evidence is intentionally composite. A hosted unit or package test
cannot claim that a privileged backend completed a released job. The nightly
gate therefore combines the lifecycle catalog with `released-host` and all
three GitHub-hosted Linux providers. The manual `lifecycle` suite and release
gate additionally require `macos-apple-vf` and `macos-microsandbox`. Those jobs
install verified release bundles, run active Pipelines, exercise cancellation
or graceful drain, and reject leftover provider state. `backend-evidence`
accepts the selected cadence only when every job required by that cadence
succeeds.

Run the portable catalog locally with:

```shell
python3 tools/lifecycle_matrix.py run --evidence-dir /tmp/octacity-lifecycle-matrix
```

Run the complete released-host and provider gate from `backend contracts` with
suite `lifecycle`; the two Apple Silicon jobs require the documented ephemeral
self-hosted runners.

### Performance budgets and raw evidence

The nightly and release Linux Native slice enables the performance gate. A
manual run can select suite `performance`; it uses the same GitHub-hosted real
Native backend and does not require a self-hosted runner. The slice records raw
operation samples in `performance-measurements.json`, and
`tools/performance_matrix.py` evaluates them against the versioned limits in
`tools/performance_budgets.json`. The retained `performance-report.json`
contains the calculated p95, minimum, or maximum and the result of every
budget. A missing metric, wrong unit, insufficient sample count, malformed
number, or exceeded limit fails the job.

The measurements cover manual Trigger acceptance and replay, an internal DAG
transition, queue-to-lease-preparation latency, repeated management REST reads,
durable event throughput, log-index catch-up and both search modes, lifecycle
preparing-to-running and cleaning-to-completing backend boundaries, artifact
download throughput, remote L2 cache restore, 32 simultaneous PostgreSQL
queries through a four-connection pool, and 100 concurrent authenticated idle
Agent registrations followed by five no-work poll cycles per Agent. The soak
uses real enrollment and registration credentials against the released server,
but does not start 100 runner processes or consume jobs.

Validate the budget document without provisioning a backend:

```shell
python3 tools/performance_matrix.py validate
```

Evaluate retained raw evidence locally with:

```shell
python3 tools/performance_matrix.py evaluate \
  --measurements /path/to/performance-measurements.json \
  --report /tmp/performance-report.json
```

Budgets are absolute release safety ceilings, not a moving comparison with the
previous run. Change them only in review together with evidence explaining the
new product expectation; never relax a limit automatically after a slow run.

Manual suites that select a self-hosted backend and release invocations inspect
the canonical `octacity-release` runner group before GitHub creates a
self-hosted job. Nightly runs never inspect or require that group. The group
must allow this repository, public repositories, and every trusted workflow in
this repository; do not restrict it to a branch-pinned workflow because release
jobs execute the same reusable workflow from a tag. Jobs also name the group in
`runs-on`, so an unrelated organization runner with matching labels cannot
satisfy the preflight and then leave the job queued.

Configure the repository Actions secret `OCTACITY_RUNNER_INVENTORY_TOKEN` with
a fine-grained token limited to the OctaHive organization and the
`Self-hosted runners: read` organization permission. The workflow follows every
GitHub API page and never uses this token on a self-hosted machine. If the token
is absent or invalid, the group is not visible to the repository, or a required
labelled runner is offline at planning time, the corresponding job is skipped
and the hosted `backend-evidence` job fails with explicit missing-evidence
diagnostics rather than reporting a successful release gate.

Ubuntu 24.04 restricts unprivileged user namespaces through AppArmor. The
hosted setup loads Ubuntu's packaged `bwrap-userns-restrict` profile rather
than disabling that system-wide protection, then executes a namespace probe
from the delegated runner cgroup before starting a long Rust build or test.

Each Native test command enters a dedicated sibling `runner` cgroup before it
starts the Agent. The Agent can therefore move only its runner descendants into
the clean `jobs` subtree without receiving authority over the VM's root cgroup.

Select `linux-native`, `linux-containerd`, or `linux-microsandbox` to run one
released-product Linux matrix without owning a runner. The workflow creates
all privileged Linux state on the disposable VM and removes it in an
`always()` cleanup step; the VM is discarded after the job as an additional
boundary.

The Apple VF and macOS Microsandbox slices are selected independently. To run
one of them on a single Apple Silicon macOS host, select the backend through
`OCTACITY_RUNNER_MACOS_BACKEND` and execute
`tools/runner/register-backend-runner.sh` in a separate terminal. The wizard
checks the selected backend prerequisites, opens the repository registration
page, registers an ephemeral one-job runner, and opens the exact workflow
suite for that backend. Apple VF's disposable filesystems and strict contract
are provisioned and exercised inside the job. The
one-hour GitHub registration token is read without echo and is never written
to disk. Because self-hosted runners execute repository code, use this
procedure only for a trusted revision and do not enable it for unreviewed
public pull requests.

A manual `release-qualified`, `all`, or `lifecycle` run and every release gate
require both macOS labels to be online before the workflow starts. Provision
two ephemeral runner installations, normally on two hosts. A single
sufficiently provisioned host may use two terminals and distinct
`OCTACITY_RUNNER_ROOT` directories; never reuse one configured `actions-runner`
directory for both registrations. Individual macOS suites require only their
selected backend runner, and nightly runs require neither.

An optional self-hosted ARM64 Linux runner still needs one initial cgroup
installation:

```shell
sudo tools/runner/install-linux-native-cgroup.sh "$USER"
```

This installs a boot-persistent cgroup-v2 tree with sibling `runner` and
`jobs` leaves. The wizard starts the Actions runner through the installed
`octacity-in-cgroup` wrapper, while the Native backend receives the clean
`jobs` subtree. The dedicated Native work and cache filesystems must also be
mounted before the wizard runs. `tools/runner/preflight-backend-runner.sh`
fails closed if any runtime, release checksum, Docker daemon, cgroup
controller, or real backend contract is unavailable.

Every released Linux backend and both Apple Silicon macOS provider jobs
build deterministic server and Agent release-candidate archives and use
`octacity-release-harness` before every release scenario. The harness accepts
only self-verifying extracted bundles, validates their exact versioned release
contracts, checksum inventories, component digests and protocol ranges, and
copies the server, Agent and pre-provisioned Octa bundle into a fresh isolated
installation. It then verifies the copied manifests and bytes again. The
scenario receives only those installed paths; it starts the packaged server
and Agent rather than product binaries below Cargo's `target` directory.

`release_vertical_slice` starts the released REST server over PostgreSQL and
MinIO and never substitutes a workspace product binary. The Linux Native gate
exercises a manual two-node DAG and an automatically derived downstream Build,
then drains the first Agent. A second Agent with an empty local cache executes
a scheduled occurrence through remote-cache reuse, a deterministically failed
Build followed by an idempotently replayed REST retry, and a separately
cancelled Build. The retry evidence retains both typed Attempts and their
lineage. The gate reads Build Results, performs full-text and literal log
searches, downloads and hashes an artifact, and checks ordered lifecycle and
resource events, causal linkage, duplicate suppression, metrics, the active
cgroup's CPU, memory, swap, and process limits, cgroup cleanup, and workspace
cleanup. A private loopback TLS proxy fronts the
server's cache ingress so the released runner uses the production HTTPS and CA
contract. The workflow retains its installation receipt, release manifests,
server and Agent logs, REST evidence, and Prometheus snapshot.

The containerd, Apple VF, and Linux/macOS Microsandbox jobs first run the strict backend
contract, which verifies their digest-pinned image, filesystem boundary,
resource accounting, cancellation, and orphan cleanup, and then run the
packaged Agent against the real released server vertical slice. Their final
cleanup assertion rejects remaining job workspaces and provider-owned runtime
state. Apple VF still advertises `isolation`: its per-workload VM is an
implementation detail, while the signed job requests the same four guarantees
as containerd. Both qualified Microsandbox hosts run the full released-product
matrix, including remote cache reuse and artifact-integrity checks. Direct Host execution
is independently qualified on released Linux, macOS, and Windows Agents and is
never reported as Native, isolation, or virtualization.

The hosted OCI fixture selects an architecture-specific immutable manifest
digest for `linux/amd64` or `linux/arm64`. The adapter still reads the embedded
configuration and rejects any OS or architecture that does not match the
declared execution target.

### OCI release-gate threat boundaries

Repository input may select only the signed runtime class and the
digest-pinned image recorded in the immutable Build snapshot. It cannot select
the containerd socket, namespace, snapshotter, runtime, registry configuration,
Apple `container` executable, Microsandbox executable, firmware, or host paths;
those remain operator-owned Agent configuration. The disposable gate verifies the downloaded
Microsandbox and containerd bundles by hard-coded SHA-256 values before
execution. It also requires containerd's Transfer plugin to be healthy before
running any contract. Containerd then verifies the resolved image descriptor
against the signed digest.

The privileged host runtime is an explicit trust boundary. Containerd runs in
a private namespace and receives no registry credential through JobSpec or a
process argument. Job mounts remain limited to the verified read-only Octa
release and the Agent-owned workspace, cache, and identity paths. Cancellation
must remove the task or VM and its workspace. Cleanup accepts only a marked,
non-symbolic direct child of `RUNNER_TEMP`; the root-owned containerd PID file
must still identify the staged containerd executable before it is signalled.
These checks address image substitution, host-path escape, credential leakage,
resource-policy bypass, orphaned execution, and deletion of an unrelated host
path. Apple VF uses `--network none`, a read-only image root, no Linux
capabilities, fixed bind targets, quota-backed APFS work/cache images, and
Agent-private ownership markers. Startup and teardown never enumerate or
delete an unmarked container belonging to another user or Agent.

All tests require:

```shell
export OCTACITY_CONTRACT_OCTA_RELEASE_ROOT=/opt/octacity/octa
export OCTACITY_CONTRACT_WORKSPACE_BYTES=1073741824
```

The release root must have the layout accepted by `RunnerInstallation`: a
Linux `octa-runner`, the matching `octa-runner-capabilities.json`, `Octa.lock`,
and the complete set of exact locked plugin executables in `plugins/`. A
partial release is invalid even when the fixture itself uses only the shell
plugin. Generate the manifest
on the release build platform with:

```shell
./octa-runner capabilities > octa-runner-capabilities.json
```

The workspace limit must be a positive whole MiB for Microsandbox. On an Apple
Silicon macOS host, provision the `linux-aarch64` release because the guest is
Linux; the manifest lets the macOS agent inventory that release without trying
to execute its ELF runner on the host. The adapter places the OCI root overlay
in guest RAM and applies the signed disk budget to the writable workspace bind
mount, so writes elsewhere in the image cannot escape both resource limits.

## Host

Host mode invokes the verified platform-native runner directly as the Agent
service identity. It has dedicated work roots, owned-lifecycle process-group
cancellation, bounded runner/event transport, and advisory resource accounting, but
it deliberately claims no filesystem, process, network, resource, container,
or guest isolation. Use it only for trusted repositories with an exact Host
execution target in both Project policy and the Pool `execution_allowlist`.
Host jobs require an unrestricted-network policy because the
backend cannot honestly enforce disabled or allowlisted egress.
Host also rejects workload identity because a direct host path cannot project
the credential with an isolation boundary. Workspace accounting uses one
non-overlapping, entry-bounded scan per execution; it is diagnostic rather than
a disk-enforcement boundary. An abrupt Agent death can leave descendants, so
Host must run under a dedicated/disposable service boundary where that risk is
material.

```shell
export OCTACITY_CONTRACT_HOST_WORK_ROOT=/absolute/private/host-work
export OCTACITY_CONTRACT_HOST_ENVIRONMENT_ID=host-toolchain-v1
export OCTACITY_CONTRACT_WORKSPACE_BYTES=33554432
cargo test -p octacity-job --test backend_contract \
  host_backend_satisfies_the_real_runner_contract -- --ignored --exact --nocapture
```

The portable released-Agent contract additionally requires
`OCTACITY_RELEASE_AGENT_ROOT` and `OCTACITY_CONTRACT_HOST_SOURCE_ROOT`. The
source root is the already verified pinned Octa checkout; the Host gate uses a
separately checksummed contract candidate whose Git plugin permits that exact
local source, so execution qualification never depends on external DNS or
network availability. Production release candidates retain `allow_file =
false`. The `host` workflow stages both release roots and runs this test on all
three supported operating systems:

```shell
cargo test -p octacity-release-harness --test released_host_agent \
  released_host_agent_satisfies_the_portable_execution_contract \
  -- --ignored --exact --nocapture
```

## Native

Provision a cgroup v2 directory delegated to the test user with the `cpu`,
`memory`, `io`, and `pids` controllers enabled. Mount a dedicated empty
filesystem whose total capacity does not exceed the configured workspace
limit. Then run:

```shell
export OCTACITY_CONTRACT_NATIVE_CGROUP_ROOT=/sys/fs/cgroup/octacity/jobs
export OCTACITY_CONTRACT_NATIVE_WORK_ROOT=/var/lib/octacity-contract/native
export OCTACITY_RELEASE_NATIVE_CACHE_ROOT=/var/lib/octacity-contract/native-cache
export OCTACITY_CONTRACT_NATIVE_BWRAP=/usr/bin/bwrap
export OCTACITY_CONTRACT_NATIVE_PATH=/usr/local/bin:/usr/bin:/bin
cargo test -p octacity-job --test backend_contract \
  native_backend_satisfies_the_real_runner_contract -- --ignored --exact --nocapture
```

`OCTACITY_RELEASE_NATIVE_CACHE_ROOT` must be a separate dedicated filesystem
mount whose capacity does not exceed `OCTACITY_CONTRACT_WORKSPACE_BYTES`.
The release gate removes Agent A's owned L1 entries after it exits while
preserving the mounted filesystem and its original entries. Agent A publishes
the fixture result to the server-backed L2; Agent B starts with an empty L1 and
must observe a cache hit, so a local hit cannot masquerade as remote reuse.

## Microsandbox

Use a Linux (`x86_64` or `aarch64`) or Apple Silicon macOS host supported by the
pinned Microsandbox SDK. The Windows `x86_64`/WHP route is a preview. Its
`windows-microsandbox-preview` self-hosted gate records prerequisite backend
evidence, but passing that gate does not promote Windows into the
release-qualified matrix; promotion also requires a released-product matrix
with equivalent cache and artifact evidence. Intel macOS is unsupported. The image must be an
immutable OCI digest containing the runtime libraries needed by the installed
Linux Octa release. The work and state roots must be separate, operator-owned
directories. This contract also provisions a per-job identity, requires it at
the fixed read-only `/run/octa-identity` guest path, and probes one allowed and
one denied HTTPS host through a restricted network policy. It therefore
exercises actual Microsandbox identity mounting and egress enforcement rather
than only checking the adapter's SDK request. The guest image must contain
`curl`. The allowed endpoint must answer HTTPS; the denied endpoint should
answer without the sandbox policy so the negative assertion distinguishes
isolation from an unrelated outage.

```shell
export OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT=/absolute/path/to/micro-work
export OCTACITY_CONTRACT_MICROSANDBOX_STATE_ROOT=/absolute/path/to/micro-state
export OCTACITY_CONTRACT_MICROSANDBOX_EXECUTABLE=/opt/microsandbox/bin/msb
export OCTACITY_CONTRACT_MICROSANDBOX_LIBKRUNFW=/opt/microsandbox/lib/libkrunfw.so
export OCTACITY_CONTRACT_MICROSANDBOX_ENVIRONMENT_IDENTITY=microsandbox-0.6.18-linux-amd64
export OCTACITY_CONTRACT_MICROSANDBOX_IMAGE='registry.example/build@sha256:<64-lowercase-hex>'
export OCTACITY_CONTRACT_MICROSANDBOX_ALLOWED_HOST=allowed.contract.example
export OCTACITY_CONTRACT_MICROSANDBOX_DENIED_HOST=denied.contract.example
cargo test -p octacity-job --test backend_contract \
  microsandbox_backend_satisfies_the_real_runner_contract -- --ignored --exact --nocapture
```

The Windows preview workflow downloads the pinned `0.6.18` x86_64 ZIP, verifies
its hard-coded SHA-256, requires `msb doctor` to accept WHP, and runs only on a
self-hosted runner labelled `Windows`, `X64`, and
`octacity-microsandbox-whp`. Merely compiling the adapter or seeing upstream
preview support never promotes Windows into the release-qualified matrix.

The released-product vertical slice additionally requires Docker so its
composite action can start disposable pinned PostgreSQL and MinIO containers.
For GitHub-hosted Native, the workflow itself supplies the backend variables
and the downloaded, checksummed `OCTACITY_CONTRACT_OCTA_RELEASE_ROOT`. A
self-hosted runner supplies the corresponding values through its service
environment. The workflow packages the matching server and Agent, installs all
three roots through the harness, and supplies only the isolated paths, real
dependency endpoints, selected backend, and evidence directory to the
scenario.

## Containerd process isolation

Use a Linux host with containerd, a cgroup-v2-compatible runc or crun runtime,
and the configured snapshotter. The engine pulls and unpacks the exact signed
digest through containerd's Transfer API. Private registries use an optional,
operator-owned containerd `hosts.toml` directory; credentials are never fields
of JobSpec or command-line arguments. As with Native, `work_root` must be a
dedicated filesystem whose total capacity does not exceed the configured job
limit.

```shell
export OCTACITY_CONTRACT_CONTAINERD_ENDPOINT=/run/containerd/containerd.sock
export OCTACITY_CONTRACT_CONTAINERD_NAMESPACE=octacity
export OCTACITY_CONTRACT_CONTAINERD_SNAPSHOTTER=overlayfs
export OCTACITY_CONTRACT_CONTAINERD_RUNTIME=io.containerd.runc.v2
export OCTACITY_CONTRACT_CONTAINERD_ENVIRONMENT_IDENTITY=containerd-release-v1
export OCTACITY_CONTRACT_CONTAINERD_WORK_ROOT=/var/lib/octacity-contract/containerd-work
export OCTACITY_CONTRACT_CONTAINERD_CACHE_ROOT=/var/lib/octacity-contract/containerd-cache
export OCTACITY_CONTRACT_CONTAINERD_STATE_ROOT=/var/lib/octacity-contract/containerd-state
export OCTACITY_CONTRACT_CONTAINERD_IMAGE='registry.example/build@sha256:<64-lowercase-hex>'
# Optional: export OCTACITY_CONTRACT_CONTAINERD_REGISTRY_CONFIG_DIR=/etc/containerd/certs.d
cargo test -p octacity-job --test backend_contract \
  containerd_isolation_provider_satisfies_the_real_runner_contract -- --ignored --exact --nocapture
```

## Apple Virtualization.framework-backed isolation

Use Apple Silicon macOS 26 or newer with Apple `container` 0.6 or newer and
its API service running. The provider executes the digest-pinned Linux ARM64
image in the runtime's per-container Virtualization.framework VM but registers
mode `isolation`, not `virtualization`. Both `work_root` and `cache.root` must
be separate bounded filesystems; the supplied self-hosted setup creates
disposable APFS sparse images for those boundaries.

```shell
export OCTACITY_CONTRACT_APPLE_VF_EXECUTABLE=/usr/local/bin/container
export OCTACITY_CONTRACT_APPLE_VF_ENVIRONMENT_IDENTITY=apple-vf-release-v1
export OCTACITY_CONTRACT_APPLE_VF_WORK_ROOT=/var/lib/octacity-contract/apple-vf-work
export OCTACITY_CONTRACT_APPLE_VF_CACHE_ROOT=/var/lib/octacity-contract/apple-vf-cache
export OCTACITY_CONTRACT_APPLE_VF_STATE_ROOT=/var/lib/octacity-contract/apple-vf-state
export OCTACITY_CONTRACT_APPLE_VF_IMAGE='registry.example/build@sha256:<64-lowercase-hex>'
cargo test -p octacity-job --test backend_contract \
  apple_vf_isolation_provider_satisfies_the_real_runner_contract -- --ignored --exact --nocapture
```

To qualify Apple VF independently, select `macos-apple-vf`. A complete release
gate selects it automatically and requires its runner to be online. The job runs
`tools/runner/self-hosted-apple-vf.sh`, the strict backend contract, and the
released server/Agent vertical slice, then retains evidence and checks that no
workspace or owned container marker remains. A green portable macOS job is not
a substitute for this real-machine gate.

Each test verifies signed runtime and isolation selection, real bidirectional
runner JSONL, structured events, terminal resource accounting, graceful
cancellation of a second long-running job, workspace removal after both jobs,
and a final orphan-cleanup pass. Release vertical slices retain structured raw
performance samples instead of treating a complete test's wall-clock duration
as operation latency. The Microsandbox variant additionally verifies the fixed
workload-identity mount and allows/denies real network probes according to its
restricted egress policy; the service-backed Vault and artifact contract proves
the actual Vault login and secret-redaction path.

## Including a real backend in Linux coverage

Portable coverage deliberately leaves the privileged backend tests ignored.
On a provisioned Linux worker, accumulate the normal suite and the selected
strict backend contract into one report instead of excluding backend code:

```shell
cargo llvm-cov clean --workspace
cargo llvm-cov --workspace --all-features --no-report
cargo llvm-cov --no-clean --all-features -p octacity-job --test backend_contract \
  -- containerd_isolation_provider_satisfies_the_real_runner_contract \
  --ignored --exact --nocapture
cargo llvm-cov report --summary-only \
  --ignore-filename-regex '[/\\]\.cargo[/\\]registry[/\\]|octa-runner-protocol[/\\]src[/\\]lib\.rs$'
```

Use the corresponding exact Native, Apple VF, or Microsandbox test name on workers for
those backends. This keeps production adapters in the coverage denominator and
measures them with their actual kernel/runtime boundary rather than mocks.
