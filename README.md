# OctaCity

OctaCity is the control plane and self-hosted agent for running
[Octa](https://github.com/OctaHive/octa) jobs. The repository is at the first
agent implementation milestone; no production daemon or server is available
yet.

The current workspace contains:

- `octacity-protocol`: strict, signed server-agent job types;
- `octacity-agent`: strict configuration and Octa release validation, trusted
  source-plugin supervision, a backend-neutral runner supervisor, and the
  Linux cgroup-v2 `NativeBackend`;
- `octacity-source-plugin`: the bounded pre-Octa source protocol;
- `octacity-source-git`: exact, detached Git revision materialization through
  the source-plugin protocol.

Run all checks with:

```shell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo llvm-cov --workspace --all-features --summary-only
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
An operator-facing configuration shape is available in
[`docs/agent.example.toml`](docs/agent.example.toml).
