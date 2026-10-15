# Real execution-backend contract tests

The portable workspace suite tests orchestration with an in-memory backend.
The retained backend contract suite additionally runs a real signed job, real
`octa-runner`, and the same Octafile through direct Host execution, legacy
Native, containerd isolation, Apple VF isolation, and Microsandbox
virtualization. These tests are
`ignored` because they require an installed release and, except for Host,
privileged runtime provisioning. They never skip after an operator explicitly
selects one: absent or invalid provisioning fails the test.

The release-qualified Native slice also configures the deterministic Codex CLI
fixture as an operator-selected executable. Before any source is materialized,
startup rejects missing, mutable, wrong-digest, incompatible-platform, or
release-incompatible tools. The successful, overflow, and cancellation cases
then prove identical opaque read-only projection, explicit environment
mapping, bounded/redacted trace/result/provenance and complete event evidence,
no partial output on failure, descendant cleanup, and unchanged readiness for
ordinary CI/CD Builds. JobSpec v1/v2 do not select tools individually, so this
evidence uses a dedicated Agent/Pool profile without model credentials.

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
the former downloads a checksum-pinned containerd 2.4.1 LTS bundle and starts
its daemon privately as root with the `overlayfs` snapshotter, while the latter
requires the hosted VM to expose KVM. GitHub does not guarantee nested
virtualization, so the Microsandbox job is an explicit fail-closed release
gate rather than a portable CI prerequisite. Apple VF isolation and Apple
Silicon Microsandbox remain explicit self-hosted suites. Standard
GitHub-hosted ARM64 macOS runners cannot provide the required nested
Virtualization.framework boundary, so neither suite is treated as portable CI
evidence. The portable CI matrix remains independent of privileged host
configuration.

The containerd and Microsandbox contracts also execute one JobSpec v3 Factory
Job after the ordinary v2 lifecycle. A loopback transfer fixture serves the
exact server-owned Octafile; the Agent verifies and seals it before source
materialization. The guest then proves the protected input and workload
identity are read-only, source/scratch/output are independently writable, the
`/workspace` parent cannot acquire an undeclared sibling, no cache secret is
implicitly present, and the configured disabled or exact-host network policy
is effective. The test retains normal CPU, memory, disk, process, elapsed-time,
event, output, and teardown accounting. Positive and negative stage-secret
delivery plus per-kind bounded-output contracts are still required before this
backend slice is release-qualified for the complete Factory permission
vocabulary. Apple VF continues to run only the v2
contract until it can enforce restricted host allowlists, so backend names do
not stand in for semantic qualification.

### Private controlled-reader transport

The ignored `octacity-execution-microsandbox` test
`private_channel_reads_an_unmounted_source_without_exposing_it_to_the_guest`
starts the pinned real VM with `source_visible=false` and networking disabled.
A test-only Linux ARM64 client refuses readable `/workspace/source/input`, then
connects to the configured host Unix listener through vsock CID 2, port 5001.
The host listener reads that unmounted file and returns its bytes. The test
destroys the execution before asserting the response. Its host socket lives in
a separate private control directory outside the Job tree and projected roots.

Compile the retained probe on an Apple Silicon host using Clang and the Rust
toolchain's bundled ELF linker:

```shell
clang --target=aarch64-unknown-linux-gnu -O2 -ffreestanding -fno-builtin \
  -fno-stack-protector -c \
  agent/octacity-execution-microsandbox/tests/fixtures/vsock_probe_aarch64.c \
  -o /tmp/octacity-vsock-probe.o
"$(rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin/rust-lld" \
  -flavor gnu -m aarch64linux -static -e _start \
  /tmp/octacity-vsock-probe.o -o /tmp/octacity-vsock-probe
```

Provide the same `OCTACITY_CONTRACT_MICROSANDBOX_*` installation, image, state,
and work variables used by the real backend contract, plus
`OCTACITY_CONTRACT_VSOCK_PROBE=/tmp/octacity-vsock-probe`, then run:

```shell
cargo test -p octacity-execution-microsandbox --test private_tool_channel \
  private_channel_reads_an_unmounted_source_without_exposing_it_to_the_guest \
  -- --ignored --exact --nocapture
```

This is transport and mount-isolation evidence. The listener does not authorize
tools, and the probe is not a packaged Agent helper. It does not qualify the
ToolRisk broker, installed executable identity, helper CPU/memory/process
limits, response accounting, or cancellation and teardown of a production
helper. The signed controlled-Job preflight guard remains closed until those
boundaries have their own released evidence.

### Factory runner process ceiling

The ignored `factory_process_limit_bounds_the_live_runner_descendants` contract
uses a finite Linux ARM64 client that attempts twelve simultaneous children,
releases their barrier, and reaps them before exiting. With `process_limit=8`,
the real guest must report a nonzero real UID and at most seven children plus
its runner. Linux excludes real UID 0 and certain privileged capabilities from
[`RLIMIT_NPROC`](https://man7.org/linux/man-pages/man2/getrlimit.2.html), so a
root runner does not provide this ceiling. The Linux counter also includes
threads, so a released runner/plugin role needs enough declared capacity for
its threads as well as its subprocesses. A small declared ceiling remains
enforced; the adapter does not raise it to make a task succeed.

Factory execution selects the numeric owner of the trusted private runtime
root. The pinned SDK presents private bind files to that same guest UID/GID and
executes the runner as that user with the pinned SDK's restricted security
profile. The separate ignored
`factory_runner_cannot_regain_privileges_through_guest_setuid_executables`
contract inspects the real guest's kernel `no_new_privs` flag and effective
capabilities: they must be one and zero respectively. This prevents subsequent
exec from recovering setuid/file-capability privileges. Host permissions remain
private; the runtime does not widen them to enable access. Root-owned runtime roots are explicitly
unsupported until an ownership mapping is qualified. Ordinary Job execution
retains its existing user and mount ownership.

Compile `tests/fixtures/process_limit_probe_aarch64.c` from the Microsandbox
crate using the same Clang/ELF linker commands as the transport client above,
then set `OCTACITY_CONTRACT_PROCESS_LIMIT_PROBE` to its executable and run:

```shell
cargo test -p octacity-execution-microsandbox --test private_tool_channel \
  factory_process_limit_bounds_the_live_runner_descendants \
  -- --ignored --exact --nocapture
```

This qualifies the runner descendant process ceiling in the selected real
backend. It does not qualify a separate host reader helper's resource limits,
CPU/memory limits, aggregate tool response accounting, or the complete ToolRisk
controlled-Job route.

### Host-only candidate inputs

The ignored `octacity-job` integration test
`candidate_inputs_remain_host_readable_and_are_absent_from_the_real_guest`
uses the public `ProtectedInputStager`, `prepare_runner_projection`, released
Octa runner, and real Microsandbox backend. Its immutable manifest contains a
public document plus the two candidate source inputs. The host can read both
source inputs before and after execution. Inside the Linux ARM64 guest, an
ordinary Octa task reads the public document, cannot modify it, and verifies
that the bundle, capture manifest, and alternate host staging roots are absent.
Runner supervision owns the deadline and mandatory VM destruction.

Provision the pinned Microsandbox executable, firmware and immutable image,
then set `OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT`,
`OCTACITY_CONTRACT_MICROSANDBOX_STATE_ROOT`,
`OCTACITY_CONTRACT_MICROSANDBOX_EXECUTABLE`,
`OCTACITY_CONTRACT_MICROSANDBOX_LIBKRUNFW`,
`OCTACITY_CONTRACT_MICROSANDBOX_IMAGE`, and the verified Linux ARM64 release at
`OCTACITY_CONTRACT_OCTA_RELEASE_ROOT`. Use the same private `MSB_HOME` as the
ordinary real-backend contract. Run:

```shell
cargo test -p octacity-job --test candidate_input_projection \
  candidate_inputs_remain_host_readable_and_are_absent_from_the_real_guest \
  -- --ignored --exact --nocapture
```

The fixture uses binary and JSON canaries to qualify physical projection; it
does not reconstruct a Git candidate, verify a JobSpec signature, execute a
model, or qualify a complete Factory pilot. Those boundaries require their
separate contracts. As a sensitivity check, passing the original complete
host input root to the backend makes the real guest task fail; the production
projection passes. This evidence grants no writable or tool authority.

### Standalone protected-reader worker

The public `octacity_runner::verify_standalone_program` installation boundary
binds a relative entrypoint to its operator-controlled distribution root,
exact SHA-256 and binary platform without an Octa lock or plugin directory.
`cargo test -p octacity-runner --test standalone_installation` covers guest
platform and digest substitution, root escape and a replaceable nested
directory. Inventorying these bytes does not establish backend qualification;
the execution owner must revalidate them immediately before starting a helper.

The portable Unix contract invokes the actual `octacity-protected-reader`
binary built by the `octacity-lifecycle` package:

```shell
cargo test -p octacity-lifecycle --test protected_reader_process
```

The supervisor chooses the working directory. One u32 big-endian length frame
carries strict JSON `{schema_version: 1, relative_path, max_bytes}`; the worker
cannot choose another root, executable or command. Framing is bounded before
allocation, portable paths use the shared validator, and physical reads use the
retained no-follow directory primitive. Replies contain one status byte
(`0` success, `1` refusal), a u32 big-endian payload length and raw bounded bytes.
The test verifies exact bytes, root escape and overflow refusal, unknown-field
rejection and path redaction in diagnostics.

This host process test does not qualify CPU, memory, process-tree accounting,
mount isolation, lease authorization, helper inventory or teardown in a VM.
Those remain separate conditions for enabling controlled Jobs. No helper is
selected automatically, and the controlled-Job preflight guard remains closed.

The separate ignored `standalone_reader_uses_its_actual_entrypoint_without_an_octa_distribution`
contract runs the exact Linux ARM64 worker through the real Microsandbox engine.
Its `ExecutionProgram` has no Octa plugin projection. The worker reads its
readonly source with networking disabled and returns the independently specified
19-byte payload. The operator must supply `OCTACITY_CONTRACT_PROTECTED_READER`
in addition to the pinned Microsandbox environment variables described below.
This qualifies that process and transport boundary, not a complete controlled
Job, installed broker/normalizer, combined resource owner or released pilot.

The ignored Agent integration `confined_reader` exercises the consumed action
through `ConfinedProtectedReadBackend` with the exact inventoried Linux ARM64
binary, reserved CPU/memory/disk/process allocation and one original deadline.
It verifies the 15-byte admitted source result, cancellation of a real worker
blocked before its input frame, and caller-future deletion. The cancellation
and deletion tests retain the actual guest input stream and assert successful
physical kill and destruction before owner shutdown completes; a caller timeout
cannot count as teardown. Set `OCTACITY_CONTRACT_PROTECTED_READER_SHA256` to the
independently verified artifact digest as well as the pinned VM environment.

```shell
cargo test -p octacity-agent --test confined_reader -- --ignored
```

Microsandbox metrics can become available after the VM is ready. Accounting
waits for a verified SDK metrics report at the configured sampling cadence,
within the original execution deadline. A missing report never produces zero
usage; identity/catalog errors and deadline expiry remain accounting failures.
The completed-reader usage contract runs two sequential real workers and
compares their separately observed backend samples with the cumulative ledger.
CPU, I/O and worker elapsed time accumulate with overflow rejection; serialized
memory/disk peaks retain the larger sample, and destroyed workers have zero
current memory. Job wall time is still the original main execution window.
The main-owner contract starts another real VM with the narrowed, hidden-source
allocation and keeps its input open. Killing its attached `RunningExecution`
awaits the blocked reader's actual kill and destruction, rather than relying on
caller deletion or the helper's duration limit.
The neutral `RunningExecution::sample_final_usage` boundary preserves ordinary
backend sampling while allowing a composed owner to seal and drain auxiliary
work before sampling the unreaped main process. The final-owner contract checks
both actual VM samples: CPU and I/O accumulate, distinct writable roots add,
main wall time remains unchanged, and new reads are refused after sealing.
Summed per-part memory/disk peaks are conservative accounting bounds; they do
not claim simultaneously observed whole-Job peaks.
These terminal component contracts do not establish live combined main/helper
accounting, runner integration, signed Job composition or released controlled-Job
qualification.

`owned_writable_disk_excludes_readonly_inputs_and_other_execution_roots`
qualifies the explicit `OwnedWritableMounts` accounting scope. A 16 MiB worker
allocation contains 2 MiB of scratch and 2 MiB of output; separate 32 MiB readonly
source and 32 MiB sibling state must neither prevent launch nor appear in its
disk usage. The real worker completes and reports exactly 4 MiB. Unpartitioned
executions retain `WholeJob` accounting. Both parts of a validated Factory
partition request the explicit owned scope; combined Job accounting and teardown
remain separate qualification requirements. The physical test fixture uses
unique execution identities so concurrent contracts cannot share a sandbox.

Readonly staging has a separate bound: the signed protected-input manifest
allows at most 1 GiB in aggregate, and staging verifies each exact size and
digest. Excluding those immutable bytes from one participant's writable share
does not remove that staging limit. Total materialized repository storage and
combined Job usage still require their own accounting checks.

```shell
cargo test -p octacity-execution-microsandbox --test private_tool_channel \
  -- --ignored
```

### Native readonly candidate composition

The ignored `octacity-server` integration test
`default_native_runtime_materializes_one_exact_readonly_candidate_and_recovers_its_inputs`
uses the default `ServerRuntime`, an operator-owned version 1 profile file,
PostgreSQL, and S3-compatible object storage. A fixture first publishes retained
producer facts through ordinary Build admission, lease-authorized Artifact
upload, completion, and fenced Factory journal ports. The default owner then
selects that exact persisted candidate for a configured readonly consumer.
Its ordinary managed Job retains four compiler documents and two source inputs;
the binary bundle is absent from the compiler document bodies. Restart preserves
the Build identity and frozen inputs, and independently retained candidate proof
remains valid. The selected capture-tool pin comes from the private installed
profile, never from a model response or the capture manifest.

With disposable services, set `OCTACITY_POSTGRES_URL`,
`OCTACITY_MINIO_ENDPOINT`, `OCTACITY_MINIO_ACCESS_KEY`, and
`OCTACITY_MINIO_SECRET_KEY`; provision the `octacity-artifacts` bucket. Run:

```shell
cargo test -p octacity-server --test factory_native_candidate \
  default_native_runtime_materializes_one_exact_readonly_candidate_and_recovers_its_inputs \
  -- --ignored --exact --nocapture
```

This is a server composition contract. Its precompleted producer and binary
canary do not qualify real Git capture or restoration, Agent execution, model
completion, writable roles, ToolRisk control, or a full Factory pilot. The
consumer retains readonly source mounts and no write, capture, or tool authority.

### Scoped guest writes and trusted Git capture

The ignored `octacity-job` test
`real_guest_change_is_confined_and_roundtrips_through_trusted_git` checks a
component boundary using the public `FactoryExecutionLayout`, runner
`supervise`, `GitChangeSetCapturer`, and `GitChangeSetMaterializer`. A finite
ordinary Octa task changes one file inside an explicitly writable source
prefix in the real Microsandbox guest. The remaining source tree, Git metadata,
and protected document must remain readonly. After mandatory VM teardown,
operator-pinned host Git captures one bounded change and reconstructs its exact
candidate revision in a fresh private repository; the changed file has a known
independent content digest.

Use the same pinned Microsandbox and Linux ARM64 Octa variables as the other
physical component contracts, and set `OCTACITY_CONTRACT_GIT_EXECUTABLE` to an
absolute trusted Git executable. Run:

```shell
cargo test -p octacity-job --test guest_candidate_capture \
  real_guest_change_is_confined_and_roundtrips_through_trusted_git \
  -- --ignored --exact --nocapture
```

The fixture allows 64 runner/plugin processes independently of the separate
exact PID-limit-8 contract. It changes no signed Job or installed profile
limits. This direct component test does not exercise JobSpec signature checks,
the complete `JobExecutor` capture handoff, Artifact publication, model or
protected-tool execution, native author installation, or a full Factory pilot.
Those gates remain separate. As a sensitivity check, replacing the readonly
source plus explicit writable prefix with an unrestricted writable source mount
makes the real guest mutate the neighbouring file, and this test fails. The
bounded projection passes without a production permission change.

### Installed author capture policy

The portable Application contract checks ordinary Job assembly separately from
physical backend qualification. An installed `FactoryCodexCapturePolicy` binds
the trusted producer pin and deterministic capture instruction into the task
digest. The admitted intent must declare that exact capture implementation and
source path scope before protected documents are published. Candidate consumer
pins remain separate. Tool-free author assembly selects the coding credential,
retains four compiler inputs, and signs the bounded capture instruction; it
grants no protected-tool authority. Existing readonly profiles keep their task
identities with `capture: None`.

```shell
cargo test -p octacity-server-application --test manual_trigger \
  codex_capture::installed_codex_author_materializes_only_the_explicit_trusted_capture_scope_without_tools \
  -- --exact
```

This portable assembly test does not by itself qualify an actual execution.
The separate native profile contract accepts bounded capture with the coding
credential purpose and exact producer outputs; readonly profiles retain their
independent evaluation credential and mount restrictions. Placement requires a
workspace anchor capability on the exact backend route, and unsupported routes
remain ineligible.

The separate portable observer contracts retain the verified public model
result alongside capture bundle/manifest records, without interpreting binary
capture data as model text. Capture metadata is checked against current frozen
source scope and the installed empty-change and direct-path-count policies; retained recovery also
rejects direct changes outside that source scope. These contracts use published
Artifact lifecycle records and independently pinned bytes, and do not prove the
Git contents or a complete signed author execution.

```shell
cargo test -p octacity-server-application --lib factory_codex_output_source_tests
cargo test -p octacity-server-application --lib factory_node_capture_tests
```

### Empty parent workspace projection

A signed managed task may select the fixed `/workspace` parent workspace. The
Agent projects an empty read-only hierarchy there, then separately mounts the
admitted Source, Scratch and Output roots. It never mounts the host Job parent.
Source remains read-only except for approved directory overlays; immutable
protected inputs retain their separate read-only path, and Source2 is absent.
The backend capability defaults to false: providers without this qualified
projection refuse it before Job inspection or resource creation.

```shell
cargo test -p octacity-job --test guest_candidate_capture -- --ignored
cargo test -p octacity-execution-oci --lib
```

The explicit Microsandbox component contract changes a selected source file in
a real guest and round-trips that change through trusted Git capture and restore.
It also checks the inaccessible private Source2 sibling and refused source,
Git-metadata and protected-input writes. This qualifies the mount boundary, not
a remote model, installed author profile or complete Factory pilot. Octa and its
released plugin binaries are unchanged.

### Signed author Job on the released plugin

```shell
cargo test -p octacity-release-harness --test factory_author_job -- --ignored
```

The explicit contract requires pinned Microsandbox, firmware and image,
released Linux ARM Octa, an actual Source Git plugin and the operator-selected
Git executable. It compiles and verifies a signed managed Job, invokes the
released CodeX plugin, then captures the selected source change through trusted
Git. The operator-installed finite CLI replaces the remote model process. It
requires the private credential in its normal environment, writes one selected
file and returns the frozen Boolean; the contract does not qualify a remote
provider or real model behavior. Events, terminal results and every retained
CodeX Output file are checked for the private credential canary. Cleanup occurs
before assertions, including the publication-root and capture checks.

The same signed contract reads hostile repository instructions as data and runs
finite filesystem attacks from the released plugin's normal CLI process. Its
retained `output/security-probes.json` records each result explicitly: writes
to a source sibling, Git metadata, an escaping repository symlink and the
protected Octafile; reads of private Source2 and another host credential; and
creation of an undeclared workspace sibling must all be `denied`. The protected
Octafile must already be readable, otherwise the probe records `absent` and
the contract fails. Source2 contains a known host-readable canary before the
guest starts. The other credential is also created and checked on the host;
neither its bytes nor its private path may enter public output or diagnostics.
The admitted source edit and its trusted Git capture must still succeed.

The portable CLI fixture contract deliberately runs the same probes without a
sandbox and requires the writable/readable attacks to report `allowed`. It also
checks that repository shell substitutions never execute. This sensitivity
check prevents canned denial reports from qualifying the physical boundary:

```shell
python3 -m unittest discover -s tools/tests -p test_factory_codex_author_fixture.py
```

These are deterministic adversarial process and mount contracts. They qualify
the Agent boundary when an untrusted process attempts those operations, without
claiming that a remote model resists natural-language prompt injection.

| Threat boundary | Executable evidence | Scope |
| --- | --- | --- |
| Repository prompt and path/mount escape | `factory_author_job` plus the CLI fixture contract above | Signed Job, released plugin, real Microsandbox and trusted capture |
| Private candidate/context files | `candidate_input_projection`, `guest_candidate_capture` | Real guest projection with known host canaries |
| Private variable crossing or task replacement | Runner `private_variables`; Job `execution_v2::installed_private_credentials` and `missing_installed_factory_credentials` | Portable binding, digest revalidation, refusal ordering and redaction |
| Executable/platform/root substitution | Runner `standalone_installation` and installation tests | Portable exact identity and filesystem ownership checks |
| Unqualified execution or Host fallback | Job `execution_v2::reports_an_unqualified_mode_without_falling_back_to_a_legacy_backend` and `never_falls_back_to_host_for_a_factory_job` | Portable exact route refusal before execution |

Passing one row does not qualify the other boundaries or a complete Factory
pilot. The portable refusal tests do not require a privileged runtime; the
explicit real-backend tests still require their pinned installation.

### Private credentials bound to immutable tasks

The portable Job/Runner contracts install a logical credential profile as opaque,
zeroing named values. The profile identity never selects a repository secrets
file. An absent installation stops before Artifact download, Source acquisition
or backend start. Only the generated Octafile's exact root `required: true,
secret: true` declarations may receive these values; public variables, includes,
task dependencies and nested variable scopes cannot acquire this authority.

Runner independently reopens the protected Octafile and verifies its frozen
hash, size and selected tasks immediately before backend start. Changed bytes
and public/private variable collisions stop before that boundary. Values exist
only in the transient runner command, are erased after writing, and extend
redaction before any runner event or result is forwarded. These APIs accept the
closed compiler-generated task shape, rather than general repository Octafiles.

```shell
cargo test -p octacity-runner --test private_variables
cargo test -p octacity-job --lib tests::execution_v2::installed_private_credentials
cargo test -p octacity-job --lib tests::execution_v2::missing_installed_factory_credentials
```

These portable contracts qualify binding and refusal/redaction ordering. Their
recording backend does not qualify provider credentials, the actual released
model/plugin process, native author installation or a complete Factory pilot.

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
  hashicorp/vault:2.1.1@sha256:8af37ae9d45e4a0fac48ab700e0d99efc4c2d6cd84354a869f2147f1d3abab46

python3 tools/build_pinned_minio.py minio octacity/minio-contract:source-pinned
docker run --detach --rm --name octacity-minio-contract \
  -p 127.0.0.1:19000:9000 \
  -e MINIO_ROOT_USER=octacity \
  -e MINIO_ROOT_PASSWORD=octacity-secret \
  octacity/minio-contract:source-pinned
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
export OCTACITY_CONTRACT_MICROSANDBOX_ENVIRONMENT_IDENTITY=microsandbox-0.7.6-linux-amd64
export OCTACITY_CONTRACT_MICROSANDBOX_IMAGE='registry.example/build@sha256:<64-lowercase-hex>'
export OCTACITY_CONTRACT_MICROSANDBOX_ALLOWED_HOST=allowed.contract.example
export OCTACITY_CONTRACT_MICROSANDBOX_DENIED_HOST=denied.contract.example
cargo test -p octacity-job --test backend_contract \
  microsandbox_backend_satisfies_the_real_runner_contract -- --ignored --exact --nocapture
```

The Windows preview workflow downloads the pinned `0.7.6` x86_64 ZIP, verifies
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

## Portable review planning through the common node journal

Run `cargo test -p octacity-server-application --lib factory_review_planning_tests`.
The two-node contract publishes a trusted Evidence Manifest, projects its complete
structured value into a configured planning action, and retains one immutable
Evaluation Plan. Restart returns the original accepted record. The installed
profile binds criteria, evaluator capabilities, data-handling policy, budgets,
action and output contracts by content digest; logical node names carry no authority.

The planner requires an exact persisted input, one selected source-record digest,
and matching code-owned evidence facts. Identical JSON without those facts and
partial projections fail closed. Unavailable required capability produces the
configured escalation without evaluator I/O. Plan authority ends by the earliest
evidence freshness boundary and the original deadline. Trusted time is checked
after snapshot reads and before returning a result; elapsed time and structured
output bytes are charged within the frozen ceilings.

These portable tests use the existing authoritative in-memory journal contract.
They qualify the planning action, persistence and replay semantics; production
evidence construction, native PostgreSQL composition and evaluator execution
require their own integration evidence.

## Deterministic Decision from a selected completed review

Run `cargo test -p octacity-server-application --lib factory_evaluation_action_tests`.
The configured journey publishes an Evidence Manifest, Evaluation Plan and typed
Assessment, then projects their complete values into the installed Decision action:
`{evidence, plan, assessments, signal_receipts}`. Each value must match an explicitly
selected committed record, its digest and its trusted discovery facts. Copying a
Plan into configuration or supplying Assessment-shaped model JSON grants no authority.

The action derives the completed branch join from the original persisted reviewer
inputs, Contexts and Node Attempts. It accepts no caller-supplied progress and
cannot manufacture a missing required result. The installed profile binds the
Decision policy version/digest and all five disposition-to-outcome contracts.
The existing pure Decision Engine selects the typed disposition; the configured
Flow owns subsequent routing. The common fenced journal retains the complete
Decision and its safe discovery projection, and restart returns that same record.
Evidence freshness, the Plan deadline, the Node deadline and the current authority
window all remain upper bounds; result bytes and elapsed time consume the Node budget.
Native `evaluation_decision_executor` and FILE `evaluation_decisions` install
the same action. The runtime contracts reject policy or output-contract changes
without repinning the identity; a metadata-only parser cannot silently omit an
executor. Run the Server library `factory_evaluation_runtime_tests` filter for
these composition contracts.

These contracts qualify metadata composition and replay using the authoritative
in-memory journal. They do not establish real evaluator execution, delivery or
deployment; those effects require the packaged PostgreSQL-backed pilot.

The generic `flow_tagged_schema_tests` contract preserves externally tagged JSON,
checks unit/payload alternatives, bounds, restoration, structural compatibility
and template alias expansion. The CodeX compiler and public OpenAPI tests verify
the same closed alternatives at their respective boundaries. This permits typed
non-accepting Decision reasons without changing historical Decision serialization
or widening graph identifiers. Operator Console tests cover authoring, value entry,
compatibility and own-property handling for names such as `__proto__`.

## Installed GitHub API trust over a real TLS peer

Run `cargo test --locked -p octacity-github --lib`. The HTTPS tests load the
private provider TOML through the public plugin API and observe an exact existing
review over real TLS. Optional `api_root_certificates_pem` accepts at most eight
individual PEM certificates, at most 16 KiB each, merged with system API trust.
The exact configuration bytes remain the provider identity. Unknown trust,
hostname mismatch, malformed/oversized/bundled entries and an unrepinned trust
change fail closed. HTTP redirects, automatic retries, credential scope and
Git's independent TLS/executable configuration retain their existing boundaries.

These portable tests qualify the installed API transport. They do not prove
packaged delivery, remote Git TLS or an actual Factory publication; those require
the corresponding released-product fixture and real observed side effects.

## Retained candidate delivery inputs

The Native delivery boundary keeps the composite captured-candidate identity
separate from the SHA-256 of the manifest file. Delivery protocol v1 retains
its existing `changeset_digest` wire meaning: the manifest file SHA-256. Before
staging either private connector input, the server verifies the canonical,
provider-neutral capture manifest against the composite candidate digest,
candidate revision, actual retained bundle hash and size, and original Work
base. A later coding cycle may have a different immediate parent while retaining
that same original base. Substituted inputs produce no staged files; exact
replay preserves the original private files and their permissions.

```shell
cargo test --locked --jobs 2 -p octacity-server --lib runtime::delivery
cargo test --locked --jobs 2 -p octacity-protocol --lib delivery
```

These contracts qualify the server handoff and unchanged v1 wire contract.
They do not establish an external publication.

## Durable HTTPS review target

`factory_delivery_target` exercises the pilot's separate HTTPS forge through
actual requests. Ref lookups read the test-owned bare Git repository, including
a candidate branch created by a real push. Creating a review atomically stores
its supplied metadata before optionally dropping the HTTP response. Read-only
observation returns that review and the actual Git revisions; target restart
preserves the review without another POST. Request sizes, accepted routes and
credentials are bounded. The target never creates a Factory Delivery receipt.

```shell
cargo test --locked -p octacity-release-harness --test factory_delivery_target
```

The ignored `installed_github_process_observes_actual_https_git_absence` case
also invokes an explicitly built native provider executable with an empty
inherited environment and private installed configuration. It observes two real
HTTPS Git-reference reads, each carrying exactly one configured
`x-github-api-version` header, first directly and then through the common
`DeliveryAdapterRegistry` process host. Both observations prove that absence
creates neither a review nor a POST. The fixture continues to reject duplicate headers. Set
`OCTACITY_FACTORY_GITHUB_PLUGIN` to the exact executable path and
`OCTACITY_CONTRACT_GIT_EXECUTABLE` to the pinned system Git executable:

```shell
cargo test --locked -p octacity-release-harness --test factory_delivery_target \
  installed_github_process_observes_actual_https_git_absence -- --ignored --exact
```

This qualifies the external pilot fixture. Packaged GitHub plugin publication,
its observe-before-retry behavior and the Factory delivery owner still require
the corresponding released-product journey. Test-owned local Git transport
also does not qualify remote Git TLS.

## Packaged Server/JEV shadow and unknown-response slice

`factory_pilot_signal` starts the checksummed Server binary against a disposable
PostgreSQL database and installs the real JEV adapter from private operator
files. Its local HTTPS fixture supplies an explicit private CA; normal hostname
verification remains enabled. The fixture is an external provider boundary and
does not seed receipts, Factory history or execution effects.

Use the released-product environment described above: the verified
`OCTACITY_RELEASE_SERVER_ROOT`, `OCTACITY_RELEASE_AGENT_ROOT` and
`OCTACITY_CONTRACT_OCTA_RELEASE_ROOT`, the selected `OCTACITY_RELEASE_BACKEND`
and its contract variables, plus `OCTACITY_POSTGRES_URL`,
`OCTACITY_MINIO_ENDPOINT`, `OCTACITY_MINIO_ACCESS_KEY` and
`OCTACITY_MINIO_SECRET_KEY`. PostgreSQL must allow creation and removal of
disposable databases. The Server bundle must contain the current adapter build.

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-release-harness --test factory_pilot_signal \
  -- --ignored --nocapture --test-threads=1
```

The successful case publishes its Flow, configuration and manual Work through
REST. An actual shadow recommendation disagrees with the declared baseline,
while deterministic consumption retains that baseline. Canonical request and
receipt integrity, exact persisted bytes and public diagnostics survive Server
restart without another POST. Only the declared public state reaches JEV;
unselected Work metadata remains private.

The lost-response case accepts one HTTPS POST and closes the connection. It
retains an unresolved reservation and publishes no terminal receipt. After
restart, the test waits for a persisted successor claim, drains the Server's
in-flight worker through graceful shutdown, and verifies unchanged reservation
bytes, no manufactured receipt and exactly one total POST. Ordinary readiness
remains independent. This recovery never guesses JEV's lost opaque request ID.

Provider-neutral recovery readiness is covered without remote calls by
`cargo test --locked -p octacity-server --lib
signal_connector_readiness_tracks_recovery_without_reevaluating_the_request`.
After a timeout, only an exact recovered result restores availability; absent
observations preserve the prior state, while invalid results, failures and
interrupted lookups cannot establish health. Recovery never invokes `evaluate`.
This contract does not add receipt lookup support to adapters that lack it.

Both cases passed against the current packaged Server in 66.13 seconds:
`/tmp/octacity-165-current-packaged-signal-final.log`.

This is a Signal-only composition test. Its Work artifact references are
explicitly unselected, no Agent process runs, and it does not qualify retained
artifact reads, Git transport, model execution, tool-risk execution or the full
nested-flow pilot. Those boundaries require their own real execution tests.

## Packaged nested model execution and restart

`factory_pilot` uses the same released-product environment and requires
`OCTACITY_FACTORY_PILOT_CLI` to select the native Linux ARM64 fixture executable.
Build `fixtures/factory/pilot-cli.rs` with a pinned matching Rust toolchain; it
embeds `pilot-cli.mjs` and executes the qualified image's `/usr/local/bin/node`.
The fixture accepts a bounded installed JSON program. It does not dispatch on
Flow names, node names or model roles. Changes to the embedded program require
rebuilding the executable; its actual SHA-256 is pinned in the installed profile.

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-release-harness --test factory_pilot \
  packaged_factory_pilot_runs_real_jobs_before_work_admission \
  -- --exact --ignored --nocapture --test-threads=1
```

The test creates a disposable PostgreSQL database, publishes ordinary resources
through REST, enrolls the packaged Agent and runs the released Git source plugin
and Octa in the qualified Microsandbox image. A private operator-installed Git
mirror supplies the exact local fixture revision. Task and acceptance references
come from the resulting published artifacts. No completed Job, Flow record,
model result or receipt is inserted directly into the database.

The second phase installs a finite readonly model profile and admits a nested
Flow through REST. The real model Job must produce the three released Codex
records; the Server verifies their retained bytes, provenance and structured
result before the Flow reaches its declared operator gate. Restarting both
processes preserves identical node records, Build references and call receipts.
An operator response then completes the Flow without another model
Build. Restart waits allow the installed durable claim lifetime plus acquisition
and completion time; polling does not assume that an old claim disappears at
process shutdown.

This slice verifies native model execution and restart continuity. It does not
yet qualify protected tool reads, private-test publication, independent candidate
consumers or policy-governed promotion. The complete configurable journey in
task 16.5 requires those additional real execution boundaries.

## Signed controlled execution

`factory_controlled_job` exercises a signed v3 Job with the released Codex plugin
and Octa, an actual native finite CLI, and separately confined reader executions.
Only the remote model and coordinator transport are contract fixtures. The test
uses no Flow or node labels to select executable behavior.

Provide the Microsandbox contract environment plus
`OCTACITY_FACTORY_PILOT_CLI`, `OCTACITY_CONTRACT_PROTECTED_READER`,
`OCTACITY_CONTRACT_CODEX_READER`, `OCTACITY_RELEASE_AGENT_ROOT` and
`OCTACITY_CONTRACT_GIT_EXECUTABLE`. The reader and bridge must be actual immutable
Linux guest executables. The Agent bundle supplies the released Git source
plugin. Use dedicated short private work/state roots; Unix runtime socket limits
also apply to Microsandbox's derived paths. Import the exact qualified image into
that state root's `microsandbox` home before running; another installation's image
cache does not satisfy this prerequisite.

This direct signed-Job fixture passes a local repository to SourceGit. Supply
a disposable Agent bundle whose source-plugin settings explicitly allow local
file transport; the production bundle keeps it disabled. The packaged pilot
instead installs its own private Git mirror transport.

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-release-harness --test factory_controlled_job \
  -- --ignored --nocapture --test-threads=1
```

The immutable command grant permits exactly one public file. The main model VM
cannot read Source directly; its structured result must equal the bytes returned
through the neutral Agent channel and pinned reader. The provider-local hook
first sends an authorization-only request and waits for a fenced disposition;
it receives no file bytes and reserves no response quota. The subsequent MCP
call obtains current authorization for the same canonical proposal immediately
before its single physical read. No permission token is cached or transferred
to the workload. Both exchanges share the original execution scope, while only
the physical read consumes the signed response quota and helper allocation.
Helper executions are serialized. The portable `codex_reader_process` contract
checks hook-to-MCP dispatch and refusal after a successful hook; the dispatcher
contract uses exactly one response's byte allocation and rejects another read.

Portable regressions are `cargo test --locked -p octacity-lifecycle --test
codex_reader_process` and `cargo test --locked -p octacity-lifecycle --lib
hook_authorization_preserves_the_single_read_response_allowance`.

Assertions cover the exact current registration/lease/fence, distinct request
identities, exact structured result, absence of the unselected canary in retained
outputs/events, and removal of the private endpoint and directory. Installation
pins are revalidated before execution. The ordinary Agent component and lifecycle
tests separately cover explicit bridge installation, safe substitution failure,
and current-lease handoff without starting controlled work for ordinary Jobs.

The normal Agent advertises `controlled_tools` only for the exact compatible
route after both reader programs and the private channel address are verified.
Older inventories default to unqualified; a flag on another backend cannot
satisfy placement. An unusably long Unix channel path fails startup before
registration, with a diagnostic that omits the private path.

This tracer qualifies the released execution boundary. It does not start a
packaged Agent daemon against the real Server authorization endpoint or establish
the complete protected-test/candidate/promotion journey required by task 16.5.

## Protected-test author completion

The application contracts `factory_protected_test_publication_tests` and
`protected_author_completion_resolves_the_actual_signed_job_before_publication`
exercise publication before successful Job settlement. The installed canonical
policy pins private logical outputs, classes, paths, coverage and retention.
The Server derives inventory entries from actual published file identities and
publishes the closed `ProtectedTestHandoff` under the admitted outcome schema;
no command or model predicts future Artifact IDs.

The completion finalizer resolves the actual Job, immutable Build origin,
current Attempt, persisted accepted ancestry and exact compiled command program.
A matching origin alone does not qualify a substituted Job. Current Lease
fencing remains required for every publication, including a replay whose bytes
already exist. A terminal completion acknowledgement performs no new writes and
returns to the ordinary completion ledger. Private artifacts remain opaque in
the common Flow result; only the schema-validated public Report carries data.

These application tests use real domain contracts and in-memory authoritative
ports. They do not replace the PostgreSQL-backed packaged author/candidate/
promotion journey required by task 16.5.

The packaged author boundary is covered separately by
`packaged_protected_author_completes_with_actual_private_file_identities` in
`factory_pilot`. Run that ignored case with the same release, PostgreSQL,
object-store and Microsandbox environment as the nested pilot above. Its
installed static command publishes an actual private file; the normal Server
completion finalizer derives the inventory and closed public handoff. The test
reads the exact private bytes through authorized Artifact access, checks that
public Flow data exposes no private contents or paths, and restarts Server and
Agent without changing retained records or admitting another Build. The command
explicitly pins the full approved Octa plugin inventory, while executable, tool,
network and credential grants remain empty. Candidate execution, independent
verification and promotion remain separate pilot steps.


## Packaged controlled tool-risk execution

The second `factory_pilot` case adds the actual packaged Agent composition to
that released execution proof. It uses the same PostgreSQL and object-store
contract environment, immutable Linux ARM64 model fixture and qualified
Microsandbox image. Also supply `OCTACITY_CONTRACT_PROTECTED_READER` and
`OCTACITY_CONTRACT_CODEX_READER` as separately pinned native Linux guest programs.
The fixture installs them outside writable Job and credential roots. Short
private backend state roots remain necessary for the control channel and guest
runtime sockets.

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-release-harness --test factory_pilot \
  packaged_factory_tool_risk_job_reads_only_through_its_actual_agent_lease \
  -- --exact --ignored --nocapture --test-threads=1
```

A real ordinary bootstrap Job publishes both Work inputs. A separate installed
ToolReasoning profile, File catalog and current Agent then execute one nested
model Job. Registration must advertise `controlled_tools` on the exact selected
Microsandbox route. The signed command permits one existing public file; the
model VM must be unable to open it directly. Its retained structured result must
instead equal `facts`, read through the blocking hook, Agent-owned channel and
confined helper.

The installed JEV connector reaches a finite HTTPS wire fixture with a private
CA and a dedicated Server-only credential. The authorization-only hook and the
MCP read use the same canonical proposal, reusing one actual POST and one
durable Shadow receipt while performing only one physical read.
The test verifies receipt integrity and deterministic Allow consumption, with
only the configured redacted state reaching the connector. Retained outputs
must contain none of the independent model, provider or unselected Source
canaries. No completed Job, model result, receipt or Flow record is inserted by
the fixture.

Both controlled execution contracts now allow exactly the five bytes in `facts`
as their per-call and total response allocation. The packaged ToolRisk contract
also permits only one unique assessment. This prevents a duplicate physical read
from hiding behind a larger response allocation or provider-call allowance.

Restarting the packaged Server and Agent preserves the exact completed node,
Build lineage and call receipts; an operator response then completes the nested
Flow without another model Job. This qualifies the normal daemon's controlled
execution and native tool-risk authorization path. Protected-test publication,
independent candidate consumers and promotion require their own pilot slices.

## Trusted capture observation publication

The optional signed capture observation declares an arbitrary bounded logical
Report name and immutable schema reference. After the trusted Git commit, the
Agent derives a closed four-field record from the actual capture manifest:
`candidate_revision`, `candidate_digest`, `input_digest` and `fresh_until`.
The original signed Factory input digest and execution deadline supply the last
two fields. The model's ordinary primary result remains separate. Logical names
never select filesystem paths; the trusted producer owns the private file.

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-changeset --lib \
  produces_closed_capture_observations_from_the_actual_trusted_commit
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-job --lib \
  managed_capture_binds_observations_to_signed_input_and_original_deadline
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-lifecycle --lib \
  trusted_capture_observation_is_a_schema_pinned_report_at_the_actual_freeze_boundary
```

The Lifecycle contract runs real Git capture, the production capture-output
adapter, `OutputPublisher::freeze` and actual HTTP PUT uploads. A signed zero
Report allowance must reject publication; one Report allowance retains the
exact logical name, schema identity, trusted producer, byte count and digest.
The record contains no candidate file contents or names. On the Server,
`capture_observations_require_the_exact_installed_report_contract_without_model_deliverables`
checks the exact declared Report kind, registered schema, capture tool and plugin.
`capture_observer_exposes_only_independently_verified_candidate_report_data` and
`capture_observer_rejects_schema_valid_but_substituted_candidate_facts` exercise
the installed output source against ordinary Job journals and retained Artifact
bytes. The source independently verifies the Git manifest before comparing all
four fields with the actual candidate, signed input and original deadline. A
report cannot extend actual retention. Bundle and manifest Artifacts remain
opaque; only the closed verified Report becomes Flow data. Ordinary raw Report
uploads use `application/octet-stream`; the exact pinned report format and
schema still determine JSON interpretation, without a media conversion.

Protected input transfer also preserves the signed logical media type when the
same immutable bytes were uploaded with generic transport metadata. The public
`protected_download_preserves_signed_logical_media_over_generic_upload_transport`
contract checks authorized logical metadata and rejects forged media, SHA-256
and size. The ignored S3 contract
`s3_download_honors_declared_logical_media_for_exact_generic_transport_bytes`
uploads real bytes to the configured MinIO instance, downloads through the
presigned capability and verifies both the logical Content-Type and exact body.
Run it with `cargo test --locked -p octacity-artifact-s3 --test minio -- --ignored`.
The response metadata changes neither stored bytes nor immutable integrity,
ownership, retention or Lease checks. This qualifies transfer; the complete
packaged validation journey has its own acceptance test.

The capture Build adapter publishes the independently verified `CandidateSubject`
as discovery only after successful completion. This lets declared downstream
Context inputs select the actual candidate without interpreting arbitrary model
JSON. The native Codex binding selects capture or ordinary observation by the
complete installed profile; readonly and capture profiles may share one CLI.
The existing `configured_capture_executor_returns_the_verified_manifest_through_the_common_node_contract`
contract checks discovery against the independently verified capture subject.

Run these Server application contracts with `cargo test --locked -p octacity-server-application --lib capture_observer_` and the exact compiler test
name above. These checks qualify the producer, publication and independent
acceptance boundaries. The complete PostgreSQL-backed packaged candidate
journey remains a separate pilot requirement.

## Packaged private-test author qualification

`packaged_protected_author_completes_with_actual_private_file_identities` uses
the same explicit packaged Server/Agent, PostgreSQL, object-storage, released
Octa and Microsandbox environment as the packaged pilot above:

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-release-harness --test factory_pilot \
  packaged_protected_author_completes_with_actual_private_file_identities \
  -- --ignored --exact --nocapture
```

A configured accepted Gate precedes an installed static Reasoning command.
The real Agent publishes one private file; the lease-bound Server finalizer
derives the inventory and public handoff from its actual published identity
before ordinary Job success. The common Flow completes. The test checks the
retained file bytes, opaque inventory metadata and closed public payload, then
restarts both packaged processes and compares the persisted records, Build
lineage and publications. It creates no synthetic Agent outputs or receipts.

The command explicitly approves the complete installed plugin inventory while
granting no Source, external executable, network, secret or tool authority.
This qualifies author publication and restart durability. Its finite private
file is an opacity fixture; executing protected checks against a reconstructed
candidate and the full author-to-delivery journey require separate integration
coverage.

## Packaged original-test candidate validation

With the same explicit packaged environment, run:

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-release-harness --test factory_pilot \
  packaged_original_private_tests_validate_the_real_captured_candidate \
  -- --ignored --exact --nocapture
```

The actual author publishes a private JSON assertion and the Server derives its
inventory from the published file identity. A separate ordinary CodeX Job writes
the approved Source file; trusted Git capture freezes its real candidate and
publishes independently verified candidate facts. The validation Job restores
that exact candidate read-only and runs the original retained assertion from the
original inventory. Its primary report and closed facts must bind the same
inventory, candidate revision, candidate digest, covered class and report digest.
The common Flow completes, then restarting Server and Agent must preserve the
exact records, Build lineage and publications without repeating those Jobs.

The bootstrap and all three role Jobs execute through the packaged Agent and
qualified Microsandbox backend. The model process is an explicit finite adapter
fixture; the static validator reads actual candidate and private-file bytes.
No successful Job, Artifact identity, report, capture or completion receipt is
inserted by the test. This qualifies original-test validation and durability;
separate-Agent verification, typed criterion review, delivery and promotion
remain separate parts of the complete pilot.

## Packaged independent candidate verifier

With `OCTACITY_FACTORY_VERIFIER_CLI` explicitly pointing to the separately pinned
finite CLI that supports bounded `expected_text` checks, run:

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-release-harness --test factory_pilot \
  packaged_separate_agent_verifies_only_the_explicit_nested_candidate \
  -- --ignored --exact --nocapture
```

This extends the actual original-test validation journey with a scoped nested
Flow, another immutable Build Configuration, and a separately enrolled Agent in
its own pool. The verifier receives only its evaluator credential and the two
explicit candidate Artifact references. Its finite model process reads the
actual candidate file and requires its complete bounded contents to equal the
expected text; the original private inventory, private test body and author
output are absent from its Job. The retained closed report binds the original
candidate revision, candidate digest and producer input digest. The public
ordinary Attempt diagnostics must show different Agent identities and the exact
configured pool for each role. The nested call returns the exact child payload;
Server and both Agents then restart without changing retained records, Build
lineage or publications.

This is a real packaged Server/Agent and Microsandbox check, with five successful
ordinary Jobs including bootstrap. No report, capture, Artifact identity or
completion receipt is seeded. It qualifies isolated candidate verification;
it does not substitute for EvaluationPlan-bound Assessment review, external
delivery or deployment, or qualify the complete pilot by itself.

## Packaged candidate delivery and accepted-response recovery

With the separately pinned `OCTACITY_FACTORY_VERIFIER_CLI` and native
`OCTACITY_FACTORY_GITHUB_PLUGIN` executable explicitly selected, run the existing
private pilot environment:

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-release-harness --test factory_pilot \
  packaged_delivery_recovers_the_accepted_real_review_after_server_restart \
  -- --ignored --exact --nocapture
```

This extends original private-test authoring, real CodeX capture, original-test
validation and separate-Agent nested verification with the installed Native
publication action. Its Node and Project explicitly grant both the exact frozen
action identity and the pinned physical provider plugin. The provider alone owns
the delivery credential; model Jobs do not receive it.

The target reads a real bare Git repository. The provider pushes the exact
captured revision and creates a review over verified HTTPS. The fixture syncs
that accepted review before holding its response and any recovery observation.
The parent then kills and reaps only its owned Server process, releases the held
response, and starts Server against the same PostgreSQL journal. Recovery must
retain the exact candidate receipt, original base, actual branch contents and
review identity, with exactly one review POST. A final normal Server and both
Agents restart preserves the common records and ordinary Build lineage.

The qualified slice executes five ordinary Jobs including bootstrap and retains
seven configured node completions. No Git effect, review, Artifact or Factory
receipt is seeded. The separate public Server runtime timeout regression also
proves completed task handles are joined once, while remaining owned tasks are
aborted and drained under the original shutdown bound; fault injection does not
require a graceful-success response from a deliberately crashed process.

```shell
cargo test --locked -p octacity-server --lib \
  shutdown_timeout_drains_only_tasks_that_have_not_already_completed
```

The provider's private installed Git wrapper redirects only its frozen logical
HTTPS repository to the test-owned bare mirror. This qualifies actual local Git
effects and HTTPS API delivery recovery, not remote Git TLS, deployment,
EvaluationPlan-bound review in this same journey, or the complete pilot.

## Finite deployment CLI contract

The operator-installed `fixtures/factory/deployer.mjs` transfers a declared
regular candidate file to a separate HTTP target and verifies its actual bytes
through two target reads. These reads bracket the configured observation window;
the report records that complete window, ending at the second verified read.
The command accepts the compiler's protected `deployment-target.json`, existing
`promotion-input.json` and frozen command input. It requires the exact single
environment contract and original input digest, honors the original deadline,
and emits the existing `PromotionObservation` shape. Its final CLI argument is
the operator-installed arbitrary strategy key, frozen by the command task pin.
The finite program supports one stage-only target with empty allocation and no
production environments; it rejects an uninstalled strategy or unsupported
rollout parameters before any HTTP write. Strategy names are not interpreted by
factory-core. The target owns the durable deployment receipt; the command does
not invent a deployment identity.

```shell
python3 -m unittest discover -s tools/tests -p test_factory_pilot_deployment.py
```

The contract uses an actual separate HTTP service and target file. It checks
scope substitution before any remote write, bounded candidate and response
bytes, candidate path escape, changed readback and a connection lost after the
target accepted the write. An unknown write response fails without retrying the
POST or publishing a successful observation. Each HTTP exchange uses the
remaining original deadline, including the response body; a slow acknowledgement
within that budget does not fail because of a separate short request timer.
The finite pilot command supports an unauthenticated hermetic target only and
rejects a credential selection;
other installed deployment programs must bind their own qualified credential.
The separate Rust target fixture serves the deployed candidate through its
`/live` HTTP route and persists the candidate, content and immutable receipt in
one atomically published, synced state file. Its finite target accepts one
operation: exact replay returns the same receipt, including after target
restart; another operation or a substituted candidate cannot replace it.

```shell
cargo test --locked -p octacity-release-harness --test factory_deployment_target
```

These CLI and target contracts alone do not qualify VM egress or packaged
runtime wiring. The separate [packaged deployment contract](#packaged-candidate-deployment)
below exercises those boundaries for its exact installed stage target.

## Installed deployment command qualification

Installed command source has a dedicated 64 KiB UTF-8 bound. YAML line breaks
and indentation are preserved; blank, oversized or unsupported control-bearing
source is rejected during configuration deserialization. The existing string
wire representation and task digest for unchanged source are preserved, while
ordinary human-readable Factory text keeps its 4 KiB bound.

```shell
cargo test --locked -p octacity-server-application --lib \
  installed_command_program_accepts_bounded_source_without_changing_ordinary_factory_text
```

`FactoryDeploymentCommandAuthority` pins one immutable environment, canonical
HTTP(S) endpoint and optional separate Agent-local credential selection. Its
reference digest is domain-separated from ordinary command policies; changing
any selection requires repinning the command identity and task. The protected
`deployment-target.json` contains only `authority`, `environment`, `endpoint`
and `credential_profile` (a logical name or `null`). The existing frozen
`promotion-input.json` remains a separate document.

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-server-application --lib \
  explicitly_pinned_deployment_command_qualifies_without_widening_builtin_commands
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-server-application --lib \
  deployment_compiler_rejects_broader_node_permissions_with_the_same_frozen_task_pin
```

The public input publisher verifies the actual four-document compilation and
closed managed-program proof. Negative contracts reject substituted selections,
unpinned authority, noncanonical credential-bearing URLs, an ordinary command
role, a different admitted environment and broader effective Node permissions.
This concrete installed adapter qualifies exactly one environment per action;
the core Promotion Plan still supports multiple environments. Existing owner
readiness and authorization remain required. This application contract alone proves qualification and compilation. Actual
deployment effects, the selected Agent/backend network path and post-deployment
measurements are checked by the separate packaged deployment contract below.

The ordinary materializer signs the four generated documents plus the two
independently verified retained candidate inputs. The output observer uses the
same closed installed-role catalog for both Job attribution and semantic
reconstruction. It rejects a changed frozen `BuildPolicySnapshot` toolchain or
Job Octa inventory, including before charging terminal usage. Legacy builtin,
author and validator qualifications retain their separate restrictions.

```shell
cargo test --locked --jobs 2 -p octacity-server-application --lib \
  retained_deployment_reconstructs_the_qualified_target_and_plan_at_both_verification_boundaries
cargo test --locked --jobs 2 -p octacity-server --lib \
  native_deployment_command_installs_only_its_exact_action_capture_and_approved_inventory
cargo test --locked --jobs 2 -p octacity-server --lib \
  file_deployment_authority_recomputes_the_policy_pin_before_native_installation
```

The Native and FILE installation contracts reach the qualified ordinary Build
ports, preserve the exact action/plugin/capture requirements, and reject
unrepinned target selections. These installation tests perform no provider I/O;
the separate packaged deployment contract exercises an external target. Neither
installation tests nor that scoped journey establish the complete pilot.

## Qualified readonly reviewer Jobs

An optional installed CodeX `reviewer` policy pins one independent evaluator and
changes the immutable task identity. Its absence preserves existing task pins.
The role requires readonly candidate inputs and rejects capture, tools and
reader quotas. Whole `EvaluationPlan` and `EvidenceManifest` values must be
explicitly projected into the frozen input from their exact committed journal
records, with matching typed discovery and a selected evaluator. Criterion-pack
bytes must be explicitly selected as immutable Policy context.

The materializer and retained output observer use the same journal-qualified
invocation and compiler. The ordinary Job receives four generated protected
documents plus the independently verified candidate bundle and manifest. Its
model schema is the adapter's existing EvaluationV1 schema. The model cannot
supply an Assessment identity or provenance: the server creates the immutable
`Assessment` only after checking all result, provenance and trace records against
the executed Job and exact prompt. The common Node owner retains its typed
Assessment discovery atomically with completion; replay creates no second Build.
Retained observation checks the original invocation at its frozen start and
actual model timing within the original Node/Plan window, without renewing
execution authority.

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-server-application --lib \
  factory_codex_output_source_tests::review
cargo test --locked --jobs 2 -p octacity-server-application --lib \
  review_invocation_qualifies_only_the_full_selected_plan_evidence_and_installed_evaluator
cargo test --locked --jobs 2 -p octacity-server-application --lib \
  qualified_common_reviewer_records_produce_server_owned_assessment_and_reject_a_model_authored_assessment
```

These public application contracts reuse the capture journal and ordinary
Artifact publication lifecycle. They cover default-denied invocation recovery,
exact readonly dispatch, server-owned Assessment projection and owner replay.
The packaged contract additionally runs the role through actual Server/Agent
processes, PostgreSQL and the qualified Microsandbox route. With the separately
pinned `OCTACITY_FACTORY_VERIFIER_CLI` selected as described above, run:

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-release-harness --test factory_pilot \
  packaged_original_evidence_plan_reviewer_and_decision_retain_exact_typed_authority \
  -- --ignored --exact --nocapture
```

The same admitted Run executes protected authoring, real candidate capture and
original private-test validation, then constructs its EvidenceManifest and
EvaluationPlan. A separately enrolled readonly evaluator Job produces qualified
EvaluationV1 records; the server creates the exact plan-bound Assessment and the
configured deterministic action creates an Accept Decision. The test checks all
four typed discovery projections, exact candidate/evidence/criterion/provenance
bindings, different producing Agent identities and pools, and absence of private
tests and credentials in review metadata. Its criterion Artifact is published by
the real bootstrap Job with the exact installed bytes and digest.

There are eight committed node completions and four ordinary role Builds, plus
the bootstrap Job. Server and both Agents restart with exact retained records,
publications and Build lineage preserved, without repeating a role Job. This
qualifies the packaged typed-review slice. The separate RequirementsReview and
DeployedCandidate contracts below cover further composed boundaries; this test
alone does not qualify the complete pilot.

## Packaged requirements-to-Decision journey

Use the same checksummed Server/Agent installations, qualified Microsandbox
image, PostgreSQL/object-storage environment and local Git mirror prerequisites
as the packaged candidate tests above. Select the original finite model CLI with
`OCTACITY_FACTORY_PILOT_CLI` and the separately pinned content-checking CLI with
`OCTACITY_FACTORY_VERIFIER_CLI`. This scenario does not install controlled-reader
tools, so it does not require `OCTACITY_CONTRACT_PROTECTED_READER` or
`OCTACITY_CONTRACT_CODEX_READER`. Those remain prerequisites for the separate
controlled/Shadow scenarios.

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-release-harness --test factory_pilot \
  packaged_nested_triage_and_requirements_are_retained_by_original_tests_and_review \
  -- --ignored --exact --nocapture --test-threads=1
```

The observed packaged run passed once in 319.84 seconds. It commits seventeen
node completions and eight ordinary role Builds, plus the real bootstrap Job
(nine Jobs total). Nested triage and configured requirements produce an accepted
criteria Artifact through actual publication. The accepted `criteria_artifact`
reference is checked against the producing ordinary Build's published Artifact
ID, digest, size and downloaded contents. That exact Artifact supplies the
original private-test authoring contract before CodeX changes Source and
captures its candidate. The original retained private bundle validates that
candidate; a separately enrolled Agent in its own pool then verifies only the
explicit candidate in a scoped nested Flow. The same admitted journey continues
through original EvidenceManifest, EvaluationPlan, readonly typed review,
server-owned Assessment and configured Decision.

The test preserves the actual accepted criteria Artifact identity, candidate and
original test bindings through the downstream records. Server and both Agents
restart with the same retained completions, Builds and Artifact publications;
no successful Job, receipt or outcome is seeded. The successful run log is
`/tmp/octacity-165-packaged-prelude-input-contract-runtime.log`. This scenario
does not include external delivery or deployment, and is not a complete-pilot
qualification.

## Packaged candidate deployment

Use the same packaged environment and finite CLI selections above, plus
`OCTACITY_FACTORY_GITHUB_PLUGIN` pointing to the separately checksummed native
Github provider with its qualified private-CA configuration and single API
version header. The fixture installs operator-owned policy and credentials;
repository content cannot select these pins. As with RequirementsReview, this
scenario uses no controlled-reader tools. Current local operator builds must be
identified by their actual checksums/provenance rather than described as an
unchanged published release.

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-release-harness --test factory_pilot \
  packaged_candidate_is_delivered_deployed_and_observed_through_its_exact_policy \
  -- --ignored --exact --nocapture --test-threads=1
```

The observed packaged run passed once in 224.99 seconds, with nine committed
node completions and five ordinary role Builds plus bootstrap (six Jobs total).
It runs actual private-test authoring, CodeX candidate capture, original-test
validation and nested separate-Agent verification before the real Git/HTTPS
delivery recovery described above. An accepted review survives the intentional
owned Server crash and restart with exactly one review POST. The installed
promotion command then deploys the readonly captured candidate to a separate
HTTP target, followed by its configured post-deployment criteria gate.

Deployment has its own immutable ordinary Build Configuration: its restricted
runtime network and Factory grants both select exactly
`host.microsandbox.internal`. The packaged Agent and qualified Microsandbox
backend execute this allowed host-alias path; the fixture does not reuse the
author's disabled-network configuration or substitute a gateway IP. The static
program pins one stage-only authority, environment, endpoint and strategy, with
no deployment credentials or production allocation.

The target receives the actual candidate file bytes and owns the durable
receipt. Two verified matching reads bracket the configured 100 ms observation
window. The retained PromotionObservation must report `passed` and exactly
`matching_reads: 2`, bind the captured CandidateSubject and installed environment,
and match the input digest independently read from its public Node Attempt
diagnostic. The test downloads the actual observation Artifact, checks `/live`
returns the candidate's `facts` bytes, and compares the target's retained
candidate and content digest. These checks repeat after the final normal Server
and both-Agent restart, alongside unchanged retained records, Builds and
publications. No deployment receipt or completion is inserted by the test.

The successful run log is
`/tmp/octacity-165-packaged-deployment-diagnostic-runtime.log`. This qualifies the
exact packaged stage deployment and readback path, not arbitrary network egress,
remote Git TLS, production rollout or all deployment providers. The composed
journey with both Shadow signals, optional research/reordering/removal and exact
audit continuity is qualified separately in the final section below.

## Ordinary admission and fresh observation time

Ordinary Build admission can complete after a Factory owner pass sampled its
observation time. `FactoryNodeBuildAdapter` now validates the durable acceptance
receipt and returns `Waiting` with that exact execution. It does not immediately
observe newly created state using the older entry time. The next fenced owner
pass observes the retained Build with its own fresh time and stable operation
identity; it does not create another Build or manufacture a later timestamp.

The public adapter test advances its controlled clock from 31 to 32 during
`create_factory_build`, persists the actual admission time of 32, and requires
`Waiting` with the accepted Build, Attempt and Job identities. A reconstructed
adapter still rejects an existing Build when observed at 31. Observation at 32
returns the same execution, with exactly one admission request. Existing
ownership, deadline and future-state timestamp guards remain in force.

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-server-application --lib \
  admission_time_advancing_during_create_returns_waiting_with_the_accepted_execution

CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-server-application --lib \
  factory_node_build_tests
```

The first test produced an actual behavioral Red: admission completed at 32, but
the adapter returned `InvalidInput` instead of retaining the pending execution.
The observed evidence is `/tmp/octacity-165-admission-observation-clock-red.log`
and `/tmp/octacity-165-admission-observation-clock-green.log` (one test passed).
The expanded test, including the stale-time rejection and single-admission
assertions, also passed in the complete 29-test adapter regression recorded in
`/tmp/octacity-165-admission-observation-regression-green.log`.

Final regression commands, using the same debug/incremental settings above:

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-server-application --lib -- --test-threads=2

CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-server-application \
  --test manual_trigger --test factory_artifact_reader --test factory_observability \
  -- --test-threads=2

CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo clippy --locked --jobs 2 -p octacity-server-application -p octacity-server \
  -p octacity-release-harness --all-targets --all-features -- -D warnings
```

One complete Application run passed all 382 tests in 921.20 seconds
(`/tmp/octacity-165-admission-observation-app-bounded-final.log`). The three
integration targets passed 42 tests: manual triggers 34, Artifact reader 5 and
observability 3 (`/tmp/octacity-165-admission-observation-integration-final-green.log`).
Strict Clippy passed in 39.90 seconds
(`/tmp/octacity-165-admission-observation-strict-final.log`). The initial manual
trigger run passed 33/34: its builtin-command fixture still declared only shell
instead of the exact approved codex+shell plugin inventory. Aligning those
test-only literal pins preserved its phantom-executable and secret assertions;
no production qualification guard changed for that correction.

This qualifies the admission-to-observation boundary. The final section records
the separate successful composed packaged journey and its retained proofs.

## Optional-parent and audit fixture contracts

The lightweight reordered/removed Prelude probes publish new versions of the
same parent Flow. Their initial parent already declares the accepted-input
schema. The fixture extension must reuse that exact schema, append only missing
contracts, and reject a conflicting body under an existing reference. Core's
unique-schema validation remains unchanged.

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-release-harness --test factory_pilot \
  prelude_schema_extension_preserves_an_existing_accepted_contract -- --test-threads=2

CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-release-harness --test factory_pilot \
  published_prelude_root_uses_the_public_content_addressed_reference -- --test-threads=2

CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-release-harness --test factory_pilot \
  -- --test-threads=2
```

The meaningful fixture Red exercised public `FactoryFlowConfiguration::new`:
the repeated accepted schema caused `InvalidConfiguration` for
`admitted Flow data schemas`. It is recorded in
`/tmp/octacity-165-prelude-schema-extension-behavior-red.log`; the earlier
`schema-extension-red.log` compile/API failure is not behavioral evidence.
`/tmp/octacity-165-prelude-schema-extension-green.log` records four fast fixture
tests passing, with the eleven packaged environment tests still ignored.

The first combined actual run completed A's twenty-three nodes and twelve Jobs
(including bootstrap), and passed its assertions before the variant probes, but
failed on the lightweight parent replacement returning HTTP 400. That entire
run failed after 648.82 seconds. Its corrected rerun
(`/tmp/octacity-165-unique-prelude-schemas-runtime.log`) completed A's same
twenty-three nodes and twelve Jobs and published B's parent version, then failed
after 489.60 seconds in a fixture assertion reading top-level definition IDs.

A second meaningful fixture Red serializes the public
`PublishedFactoryFlowDefinition` DTO and requires its exact root definition.
Definitions store their full content-addressed identity under `reference`;
matching that complete reference, including digest, fixes the assertion without
changing product wire contracts. Evidence is
`/tmp/octacity-165-public-prelude-root-red.log` and
`/tmp/octacity-165-public-prelude-root-green.log` (five fast tests passed, eleven
packaged tests ignored). The same all-targets/all-features strict Clippy command
above passed again in 16.47 seconds
(`/tmp/octacity-165-public-root-reference-strict.log`).

That corrected actual run (`/tmp/octacity-165-public-root-reference-runtime.log`)
completed all A/B/C variants: 39 completions and 18 ordinary Jobs including
bootstrap, followed by the final Server and both-Agent restart. It still failed
after 698.68 seconds because the fixture required equality of the whole audit
list. The original 195 facts and all three FlowRun references remained exact;
seven legitimate `factory.execution_input_deleted` facts had been appended by
asynchronous expired-input cleanup.

The public-DTO fixture test
`completed_run_audit_preserves_facts_when_retained_input_cleanup_appends` now
requires every original audit identity to retain its exact whole fact, while
allowing new unique facts and keeping FlowRun references strictly equal. Its
positive append case failed under the old assertion
(`/tmp/octacity-165-append-only-audit-red.log`). The same fast-test command above
passed six tests, including missing/mutated/duplicate fact and changed Flow
reference negatives (`/tmp/octacity-165-append-only-audit-green.log`). This changes
only the fixture assertion; product audit and retention behavior are unchanged.

The corrected combined actual run passed in 694.84 seconds, with all final
post-restart checks (`/tmp/octacity-165-append-only-audit-runtime.log`). The Cargo
commands above reproduce the fixture contracts; the final section records the
actual preserved-binary invocation used for the successful long run.

## Composed packaged 16.5 qualification

The composed contract passed once: one test passed, zero failed or ignored, in
694.84 seconds. It uses checksummed current operator Server/Agent builds,
released Octa and the qualified Microsandbox image, PostgreSQL, retained object
storage and the existing finite CLI/provider installations described above.
The private runner loads that pilot environment. The actual invocation was:

```shell
env \
  OCTACITY_CONTRACT_PROTECTED_READER=/private/tmp/of-reader-linux-avtgmjws/output/octacity-protected-reader \
  OCTACITY_CONTRACT_CODEX_READER=/private/tmp/of-reader-linux-avtgmjws/output/octacity-codex-reader \
  OCTACITY_FACTORY_VERIFIER_CLI=/private/tmp/octacity-165-cli-content/finite-cli \
  OCTACITY_FACTORY_GITHUB_PLUGIN=/private/tmp/octacity-165-delivery-provider-single-header/octacity-github \
  python3 /tmp/octacity-165-run.py \
  /tmp/octacity-165-append-only-audit-runtime.log \
  /tmp/octacity-165-preserved/factory-pilot-append-only-audit \
  --ignored --exact \
  packaged_nested_criteria_drive_private_tests_before_review_delivery_and_promotion \
  --nocapture
```

To reproduce from source with the same qualified environment and explicit
reader, bridge, verifier and provider selections:

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 -p octacity-release-harness --test factory_pilot \
  -- --ignored --exact \
  packaged_nested_criteria_drive_private_tests_before_review_delivery_and_promotion \
  --nocapture --test-threads=1
```

Run A executes Research then Requirements and commits 23 node completions and
12 ordinary Jobs including bootstrap. Actual accepted criteria drive private
test authoring, candidate capture, original-test validation, nested verification
on a separately enrolled Agent, EvidenceManifest, EvaluationPlan, qualified
Assessment and deterministic Decision. Routing and ToolRisk Shadow recommendations
both disagree with their authorized baseline; the retained receipts preserve
that baseline. JEV credentials remain Server-only, and the tool Job reads the
original Source through the lease-owned protected reader.

The same journey publishes the captured candidate through actual bare Git and
HTTPS review, recovers an accepted response across an owned Server crash without
another review POST, deploys actual candidate bytes to its installed stage
target, and checks bounded readback. On the same live Server and Agents, B
reorders Requirements then Research (9 completions, 4 Jobs), while C removes both
optional phases (7 completions, 2 Jobs). All three use versions 1, 2 and 3 of the
same parent Flow identity. B retains Requirements as its exact criteria Artifact
producer; C selects Classification. Only declared readonly roles create Jobs.

The total is 39 completions and 18 ordinary Jobs including one bootstrap.
Publication of B/C and the final Server and both-Agent restart preserve exact
admitted closure bytes/references, common records, Builds, completions, Artifact
publications and both Shadow receipts, without repeating successful Jobs.
Public paged audit checks retain every original whole fact and exact root/nested
Flow provenance while allowing legitimate asynchronous cleanup facts to append.
All final delivery, deployment, reviewer, variant and receipt assertions passed.

This qualifies the configured finite pilot with real packaged execution and
hermetic Git/HTTPS/stage effects. Model responses and external provider endpoints
are explicit finite fixtures. It does not qualify remote Git TLS, production
rollout, arbitrary providers or a different backend/image/toolchain installation.
