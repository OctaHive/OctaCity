# Real execution-backend contract tests

The portable workspace suite tests orchestration with an in-memory backend.
The retained backend contract suite additionally runs a real signed job, real
`octa-runner`, and the same Octafile through Native, containerd OCI process
isolation, and Microsandbox OCI hypervisor execution. These tests are `ignored`
because cgroup delegation, containerd, and a microVM runtime cannot be emulated
honestly on an arbitrary developer machine. They never skip after an operator
explicitly selects one: absent or invalid provisioning fails the test.

The manual `backend contracts` GitHub Actions workflow assigns each exact test
to a runner with the required kernel or hypervisor. Linux Native runs on a
fresh GitHub-hosted `ubuntu-24.04` VM; the job downloads and verifies the
pinned Octa release, delegates an isolated cgroup-v2 subtree, and mounts
separate bounded loopback filesystems for workspaces and cache. Containerd and
Apple Silicon Microsandbox remain explicit self-hosted suites because their
runtime requirements are not available on standard hosted runners. The
portable CI matrix remains independent of privileged host configuration.

Each Native test command enters a dedicated sibling `runner` cgroup before it
starts the Agent. The Agent can therefore move only its runner descendants into
the clean `jobs` subtree without receiving authority over the VM's root cgroup.

Select the default `linux-native` suite to run the full released-product Native
matrix without owning a runner. The workflow creates all privileged Linux
state on the disposable VM and removes it in an `always()` cleanup step; the
VM is discarded after the job as an additional boundary.

The combined `released-agent` suite also schedules macOS Microsandbox. To run
that additional slice, prepare one Apple Silicon macOS host and execute
`tools/runner/register-backend-runner.sh` in a separate terminal. The wizard
executes the real backend preflight, opens the repository registration page,
registers an ephemeral one-job runner, and opens the workflow page. The
one-hour GitHub registration token is read without echo and is never written
to disk. Because self-hosted runners execute repository code, use this
procedure only for a trusted revision and do not enable it for unreviewed
public pull requests.

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

The Linux Native and Apple Silicon macOS Microsandbox jobs build deterministic
server and Agent release-candidate archives and use
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
a scheduled occurrence through remote-cache reuse and a separately cancelled
Build. The gate reads Build Results, performs full-text and literal log
searches, downloads and hashes an artifact, and checks ordered lifecycle and
resource events, causal linkage, duplicate suppression, metrics, the active
cgroup's CPU, memory, swap, and process limits, cgroup cleanup, and workspace
cleanup. A private loopback TLS proxy fronts the
server's cache ingress so the released runner uses the production HTTPS and CA
contract. The workflow retains its installation receipt, release manifests,
server and Agent logs, REST evidence, and Prometheus snapshot.

The macOS job keeps the smaller released-product vertical slice until its full
Microsandbox matrix is added in task 9.4. It executes a Linux guest through
Microsandbox; host-native execution is deliberately Linux-only.

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
The release gate creates an isolated L1 directory for each Agent under that
mount. Agent A publishes the fixture result to the server-backed L2; Agent B
starts with an empty L1 and must observe a cache hit. The per-Agent directories
also prevent a local hit from masquerading as remote-cache reuse.

## Microsandbox

Use a Linux (`x86_64` or `aarch64`) or Apple Silicon macOS host supported by the
pinned Microsandbox SDK. Intel macOS is unsupported. The image must be an
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
export OCTACITY_CONTRACT_MICROSANDBOX_IMAGE='registry.example/build@sha256:<64-lowercase-hex>'
export OCTACITY_CONTRACT_MICROSANDBOX_ALLOWED_HOST=allowed.contract.example
export OCTACITY_CONTRACT_MICROSANDBOX_DENIED_HOST=denied.contract.example
cargo test -p octacity-job --test backend_contract \
  microsandbox_backend_satisfies_the_real_runner_contract -- --ignored --exact --nocapture
```

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
export OCTACITY_CONTRACT_CONTAINERD_WORK_ROOT=/var/lib/octacity-contract/containerd-work
export OCTACITY_CONTRACT_CONTAINERD_STATE_ROOT=/var/lib/octacity-contract/containerd-state
export OCTACITY_CONTRACT_CONTAINERD_IMAGE='registry.example/build@sha256:<64-lowercase-hex>'
# Optional: export OCTACITY_CONTRACT_CONTAINERD_REGISTRY_CONFIG_DIR=/etc/containerd/certs.d
cargo test -p octacity-job --test backend_contract \
  containerd_process_engine_satisfies_the_real_runner_contract -- --ignored --exact --nocapture
```

Each test verifies signed runtime and isolation selection, real bidirectional
runner JSONL, structured events, terminal resource accounting, graceful
cancellation of a second long-running job, workspace removal after both jobs,
and a final orphan-cleanup pass. It also prints end-to-end latency for the
success-and-cancel contract. The dedicated release pipeline should run all
commands and preserve their output as the backend performance baseline. The
Microsandbox variant additionally verifies the fixed workload-identity mount
and allows/denies real network probes according to its restricted egress
policy; the service-backed Phase 6 contract proves the actual Vault login and
secret-redaction path.

## Including a real backend in Linux coverage

Portable coverage deliberately leaves the privileged backend tests ignored.
On a provisioned Linux worker, accumulate the normal suite and the selected
strict backend contract into one report instead of excluding backend code:

```shell
cargo llvm-cov clean --workspace
cargo llvm-cov --workspace --all-features --no-report
cargo llvm-cov --no-clean --all-features -p octacity-job --test backend_contract \
  -- containerd_process_engine_satisfies_the_real_runner_contract \
  --ignored --exact --nocapture
cargo llvm-cov report --summary-only \
  --ignore-filename-regex '[/\\]\.cargo[/\\]registry[/\\]|octa-runner-protocol[/\\]src[/\\]lib\.rs$'
```

Use the corresponding exact Native or Microsandbox test name on workers for
those backends. This keeps production adapters in the coverage denominator and
measures them with their actual kernel/runtime boundary rather than mocks.
