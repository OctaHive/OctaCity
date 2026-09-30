# OctaCity

OctaCity is a build-coordination server and self-hosted agent for
[Octa](https://github.com/OctaHive/octa). It turns versioned pipelines into
durable builds, assigns signed jobs to registered agents, and keeps build
state, logs, artifacts, and cache authority consistent across retries and
restarts.

OctaCity is designed for teams that want to run their own build infrastructure
without giving repository workloads access to the control plane.

## What OctaCity provides

- a PostgreSQL-backed control plane with a versioned management REST API;
- durable manual, scheduled, webhook, and upstream-build triggers;
- signed job specifications, fenced leases, idempotent retries, and recovery
  after server or agent restarts;
- self-hosted agents for Linux, macOS, and Windows;
- direct host execution plus isolated and virtualized execution providers;
- S3-compatible artifact storage, redacted build-log search, retention holds,
  and a remote build cache;
- explicit extension boundaries for source, VCS, and webhook adapters.

## How it fits together

```text
operators and VCS providers
           |
           v
   +------------------+       +------------+
   | OctaCity server  |------>| PostgreSQL |
   | REST + triggers  |------>| S3 storage |
   +------------------+       +------------+
           |
       signed jobs
           |
           v
   +------------------+       +-------------+
   | self-hosted agent|------>| Octa runner |
   +------------------+       +-------------+
```

The server owns coordination and durable state. Agents acquire one fenced
lease at a time, materialize an exact source revision, start the verified Octa
runner through the selected execution provider, and report events and outputs
back to the server.

## Execution modes

Jobs request a security mode and guarantees, not a provider-specific backend.
The agent admits a job only when its configured provider can satisfy that
contract.

| Mode | Linux | macOS | Windows |
| --- | --- | --- | --- |
| **Host** — direct execution for trusted repositories | Supported | Supported | Supported |
| **Isolation** — process or platform isolation | containerd | Apple Virtualization.framework | Not available |
| **Virtualization** — VM-backed guest execution | Microsandbox | Microsandbox on Apple Silicon | WHP preview, not release-qualified |

Linux Native remains available as a legacy execution backend. Privileged
providers require host-specific provisioning; compiling one is not treated as
evidence that its isolation boundary works. See
[real backend contract tests](docs/backend-contract-tests.md) for the qualified
platform matrix and its test cadence.

## Getting started

### Build from source

The workspace uses Rust 1.98.1 and requires `protoc`. Linux builds also require
the `libcap-ng` development package. Octa is a separate repository and must be
checked out beside OctaCity at the revision pinned by this repository:

```shell
git clone https://github.com/OctaHive/OctaCity.git octacity
git clone https://github.com/OctaHive/octa.git octa
git -C octa checkout "$(tr -d '\r\n' < octacity/.github/octa-source-revision)"
cd octacity
cargo test --workspace --all-features
```

Use the checked-in toolchain file rather than an arbitrary local Rust version.
Some real-backend and infrastructure tests are intentionally opt-in because
they need a hypervisor, container runtime, PostgreSQL, S3-compatible storage,
or other privileged services.

### Run the server

A server deployment needs PostgreSQL, an S3-compatible bucket, signing and
enrollment keys, and a strict TOML configuration. Start from one of the
CI-validated deployment examples and replace every sample address, path, and
credential reference:

- [trusted reverse proxy](docs/server.reverse-proxy.example.toml) — recommended;
- [direct trusted network](docs/server.trusted-network.example.toml).

Then validate the configuration before opening listeners:

```shell
cargo run -p octacity-server -- validate /etc/octacity/server.toml
cargo run -p octacity-server -- run /etc/octacity/server.toml
```

The management API in v1 has no operator authentication and must remain on a
trusted operator network. The server does not terminate TLS itself; use a
trusted reverse proxy or an encrypted service network. Read the
[server operations guide](docs/operations/server.md) before deployment.

### Install an agent

Agents are normally installed from a verified release archive under a
dedicated service account. Choose the platform example, configure only the
execution providers the host is prepared to offer, and validate the complete
installation before starting the service:

- [Linux example](docs/agent.example.toml);
- [Linux ARM64 example](docs/agent.linux-arm64.example.toml);
- [macOS example](docs/agent.macos.example.toml);
- [Windows example](docs/agent.windows.example.toml).

```shell
octacity-agent validate /etc/octacity/agent.toml
octacity-agent run /etc/octacity/agent.toml
```

Enrollment, release verification, systemd, launchd, Windows Service Control
Manager, runtime privileges, upgrades, rollback, and removal are covered in the
[agent operations guide](docs/operations/agent.md).

## Documentation

### Use and operate OctaCity

- [Management REST v1 workflow](docs/management-rest-v1.md)
- [Server operations](docs/operations/server.md)
- [Agent operations](docs/operations/agent.md)
- [Backup and restore](docs/operations/backup-restore.md)
- [Schema migrations and rollback](docs/operations/schema-migrations.md)

### Understand and extend it

- [Domain vocabulary](CONTEXT.md)
- [Server architecture and ownership](docs/architecture/server.md)
- [Agent architecture and implementation plan](docs/agent-implementation-plan.md)
- [Public extension seams](docs/architecture/extension-seams.md)
- [Wire protocol index](docs/protocols/README.md)

### Security and release evidence

- [Server threat model](docs/architecture/threat-model.md)
- [Security release gates](docs/security-testing.md)
- [Real execution-backend contracts](docs/backend-contract-tests.md)

## Development

The portable quality gate is:

```shell
cargo fmt --all -- --check
python3 tools/check_architecture.py
python3 -m unittest discover -s tools/tests -v
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D missing-docs" cargo doc --workspace --all-features --no-deps
cargo test --workspace --all-features
```

Coverage, security, failure, lifecycle, performance, and privileged backend
gates have different infrastructure requirements and cadences. Their canonical
commands and GitHub Actions triggers are documented in
[backend contract tests](docs/backend-contract-tests.md) and
[security testing](docs/security-testing.md).

The repository is organized by responsibility:

- `server/` — control-plane domain, application, API, and infrastructure code;
- `agent/` — agent lifecycle, source acquisition, execution, and output code;
- `shared/` — protocols and observability shared across process boundaries;
- `docs/` — public contracts, configuration examples, and runbooks;
- `tools/` — release, validation, and retained-evidence tooling.

## License

OctaCity is licensed under the [MIT License](LICENSE).
