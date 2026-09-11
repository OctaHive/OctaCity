# OctaCity

OctaCity is the control plane and self-hosted agent for running
[Octa](https://github.com/OctaHive/octa) jobs. The repository now includes the
local-execution, coordinator-transport, and durable job-lifecycle milestones.
No server is available yet.

The current workspace contains:

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
- `octacity-job`: verified source-to-runner lifecycle and cleanup owner;
- `octacity-lifecycle`: fenced lease ownership, append-only event spooling,
  replay, completion, and interrupted-attempt recovery;
- `octacity-runner`: verified Octa inventory, runner protocol, and supervision;
- `octacity-source`: trusted source-plugin registry and process host;
- `octacity-source-plugin`: the bounded pre-Octa source protocol;
- `octacity-source-git`: exact, detached Git revision materialization through
  the source-plugin protocol;
- `octacity-agent`: the CLI and composition root that wires these components.

Run all checks with:

Clean builds require `protoc` because `containerd-client` generates its Rust
gRPC bindings during compilation. CI installs the compiler explicitly on every
supported runner OS.

```shell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo llvm-cov --workspace --all-features --summary-only \
  --fail-under-lines 80
```

CI pins `cargo-llvm-cov` 0.9.1 so coverage semantics cannot change when a new
tool release appears. The 67% portable Linux floor measures production source;
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
Language-neutral wire specifications are indexed in
[`docs/protocols/README.md`](docs/protocols/README.md).
The implemented coordinator transport is specified in
[`docs/protocols/server-agent-v1.md`](docs/protocols/server-agent-v1.md).
The source-plugin process model and complete protocol v1 lifecycle are
documented in
[`crates/octacity-source-plugin/README.md`](crates/octacity-source-plugin/README.md).
An operator-facing configuration shape is available in
[`docs/agent.example.toml`](docs/agent.example.toml).
Provisioning and exact commands for the non-emulated execution contract suite
are documented in
[`docs/backend-contract-tests.md`](docs/backend-contract-tests.md).
