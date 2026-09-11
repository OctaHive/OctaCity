# Real execution-backend contract tests

The portable workspace suite tests orchestration with an in-memory backend.
The retained backend contract suite additionally runs a real signed job, real
`octa-runner`, and the same Octafile through Native, containerd OCI process
isolation, and Microsandbox OCI hypervisor execution. These tests are `ignored`
because cgroup delegation, containerd, and a microVM runtime cannot be emulated
honestly on an arbitrary developer machine. They never skip after an operator
explicitly selects one: absent or invalid provisioning fails the test.

The manual `backend contracts` GitHub Actions workflow assigns each exact test
to a labeled, provisioned self-hosted runner. It is the release gate for real
kernel and hypervisor behavior; the portable CI matrix remains independent of
privileged host configuration.

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
export OCTACITY_CONTRACT_NATIVE_CGROUP_ROOT=/sys/fs/cgroup/octacity-contract
export OCTACITY_CONTRACT_NATIVE_WORK_ROOT=/var/lib/octacity-contract/native
export OCTACITY_CONTRACT_NATIVE_BWRAP=/usr/bin/bwrap
export OCTACITY_CONTRACT_NATIVE_PATH=/usr/local/bin:/usr/bin:/bin
cargo test -p octacity-job --test backend_contract \
  native_backend_satisfies_the_real_runner_contract -- --ignored --exact --nocapture
```

## Microsandbox

Use a Linux (`x86_64` or `aarch64`) or Apple Silicon macOS host supported by the
pinned Microsandbox SDK. Intel macOS is unsupported. The image must be an
immutable OCI digest containing the runtime libraries needed by the installed
Linux Octa release. The work and state roots must be separate, operator-owned
directories.

```shell
export OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT=/absolute/path/to/micro-work
export OCTACITY_CONTRACT_MICROSANDBOX_STATE_ROOT=/absolute/path/to/micro-state
export OCTACITY_CONTRACT_MICROSANDBOX_EXECUTABLE=/opt/microsandbox/bin/msb
export OCTACITY_CONTRACT_MICROSANDBOX_LIBKRUNFW=/opt/microsandbox/lib/libkrunfw.so
export OCTACITY_CONTRACT_MICROSANDBOX_IMAGE='registry.example/build@sha256:<64-lowercase-hex>'
cargo test -p octacity-job --test backend_contract \
  microsandbox_backend_satisfies_the_real_runner_contract -- --ignored --exact --nocapture
```

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
commands and preserve their output as the backend performance baseline.

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
