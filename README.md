# OctaCity

OctaCity is the control plane and self-hosted agent for running
[Octa](https://github.com/OctaHive/octa) jobs. The repository is at the first
agent implementation milestone; no production daemon or server is available
yet.

The current workspace contains:

- `octacity-protocol`: strict, signed server-agent job types;
- `octacity-config`: parsing and intrinsic validation of agent configuration;
- `octacity-execution`: backend-neutral execution ports and shared types;
- `octacity-execution-native`: Linux cgroup-v2 Native backend;
- `octacity-runner`: verified Octa inventory, runner protocol, and supervision;
- `octacity-source`: trusted source-plugin registry and process host;
- `octacity-source-plugin`: the bounded pre-Octa source protocol;
- `octacity-source-git`: exact, detached Git revision materialization through
  the source-plugin protocol;
- `octacity-agent`: the CLI and composition root that wires these components.

Run all checks with:

```shell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo llvm-cov --workspace --all-features --summary-only \
  --ignore-filename-regex 'octa-runner-protocol[/\\]src[/\\]lib\.rs$'
```

The command line is defined with `clap`. Agent internals emit structured
`tracing` events, so service managers and collectors can redirect stderr or
consume JSON without coupling the implementation to a logging backend:

```shell
octacity-agent --log-filter octacity_agent=debug --log-format json \
  validate /etc/octacity/agent.toml
```

Native execution is deliberately Linux-only. It requires a delegated cgroup
v2 root and a quota-sized dedicated filesystem mount per workspace; the agent
rejects Native execution when either resource boundary cannot be enforced.

The implementation sequence and security boundaries are documented in
[`docs/agent-implementation-plan.md`](docs/agent-implementation-plan.md).
Language-neutral wire specifications are indexed in
[`docs/protocols/README.md`](docs/protocols/README.md).
The source-plugin process model and complete protocol v1 lifecycle are
documented in
[`crates/octacity-source-plugin/README.md`](crates/octacity-source-plugin/README.md).
An operator-facing configuration shape is available in
[`docs/agent.example.toml`](docs/agent.example.toml).
