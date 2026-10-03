# Local Apple Silicon stand operations

This runbook operates the local development and integration stand on an Apple
Silicon Mac. One launcher owns the hybrid lifecycle: PostgreSQL, MinIO, the
OctaCity server, and the UI gateway run as Linux ARM64 containers in OrbStack,
while one OctaCity Agent and its Microsandbox provider run natively on macOS.

This stand is not a production deployment and does not provide release
qualification, operator authentication, remote access, high availability,
backups, or availability guarantees. The management API is unauthenticated and
is safe here only because nginx publishes it on the loopback interface. Do not
expose port `8443` through a LAN address, tunnel, or public proxy.

## Prerequisites

Use an Apple Silicon Mac. Intel macOS, Linux, Windows, Docker Desktop, and
Colima are outside this stand's supported host contract. The current topology
requires:

- OrbStack running its Docker-compatible Linux ARM64 engine, with Docker
  Compose available through the `docker` CLI;
- Xcode Command Line Tools, Git, Python 3, OpenSSL, `protoc`, and `curl`;
- the repository's Rust toolchain, currently Rust 1.99.0 through
  `rust-toolchain.toml`;
- outbound HTTPS access to the pinned container registries, GitHub release and
  source archives, crates.io, and the npm registry during the first build; and
- enough free disk space for Rust and UI builds, container images, PostgreSQL,
  MinIO, the native installation, Agent workspaces, and Microsandbox state.

OrbStack includes its Docker engine and Compose tooling. Follow the official
[OrbStack quick start](https://docs.orbstack.dev/quick-start) if it is not
installed. Nested KVM is unavailable on Apple Silicon, so Microsandbox must
remain native; do not try to move the Agent into OrbStack or substitute an
unisolated provider. See OrbStack's
[nested virtualization note](https://docs.orbstack.dev/machines/#nested-virtualization).

Run these checks from the repository root:

```shell
test "$(uname -m)" = arm64
xcode-select -p
docker version --format '{{.Client.Version}} {{.Server.Version}} {{.Server.Os}}/{{.Server.Arch}}'
docker compose version
rustc --version
cargo --version
protoc --version
python3 --version
openssl version
git --version
```

The Docker server result must end in `linux/arm64`. Start the OrbStack app if
the Docker API is unavailable. The launcher stages and verifies the pinned
Microsandbox 0.7.6 binary and firmware itself; a separate global `msb`
installation is neither required nor used.

## State and source revision

The default private state root is:

```text
~/Library/Application Support/OctaCity/local-stand
```

All commands below use that default. To use another location, choose an
absolute path outside the repository and export it before the first command:

```shell
export OCTACITY_LOCAL_STAND_ROOT="/absolute/private/path/octacity-local-stand"
```

The launcher rejects symbolic, repository-owned, home-directory, root, and
otherwise broad state paths. It creates accepted state directories with mode
`0700` and private files with mode `0600`.

Microsandbox runtime state uses a deterministic owner-only directory named
`/private/tmp/ocm-<uid>-<stand-hash>`. The short path is required by the macOS
Unix-socket limit; `reset` reports and removes it together with the configured
state root and named volumes. Ordinary `down` preserves it for the next `up`.

Build inputs are taken from the committed Git `HEAD`, not from uncommitted
working-tree files. Commit the revision that is meant to run before starting
the stand. The first `up` builds and downloads pinned artifacts; later starts
reuse verified images, archives, and native installations when their identities
still match.

## Start, inspect, and stop

Start or converge the complete stand:

```shell
tools/local-stand up
```

The command returns only after Compose health checks pass, bootstrap converges,
the native `msb doctor` preflight succeeds, and the expected Agent registers as
online. Repeating `up` is supported and must not create another Pool or Agent.

Inspect the combined container and native state:

```shell
tools/local-stand status
```

A usable stand reports `state` and `agent` as `running`,
`agent_registered` as `true`, and all long-running Compose services in both
the `running` and `healthy` arrays. `partial` means the stand is not usable;
inspect logs before retrying.

Show a bounded tail from Compose, the native Agent, and Microsandbox preflight:

```shell
tools/local-stand logs --tail 200
```

The permitted tail is between 1 and 10,000 lines. Logs are designed not to
contain credentials or presigned URLs, but still review them before sharing
because repository commands can emit project-specific content.

The gateway records normalized paths without query strings. Nginx diagnostics
for the cache and object-transfer hosts are restricted to process-critical
events because lower-level proxy diagnostics can embed complete presigned URLs.
Management and Agent hosts retain the normal diagnostic level.

Stop the exact native Agent process group and the Compose project while
preserving PostgreSQL, MinIO, enrollment, Agent, cache, and Microsandbox state:

```shell
tools/local-stand down
```

`down` is idempotent. A later `up` reuses the durable state and is the normal
restart procedure.

## URLs and local CA

The gateway uses one fixed loopback TLS port:

| Surface | URL | Purpose |
| --- | --- | --- |
| Console and management | `https://octacity.localhost:8443/` | UI and same-origin management API |
| Liveness | `https://octacity.localhost:8443/health/live` | Server process is live |
| Readiness | `https://octacity.localhost:8443/health/ready` | Required stores and workers are usable |
| Agent protocol | `https://agent.localhost:8443/` | Native Agent only |
| Remote cache | `https://cache.localhost:8443/` | Authorized runners only |
| Object transfer | `https://objects.localhost:8443/` | Presigned artifact transfers only |

PostgreSQL, MinIO, and raw server listeners have no host-published ports. The
protocol origins are not operator web pages.

Use the generated public CA without modifying Keychain trust:

```shell
CA_CERT="${OCTACITY_LOCAL_STAND_ROOT:-$HOME/Library/Application Support/OctaCity/local-stand}/pki/local-ca.pem"
curl --fail --silent --show-error --cacert "$CA_CERT" \
  https://octacity.localhost:8443/health/live
curl --fail --silent --show-error --cacert "$CA_CERT" \
  https://octacity.localhost:8443/health/ready
curl --fail --silent --show-error --cacert "$CA_CERT" \
  --output /dev/null https://octacity.localhost:8443/
```

Browser trust is optional. To trust only this generated public CA in the
current user's login Keychain, run:

```shell
security add-trusted-cert -r trustRoot -p ssl \
  -k "$HOME/Library/Keychains/login.keychain-db" "$CA_CERT"
```

Never import `local-ca-key.pem` or `gateway-key.pem`. Remove trust before a
destructive reset, while the public CA still exists:

```shell
CA_SHA256="$(openssl x509 -in "$CA_CERT" -noout -fingerprint -sha256 | cut -d= -f2 | tr -d ':')"
security delete-certificate -t -Z "$CA_SHA256" \
  "$HOME/Library/Keychains/login.keychain-db"
unset CA_SHA256
```

Keychain commands may prompt for the current user's approval. Restart an open
browser if it does not notice a trust change immediately.

## Verify readiness and a real job

The health commands above verify the gateway and server. The integration
verifier additionally checks the built UI entry point, exactly one local Pool,
exactly one online macOS ARM64 Agent, its Linux ARM64 Microsandbox target, a
successful Build and Job, and an artifact uploaded to and downloaded from
MinIO.

The selected commit must already be committed and fetchable from the supplied
public HTTPS GitHub repository. The receipt path must not already exist:

```shell
SOURCE_REVISION="$(git rev-parse HEAD)"
RECEIPT="${OCTACITY_LOCAL_STAND_ROOT:-$HOME/Library/Application Support/OctaCity/local-stand}/evidence/vertical-slice.json"
python3 tools/verify_local_stand_integration.py \
  --root "${OCTACITY_LOCAL_STAND_ROOT:-$HOME/Library/Application Support/OctaCity/local-stand}" \
  run \
  --source-repository https://github.com/OctaHive/OctaCity \
  --source-revision "$SOURCE_REVISION" \
  --receipt "$RECEIPT"
```

Prove that repeated convergence does not duplicate the Pool or Agent:

```shell
tools/local-stand up
python3 tools/verify_local_stand_integration.py \
  --root "${OCTACITY_LOCAL_STAND_ROOT:-$HOME/Library/Application Support/OctaCity/local-stand}" \
  observe --receipt "$RECEIPT"
```

Prove that PostgreSQL metadata and the MinIO object survive a complete ordinary
restart:

```shell
tools/local-stand down
tools/local-stand up
python3 tools/verify_local_stand_integration.py \
  --root "${OCTACITY_LOCAL_STAND_ROOT:-$HOME/Library/Application Support/OctaCity/local-stand}" \
  observe --receipt "$RECEIPT"
```

Use a new receipt filename for another verification Build. The receipt contains
resource identities and the artifact digest, not credentials, but it remains a
private mode-`0600` file below the stand root.

## Destructive reset

Ordinary `down` preserves data. To preview the exact host directory and named
volumes a reset would remove, omit confirmation:

```shell
tools/local-stand reset
```

The preview intentionally exits unsuccessfully after printing the targets and
deletes nothing. Review them, remove optional Keychain trust as described
above, and only then run the exact confirmed command:

```shell
tools/local-stand reset --confirm DELETE-OCTACITY-LOCAL-STAND
```

Reset stops the verified Agent and Compose project, removes only the known
`octacity-local_postgres-data` and `octacity-local_minio-data` volumes, and
removes the resolved stand state root. The next `up` performs a clean bootstrap.
Container images and repository build caches are not durable application state
and are not deleted by reset.

## Troubleshooting

### Docker API is unavailable

Start OrbStack and verify that the active Docker server reports `linux/arm64`:

```shell
docker version --format '{{.Server.Os}}/{{.Server.Arch}}'
docker compose version
```

Do not switch the stack to an amd64 engine; every container input is pinned for
Linux ARM64.

### Port 8443 is already in use

The host and Compose sides intentionally share fixed port `8443`; changing only
the published port would invalidate Agent, cache, and presigned object origins.
Find the listener, stop the owning application, and retry:

```shell
lsof -nP -iTCP:8443 -sTCP:LISTEN
```

### Native Microsandbox preflight fails

Inspect the bounded native logs:

```shell
tools/local-stand logs --tail 500
```

The detailed preflight is stored under
`logs/microsandbox-preflight.log` inside the private stand root. Confirm that
the host is Apple Silicon, the staged files have not been modified, and macOS
still permits native virtualization. Retry `up` after correcting the host
condition. Do not mount `/dev/kvm`, run a privileged Microsandbox container, or
enable Host, Native, containerd, or Apple VF as a fallback; those actions would
change the isolation contract.

### Startup fails after an interrupted attempt

Run `status`, inspect logs, and perform an ordinary `down` before retrying:

```shell
tools/local-stand status
tools/local-stand logs --tail 500
tools/local-stand down
tools/local-stand up
```

The launcher repairs stale ownership records without signalling an unrelated
process. Do not manually delete PID files or named volumes. Use confirmed
`reset` only when loss of all local stand data is intended.

### Browser reports an untrusted certificate

Use `curl --cacert` to distinguish trust configuration from server readiness.
If curl succeeds, either keep using the explicit CA or add the public CA to the
user Keychain. Never bypass certificate validation or trust the private key.

## Non-production limitations

- The management API has no login, session, RBAC, or per-operator identity.
- The stand is loopback-only and has no supported remote exposure.
- PostgreSQL and MinIO are single local instances with no backup automation.
- The generated CA is local and not externally trusted or automatically
  rotated.
- Resource ceilings protect individual Agent operations but are not production
  capacity planning or hard macOS filesystem quotas.
- Passing the integration verifier proves this development topology only. It
  does not replace release-candidate packaging, the Agent Ready matrix,
  production security review, disaster-recovery rehearsal, or release
  qualification.
