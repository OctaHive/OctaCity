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

The console virtual host imports the same routing, caching, MIME, CSP, framing,
and referrer-policy snippets as the supported production fixture. See the
[operator console deployment runbook](operator-console.md) for the shared
same-origin contract. The local certificate and loopback-only exposure remain
development conveniences, not a production topology.

Stop the exact native Agent process group and the Compose project while
preserving PostgreSQL, MinIO, enrollment, Agent, cache, and Microsandbox state:

```shell
tools/local-stand down
```

`down` is idempotent. A later `up` reuses the durable state and is the normal
restart procedure.

The private supervisor stops its own Agent process group. If graceful stop
fails, it sends SIGKILL even if the control client disconnects while receiving
the response. After an acknowledged shutdown, the launcher waits for the owner
and its live descendants to exit before removing the ownership record.

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

## Factory demonstration in one command

To start a disposable stand with Factory enabled and retained demonstration
data, run from the repository root:

```shell
tools/factory-stand
```

The command prepares the same verified native installation, images, TLS,
PostgreSQL, MinIO, and Agent as `tools/local-stand up`. It creates the owning
Project through a temporary loopback-only bootstrap API, installs a private
Factory catalog, and publishes two Flow definitions and configurations through
the management API. Two ordinary Builds produce distinct retained input
Artifacts. The resulting Factory Runs demonstrate:

- automatic completion through a nested Flow and a deterministic risk rule;
- a higher-risk Work item waiting at a human gate for an operator response.

AI settings reuse the installed plugin's model catalog and qualified Runtime transport
support. Reviewed MCP, integration and permission settings are stored in the existing
PostgreSQL journal, separately from generated installation files. Normal restart
preserves operator settings and execution history; the generated catalog upgrade adds
only support metadata and refuses to overwrite customized files. No MCP server, grant,
model preset or execution permission is enabled automatically. See
[Runtime setup and persistence](factory.md#project-ai-agent-compatibility-declarations).

No external model or decision-service credentials are needed. This demonstration
uses native control nodes; it does not run model authoring, repository changes,
or delivery. The graphs, risk condition, budgets, and cases are declared in
[`factory-demo.json`](../../deployment/local-stand/fixtures/factory-demo.json).
The script binds server-generated references without dispatching by Flow or
node name.

The same command installs the compatible real Codex CLI declared in
[`factory-runtime.json`](../../deployment/local-stand/factory-runtime.json), verifies
its release archive checksum, and publishes a Project Runtime choice for AI-agent
profile authoring. The server builds its discovery plugin from the pinned Octa
source with the same system libraries as Server; the Linux execution plugin keeps
its separate released identity. Model names and parameter choices come from the
plugin's `--describe-models` contract. Use **Refresh models** for an explicit update,
or enter a model identifier manually when the plugin allows it. A new profile
requires an explicit model choice; the technical model-contract identity is never
submitted as its default model.

In **Factory → AI agents**, open or create an agent, choose **Create profile**,
select the Codex Runtime and a model, and save a profile named `default`. Repeat
with the desired model/settings and name `pro`. These names carry no built-in
model, permission or performance meaning. The original Role/Profile REST
publication and validation contracts own both profiles.

When `CODEX_HOME/auth.json` (default `~/.codex/auth.json`) exists, setup takes a
private authentication-only snapshot for the isolated discovery Runtime. It does
not copy `config.toml`, instructions, MCP configuration or local skills. The
snapshot stays under the owner-only stand root and is copied to an executable
private tmpfs with mode `0600`. Existing snapshots are reused; absence of a login
does not prevent advisory discovery or manual model entry. Neither creating a
profile nor refreshing the catalog starts a model task or spends execution
budget. This installation adds authoring compatibility, not execution authority:
using a profile in a reasoning stage still requires the separately qualified
Codex executor, Agent credential/policy installation and ordinary Build binding
below. The demonstration itself continues to use only native control nodes.

An older, unmodified Factory demonstration gains this catalog on its next launch.
The launcher validates the previous generated bundle, prepares the replacement,
stops the stand through its ordinary lifecycle, and switches only the generated
server configuration. Database/object volumes, operator data and demonstration
receipts are preserved. It refuses to replace customized profiles; an ordinary
stand is still not converted implicitly.

Open `https://octacity.localhost:8443/` and select **Factory**. The command prints
the Project, Flow, configuration, and Run IDs and retains them in
`evidence/factory-demo.json` under the private stand root. Local CA trust is
configured as described below.

The default root and `OCTACITY_LOCAL_STAND_ROOT` match the ordinary launcher;
`--root` overrides them. By default, the input repository is the public
`OctaHive/OctaCity` repository and its current remote `HEAD` is frozen on first
use. To select another published revision explicitly:

```shell
tools/factory-stand \
  --source-repository https://github.com/OctaHive/OctaCity \
  --source-revision <published-40-character-commit>
```

Repeating the same command reuses the frozen source, immutable installation,
resources, Builds, and Runs through stable replay identities. It neither resets
the database nor creates another demonstration. Do not run two lifecycle or
demonstration commands concurrently. The stand still uses the single ordinary
Compose project and fixed port `8443`.

An already installed ordinary stand cannot be converted silently: the command
refuses to replace a server bundle it did not prepare. Preserve any needed data
and deliberately reset a disposable installation using the reset procedure
below before its first Factory demonstration. Choosing another root does not
create a second independent Compose project.

Use `tools/local-stand status`, `logs`, and `down` to operate the resulting
stand. Restart and refill it with `tools/factory-stand`; no profile files need
to be prepared by hand. The separate integration verifier remains available
for its wider ordinary CI/CD checks.

### Select an operator-owned Factory installation

To start with a complete operator-reviewed Factory catalog instead of generated
demonstration data, use the existing ordinary installer through `factory-stand`:

```shell
tools/factory-stand \
  --factory-profiles /absolute/private/factory-profiles.json \
  --factory-runtime-bundle /absolute/private/runtime \
  --factory-agent-profile /absolute/private/agent-policy.toml
```

The catalog and Agent policy use the same private file contracts as
`tools/local-stand up`. The optional Runtime bundle supplies explicitly installed
programs. Agent executable identities and independent policy are validated by the
ordinary configurator/preflight; selecting a catalog does not grant worker access.
Before the first explicit installation starts Compose, bootstrap or the Agent,
the packaged server also validates the installed catalog offline, using the same
validator as installation updates. A rejected catalog prevents service activation.
The same `--root` and `OCTACITY_LOCAL_STAND_ROOT` selection applies to this path.

This path installs the selected catalog without merging generated demo profiles,
downloading the demonstration Codex runtime or seeding demonstration Runs. A GitHub
Source connection is installed only when explicitly selected with
`--github-repository`; an MCP endpoint does not require an unrelated Source
connection. Runtime-bundle and Agent-policy flags require `--factory-profiles`.
Demo-only `--source-repository` and `--source-revision` flags cannot be combined
with the explicit catalog; the catalog and ordinary Build definitions own its
execution inputs.

Omitting these installation flags retains the original demonstration startup.
Ordinary startup still refuses to replace a different installation implicitly.

To update an existing initialized stand, explicitly review and supply its complete
catalog and independent Agent policy, then add `--update-factory-installation`:

```shell
tools/factory-stand \
  --factory-profiles /absolute/private/factory-profiles.json \
  --factory-runtime-bundle /absolute/private/runtime \
  --factory-agent-profile /absolute/private/agent-policy.toml \
  --update-factory-installation
```

`tools/local-stand up` accepts the same update flag. Both catalog and Agent policy
are required. The existing installation must be recognizable by the ordinary
configurators; edited generated files are rejected. The installer stages private
server and Agent files under the shared lifecycle lock, validates the native
Agent and the packaged server offline, and only then stops the stand. Activation
atomically exchanges the generated configuration directory. Startup failure
restores the previous configuration after stopping candidate services; it leaves
the stand stopped so the operator can retry its previous explicit setup.

Updates preserve PostgreSQL/object volumes, registration credentials, Agent work
and cache, integration authority and demonstration receipts. Optional installed
Runtime/Delivery/Signal bundles are retained when omitted from an update. Repeating
identical inputs does not stop the stand for replacement. For an ordinary later
`up`, repeat the installed catalog, Agent policy and optional bundle selections;
`down` followed by `up` needs no reset. Installation updates do not publish profile
versions, repin models/transports, change reviewed Runtime settings or replay demo
Work. Existing qualification and revocation rules still govern retained profiles.
MCP credential and capture qualification remains separate work in this change.

### GitHub connections in the personal Factory stand

The ordinary `tools/factory-stand` command also prepares integration management
and installs the demo task repository, `OctaHive/OctaCity`, on first use. Its
selection is declared in `deployment/local-stand/fixtures/factory-demo.json`,
separately from the code repository used by the demonstration Builds. Existing
Factory stands without integration setup gain it on their next launch, retaining
their Runs. Subsequent launches reuse an installed selection without resolving
or replacing it. To select another public task repository on initial setup:

```shell
tools/factory-stand --github-repository OctaHive/OctaCity
```

For task import/status synchronization without Factory demo resources or a model
runtime, use the ordinary stand entry point with an explicit public task resource:

```shell
tools/local-stand up --github-repository OWNER/REPO
```

Both commands use the same `factory_integrations.configure_integrations` seam.
Its trusted defaults live in `deployment/local-stand/source-policy.json`:
bounded Issue reads, an unrestricted native selector with editable narrowing,
optional Factory task/acceptance preparation, and separately scoped Reporter
effects/delivery bounds. The default acceptance rule reads `## Acceptance` from
the native body; missing acceptance blocks Factory admission, not task import.
Default native effects support explicit open/closed state changes. On a fresh
public-repository installation, the launcher also reads the existing GitHub label
catalogue and installs those names as selectable additions/removals. The snapshot
is limited to 128 labels, 256 UTF-8 bytes per name and a 64 KiB native mapping;
an unavailable, invalid or oversized catalogue stops installation instead of
silently installing incomplete choices. Each label has an independent conflict
scope. Installing these choices performs no write and creates no GitHub label;
configured updates preserve unrelated labels.

The label snapshot is part of the immutable installation identity. Repeated
starts reuse it offline rather than discover new labels or widen pending effects.
An existing state-only installation retains its empty label scope; its UI explains
that no label updates are available. A different label snapshot requires an
explicit new trusted installation and review of connection/source settings. Do not
edit a retained bundle in place or reset its database to refresh these choices.

The private generated catalog references `source-profiles.json`; selector/mapping
files are named by their logical identity hash and pinned by content SHA-256.
The normal server entrypoint installs them with owner-only permissions alongside
the verified adapter and retained credentials. Repeated starts validate the same
immutable files. Existing Runtime/Profile settings and Factory receipts are not
rewritten. Repeating `up --github-repository OWNER/REPO` reuses the original native
repository ID without a GitHub lookup, so provider outage does not block startup
of a compatible retained installation. Local adapter bytes and the complete
immutable bundle are still validated; another repository, changed pins or an
incomplete bundle are rejected without replacement or network fallback.

The [packaged Source runtime gate](../testing/source-runtime.md) qualifies actual
Console/nginx/Server import, reviewed status delivery and separately enabled
Factory admission against a native adapter and hermetic HTTPS provider. It records
package pins, controlled restart barriers and public canonical identities. It
performs no live repository write and qualifies no model or Agent backend. Older
metadata-only pre-release test bundles require explicit trusted regeneration;
retain old connection pins/history and review new metadata/grants. Future
consolidated enrollment remains in `simplify-integration-setup`.
The profiles contain no Project, code Repository or Factory target IDs:
current connections, Project grants and explicitly published import/input/mapping
controls resolve those independently while the server runs. Future enrollment
should reuse this installation seam and the existing protected management APIs.

This is a pre-release test stand. Earlier metadata-only integration bundles are
not silently rewritten into another native configuration: their pins differ.
Use an explicitly selected fresh disposable root to test the new installation;
keep any existing stand/data until deliberately choosing to remove it. No released
installation migration is introduced here.

The launcher builds **committed** sources. Commit a local fix before rebuilding;
editing the working tree alone does not update its images. The server image now
includes the independent `octacity-github` executable. Initial setup resolves the
public repository's native ID without authentication and installs its exact
resource ceiling. It never reads GitHub CLI sessions, local credential stores or
`GITHUB_TOKEN` to configure the connection. Private-repository identity bootstrap
is not provided by this setup.

Open **Integrations**, create a named GitHub connection and select the installed
repository/purposes. Enter your token in the separate credential form. Reading
tasks and updating them retain independent handles; remote token permissions do
not grant an OctaCity Project access. Publish Project grants separately, then
explicitly activate the connection if intended. No polling/input or task-card
creation starts merely by installing or creating a connection.

The optional overlay keeps the existing server bundle and demonstration receipts
unchanged. Its separate private `config/integrations` bundle is immutable and
reused on subsequent starts. A mismatched installation is rejected rather than
overwritten. `tools/local-stand status`, `logs`, `down` and `reset` detect the same
overlay. The `integration-state` volume retains private material across server
restarts; a deliberate reset removes it with the rest of the disposable stand.

The integration overlay also attaches only the server to a separate non-internal
`integration-egress` network for outbound provider requests. PostgreSQL and MinIO
remain on the internal stand network; no server or storage host ports are
published. Installed adapter host/resource ceilings still apply. Without this
outbound route, a saved token can coexist with an access check reporting ready
configuration, unverified authentication and unavailable resource access. Token
replacement does not repair missing container connectivity.

The integration overlay adds `FOWNER` to the root startup stage so it can reapply
private directory modes after a restart, when retained volumes already belong
to the runtime UID. Startup still drops to the unprivileged server user; the
base stand's capability set is unchanged.

This personal mode delegates local administration to the gateway service, not to
an authenticated human account. HTTPS remains bound to `127.0.0.1`. The gateway
uses a dedicated Unix socket shared only with the server, mounted read-only in
the gateway; the server verifies kernel peer UID `101` and applies the configured
service principal. Another UID, ordinary TCP, browser headers and forwarded
protocol claims cannot establish this identity. It is not authentication for a
shared deployment. Token bytes are private server files (`0700` directories,
`0600` files) and are never returned by the management API.

Read-only connection checks use the installed adapter. Generated Source/Reporter
profiles make their bounded choices available; Project import, Factory input and
task synchronization still require separate explicit review and activation.
Installation and read checks do not qualify live writes.

`/health/factory` reports the independent Source/Factory scope: `disabled` when
optional capability is absent, `unavailable` (HTTP503) for incomplete protected
API composition or a consumer without a recent successful pass, and `ready`
after all installed checks pass. It includes Project import, input/catalog/history,
task synchronization and the Issue, Factory Source and Reporter consumers.
Startup and readiness do not contact a provider. Provider failures remain in the
affected task/input/delivery diagnostics; `/health/ready` and manual task/CI
operation do not depend on them. A healthy scope proves composition and worker
health, not successful import, execution or status delivery; qualify those journeys
separately.

## Verify readiness and a real job

### Optional Factory installation

Use [Factory operations](factory.md) for the mode/readiness distinction and the
operator journey. A demonstration or healthy stand does not replace the exact
source revision's release, real-backend and provider/model qualification gates.

Ordinary `tools/local-stand up` keeps Factory disabled without an installed
catalog. To opt in manually, supply an operator
owned native profile catalog with mode `0600`:

```shell
chmod 0600 /absolute/private/path/factory-profiles.json
tools/local-stand up --factory-profiles /absolute/private/path/factory-profiles.json
```

The launcher copies the bounded catalog into its private server bundle and
enables bounded Factory workers. It applies the Factory Compose overlay only
for that installed bundle. `down`, `status`, `logs`, and `reset` detect the same
installation without a second flag. Repeat `--factory-profiles` on subsequent
`up` commands; the bundle is immutable and rejects changed contents. Changing
the installation requires a deliberate reset of the disposable stand state.

The native Agent requires its own independent policy; a server catalog does
not enable Agent authority. Supply it separately:

```shell
tools/local-stand up \
  --factory-profiles /absolute/private/path/factory-profiles.json \
  --factory-agent-profile /absolute/private/path/factory-agent-profile.toml
```

The Agent policy is an owner-only regular TOML file with `version = 1`,
`factory_permissions`, `tool_executables`, and `allowed_network_hosts`, plus
optional `change_set_capture`, `factory_credential_profiles`, and
`protected_reader`, using the existing Agent configuration types.

An explicitly installed stdio MCP program can set `standalone = true` in its
existing `tool_executables` entry, alongside `path`, `version`, `platform` and
`sha256`. Omission retains the installed plugin compatibility contract. Standalone
mode verifies a native binary for the runner's guest platform, trusted ownership,
exact file digest and pre-execution byte revalidation; it cannot replace the
plugin-owned Codex executable. A shell script alone does not qualify its guest
interpreter. For a script-based server, install and qualify the native interpreter
and retain reviewed script/supporting bytes through the existing input mechanisms.

Inventory is separate from workload authority: explicit command/executable,
network and resource grants are still required. The server Runtime's
`settings_support.mcp_executables` and the selected MCP command must name the
verified guest projection, not the host path. Microsandbox exposes inventoried
programs read-only under `/opt/octacity/tools/tool-N`; the mapping belongs to the
exact installed tool set and must be requalified when that installation changes.
Dependencies such as shared libraries still belong to the qualified guest image.

Permission references and tool entries must agree on exact executable identity, version and SHA-256. Tool
paths are absolute and verified before use. The file supplies credential
aliases through `factory_permissions.secret_profiles`. Install private JSON
credential values separately and map only their absolute paths through
`factory_credential_profiles`, as described in [Agent operations](agent.md).
The launcher preserves these references in the Agent policy and never copies
the values into the server catalog, generated configuration, or receipt.
The Agent validates the private files before enabled execution; installing a
reference does not grant a Job permission to consume it. Changing a reference
after publication requires reset. Rotate values in the separately owned file
and restart the Agent without changing its reference or exposing it to the
server.

`protected_reader` selects a separate released reader distribution, its exact
tool/executable identities, platform, and resource/output bounds. It does not
add the reader to model plugin tools. Keep its distribution outside all Agent
writable roots, source-plugin/Octa installations and credential directories.
The installed Agent validates the technical envelope and binary inventory in
preflight and startup; backend capability matching still rejects an
unqualified reader or requested permission. Changing that policy also
requires reset. Omitting both fields preserves the ordinary stand profile.

For a separately released Linux ARM64 reader, native staging accepts
`tools/stage_local_stand_native.py install --protected-reader-root DIR`
(or `all` with the same option). Keep the usual staging arguments for the
pinned inputs. `DIR` must contain exactly the released worker `manifest.json`
and `octacity-protected-reader`. Staging checks the platform, digest, file
bounds, ownership and absence of links, adds the files to the installation
inventory, and records their relative root under `protected_reader` in
`installation-manifest.json`. Use that root below the printed installation
path in the Agent policy. The installation name includes the reader digest;
an ordinary installation without the option keeps its original identity.
Staging does not qualify the reader backend or execute the worker.

`change_set_capture` selects the existing Agent capture provider and its own
operator-pinned executable; it does not infer capture authority from model
outputs or the server catalog. The current provider is `git`. Its `executable`
contains an absolute `path`, `version`, `platform`, and `sha256`, with the same
byte verification as independently installed tools. The Agent validator also
requires a distinct canonical path outside Agent-owned writable roots. Omitting
this field keeps capture unconfigured. Changing or removing it after publication
requires reset, just like changing the remaining Agent policy. These settings
prepare configuration only; they do not qualify a writable backend or enable
native author roles.

The launcher validates the selected policy with the installed Agent in
disposable work, cache, state, and credential directories before Compose or
enrollment. It leaves live Agent state alone and removes those temporary
directories. Registration must advertise Factory v3 execution before startup
is accepted. This checks the advertised contract; actual backend qualification
and successful Job execution still need their released backend tests. Repeat
the Agent policy flag on subsequent `up` commands. Changing a published Agent
policy requires reset and never silently enables a fallback backend.

The catalog supplies exact published references and qualified profiles; the
launcher does not create Flow Definitions, select node names, infer model
aliases, or provide missing credentials. JEV must use an installed connector
profile. The existing Codex fixture proves the execution boundary only and
does not qualify an author, reviewer, verifier, delivery, or promotion profile
for the complete Factory pilot. Provider credentials remain separate from the
catalog and ordinary stand credentials.

A `codex` profile may explicitly select `reviewer: {"evaluator": REFERENCE}`
for an independent typed review. This requires an evaluator credential purpose,
read-only review mode and an exact `candidate_capture_tool`; capture, writable
tool control and response-limit overrides remain unavailable for that role.
The evaluator reference changes the immutable task digest. The Node must
explicitly select the complete committed Evaluation Plan and Evidence Manifest,
the exact candidate and the real selected CriterionPack Artifact bytes. The
server qualifies the result records and constructs the Assessment, including
its model, prompt, Plan and result provenance. A model-provided Assessment
object grants no authority. Omitting `reviewer` preserves ordinary observation
profiles and their task identities. These installation checks do not substitute
for the complete released-product pilot.

Install non-model programs in the catalog's optional `commands` array. Each
entry supplies `identity`, `plugin`, `executor`, `build_configuration`,
`pipeline_node`, `program`, `input_destination`, `parameters`,
`project_permissions`, `local_permissions`, and `outputs`; an optional
`candidate_capture_tool` pins the accepted candidate producer. These are exact
versioned references, not logical Flow or node names. Profiles sharing an
`executor` use one native binding, with the immutable task digest selecting
the installed program. The Build Configuration must also be offered by the
same Project's installed choices.

Both Project and local plugin grants must equal the complete approved Octa
runtime inventory by plugin identity and SHA-256. The Agent compares the signed
set with its installed `Octa.lock`; a subset does not describe that installation.
The admitted Node's plugin grants must retain that exact inventory as well.
The profile's primary `plugin` selects the immutable operator program and its
output contracts. Approving the runtime inventory does not add external
executables, tool control, network, secrets, or workload identities: hermetic
commands continue to reject those grants. Inventory changes require an explicit
profile, task digest, Flow binding, and Agent policy update; admission never
adds plugins automatically.

`program` contains the installed Octafile string, finite `tasks`, optional
`workspace_root`, and `credential_profile` (which must be absent or null for
hermetic commands). The Octafile uses the released Octa format. Its dedicated
installed-source type accepts nonblank YAML or JSON up to 64 KiB of UTF-8,
preserving indentation and line endings in the exact task digest. Unsupported
control characters are rejected. Human-readable Factory summaries retain
their separate 4 KiB limit. Declare outputs in the Octafile's `reports` or `artifacts`
as well as the profile's output contracts; creating a file alone does not
publish it. With task `dir: /workspace`, declare paths relative to that workspace,
such as `output/result.json`; the shell may write `/workspace/output/result.json`.
The Agent rejects absolute output declarations. Source and protected inputs
remain read-only; writable paths are limited to output and scratch.

The static program is published separately from frozen input JSON. Neither
Flow data nor a returned report can supply replacement instructions or
provenance. The `executor` reference selects the server capability and does
not require inventing an Agent external executable. Actual execution still
requires the exact released plugin and independently qualified backend.

Protected validation additionally publishes
`/octacity/protected/validation-input.json`. Its `changeset_id`, `candidate`,
`tests`, `input_digest`, and ordered `classes` come from the original verified
candidate and retained test plan. Use these identities when forming the
validation report; neither report JSON nor arbitrary Flow payloads may replace
them. This descriptor contains no private test bodies. Reserve at least six
non-test input slots in the frozen plan: three compiler documents, the
candidate bundle and capture manifest, and the private test inventory.
Private test files require their own slots and retention through the Job's
deadline. The compiler checks the complete manifest before signing.

The same catalog keeps the remaining roles explicit:

| Catalog array | Installed contract |
| --- | --- |
| `codex` | Exact author or evaluator profile, with its separate credential purpose and optional trusted capture policy. |
| `review_planning` | An exact `identity` and immutable `profile` describing the independent review plan. |
| `evaluation_decisions` | An exact action `identity` and `profile` with `action`, immutable `policy`, and all five `{disposition, outcome}` contracts. The policy contains `version`, `required_evidence`, `required_evaluators`, `quorum`, `severity_threshold`, `indeterminate_policy`, and `failure_outcome`; its canonical digest is bound into the action identity. |
| `protected_test_authors` | Entries contain `command` using the ordinary static command format and canonical `policy` containing protection limits under `policy`, exact `inventory`, and finite private `files` mappings. |
| `protected_validation` | `profiles` of static commands, `inputs` containing `author_profile`, `policy`, `inventory`, and `capture_tool`, plus distinct declared `report` and `facts` contracts. |
| `protected_validation_evidence` | Exact action `identity`, evidence `profile`, and the original author/policy/inventory/capture references used to verify and publish validation evidence. |
| `delivery` | An exact action identity and outcome, private `adapter_root`, content-derived `policies`, and bounded process timeouts. |
| `promotions` | An exact trusted-action `identity` and static command `profiles`; each command's executor and physical plugin must equal that identity. Optional `deployment_authority` explicitly selects the separately qualified external-target role. |

Author and evaluator credentials remain Agent-local; Delivery credentials stay
in the separate server package. Hermetic validation and promotion command
profiles receive no model, network, secret, external-tool or workload-identity
grants. Installing a role does not connect it to a Flow: publish explicit
definitions with the exact references and task digests. None of these arrays
supplies an implicit business graph or chooses logical node names.

An external deployment uses `promotions[].deployment_authority` with `identity`
and `version` policy keys, an immutable `environment` reference, a canonical
HTTP(S) `endpoint`, and optional logical `credential_profile`. The authority
constructor recomputes its domain-separated reference; every command's
`identity` must equal that reference. Changing the target, environment or
credential requires repinning the command identity and task digest. URLs with
userinfo, query or fragment are rejected. Credential bytes remain Agent-local.

This deployment role permits exactly the endpoint's derived host and its
optional single credential, under both project and local ceilings. It requires
the complete approved plugin inventory, readonly Source and protected inputs,
and writes only to output/scratch; external executables, tools and workload
identity remain unavailable. Dispatch and retained-output verification use the
same installed qualification and approved toolchain. The static program receives
`/octacity/protected/deployment-target.json` with `authority`, `environment`,
`endpoint` and `credential_profile`, alongside the independent frozen promotion
plan. This concrete adapter accepts one environment per action. Installation
alone does not establish a deployment effect or a successful post-check.

A protected-test author is installed as a Reasoning capability under its exact
command executor identity. Its command identity digest must equal the canonical
policy digest; changing a private file name, class or path requires repinning.
The static command publishes only its declared private files. Before ordinary Job
success, the server verifies the signed command and derives the inventory and
public handoff from actual published file IDs under the current Lease fence.
Completion retries reuse those publications. Ordinary commands retain their
existing completion path.

The author observer keeps private files and inventory as opaque Artifacts; only
the schema-checked handoff becomes Flow data. Readonly candidate inputs require
an explicit trusted capture verifier. Authoring cannot combine promotion or
private validation roles, model/network credentials, protected tools, or Source
writes. Leave one Artifact-list slot beyond private files plus inventory and
handoff so undeclared publications remain detectable. Installing this role does
not qualify its generator or complete the released-product pilot.

Promotion command installation explicitly enables its separate input role.
`/octacity/protected/promotion-input.json` contains the original frozen `plan`
and `plan_digest`, including the exact candidate, environment references,
observation policy and configured disposition criteria. The command must use
these facts for its deployment observation. Reuse the exact trusted candidate
capture pin; an installation cannot infer it from returned JSON. Ordinary
commands do not acquire this role from their node kind. Promotion and private
test validation are separate compiler roles and cannot be combined. The
ordinary CI/CD system remains responsible for rollout and rollback; the
Factory adapter qualifies its observation against the frozen plan.

Readonly Codex profiles may include `candidate_capture_tool`, an exact
`ImmutableReference` to the trusted capture implementation accepted for retained
candidate provenance. It does not grant model commands or writable source access,
and it does not change the task digest. Omitting it preserves candidate rejection.
The runtime candidate reader and released backend isolation still require separate
qualification; setting this field alone does not enable the complete pilot.

Installed Codex policy can also declare `tool_response_limits` with positive
`per_call_bytes` (at most 4 MiB) and `total_bytes` (at least the per-call bound).
These are independent reader quotas, never inferred from Artifact or report
publication limits. They become part of the immutable task digest and signed
`tool_control.response_limits`; a tool-using compiler refuses absent limits.
Tool-free profiles may omit this field and preserve their earlier task identity.
This policy does not qualify a helper or enable author/tool roles by itself.

A Codex entry can explicitly select `"execution_mode": "protected_tools"` with
`"credential_purpose": "model"` and finite `tool_response_limits`. It must omit
both `capture` and `candidate_capture_tool`. Its permissions must select protected
tools, the model credential, the three CodeX record outputs, and read-only or no
Source mount. This mode grants neither source writes nor candidate reconstruction;
independent evaluator profiles continue to use their separate credentials.

The private loader requires exactly one installed `tool_risk` entry with the same
`base_task_digest`, complete `build_configuration` reference and Project. It derives
the compiled provider policy from that binding. A missing, ambiguous or outdated
binding fails installation; it never silently becomes deterministic tool execution.
Omitting `execution_mode` preserves the existing review and capture profiles and
their task identities. The Agent must separately install and qualify its bounded
protected-reader runtime; declaring server metadata alone does not qualify it.

Install server-side Decision Signal connectors through a separate private
catalog, repeated on each `up` alongside the Factory profile:

```shell
tools/local-stand up \
  --factory-profiles /absolute/private/path/factory-profiles.json \
  --factory-signal-connectors /absolute/private/path/signal-connectors.json
```

The connector catalog has this outer format; both referenced files must have
absolute paths and mode `0600`, as must the catalog itself:

```json
{
  "version": 1,
  "connectors": [
    {
      "kind": "jev",
      "credential_profile": "routing-key",
      "configuration_file": "/absolute/private/path/jev-settings.json",
      "credential_file": "/absolute/private/path/jev-api-key"
    }
  ]
}
```

The launcher copies adapter configuration as opaque bytes and rewrites only
these file references. The credential profile must match the corresponding
Signal binding: connector discovery resolves that alias and the exact provider
capability together. A routing binding cannot select a connector installed with
another credential alias. It publishes profiles, connector configuration and keys
atomically in the immutable private server bundle. The Factory overlay mounts
that bundle read-only; the entrypoint copies only bounded connector files to
the private runtime directory and gives ownership to the unprivileged server.
Connector inputs never enter the Agent policy or ordinary stand credentials.
Changing any published input requires resetting the disposable stand.

The installed adapter interprets its own configuration. The built-in `jev`
adapter requires `version = 1` and JSON fields `endpoint`, `capability`,
`credit_cost_micro_units`, and `timeout_milliseconds`; its API key is read
separately. The endpoint's actual host must be granted by the corresponding
Factory Signal binding. An unknown adapter kind, malformed private file or
ungranted destination fails installation. Custom adapters implement the neutral
Decision Signal provider contract and are registered by server composition;
the file catalog discovers only adapters installed in this server binary.
Installing a connector does not qualify an execution backend or complete the
released-product Factory pilot.

Delivery processes have a separate private installation:

```shell
tools/local-stand up \
  --factory-profiles /absolute/private/path/factory-profiles.json \
  --factory-delivery-bundle /absolute/private/path/delivery-adapters
```

The supplied directory contains the installed provider registry, including its
`adapter.toml` manifests, pinned Linux ARM64 executables, opaque provider
configuration, private credential files and input directories. It uses the
existing Delivery process protocol; the launcher does not interpret a forge
API, select a Flow/node, or grant delivery credentials to the Agent. Keep
directories owner-private (`0700`), data files `0600`, and executables `0500`,
`0700` or `0755`. Links and special files are rejected. The package is bounded
to 1,024 entries, four directory levels and 512 MiB total; individual executable
and data bounds are 256 MiB and 4 MiB respectively.

The launcher publishes this package atomically with the immutable server
bundle. The entrypoint copies it into `/run/octacity-delivery` with private
permissions and ownership of the unprivileged server. Point installed delivery
profiles at that container path; provider configuration must use container
paths for its own dependencies. The native registry independently verifies
the exact executable and protocol capabilities. Delivery policy references
must match their complete computed content digest and the adapter's pinned
configuration digest. Installation performs no publication or credential
resolution. Changing a published package requires a deliberate stand reset;
`down`, `status`, `logs` and `reset` use the installed overlay automatically.
Repeat the same Delivery bundle option on each subsequent `up`, alongside the
Factory catalog and any separately installed Signal connector options.

Only the Factory overlay increases private runtime storage for bounded profile
and connector inputs and installs an executable private tmpfs for Delivery.
The ordinary Compose profile keeps its existing mounts. Retained delivery
inputs are reconstructed from Project-owned, digest-verified Artifact records;
the tmpfs is disposable and is not authoritative state. Existing Factory/Build
retention holds preserve objects needed by active or held Runs. Retention must
cover the complete configured execution and review window; clearing an input
directory cannot make an expired or missing Artifact valid again.

`/health/ready` remains the ordinary CI/CD readiness endpoint.
`/health/factory` reports the enabled Factory capability independently, so a
degraded connector must not prevent ordinary sample Builds from running. The
default lifecycle and sample Build verifier require no Factory credentials.

The health commands above verify the gateway and server. The integration
verifier additionally checks the built UI entry point, exactly one local Pool,
exactly one online macOS ARM64 Agent, its Linux ARM64 Microsandbox target, a
cancelled shell process tree with no accepted partial output, the Agent's
return to idle readiness, deterministic Codex success, overflow, and
cancellation inside Microsandbox, a subsequent ordinary successful Build and
Job with no Factory state, and an artifact uploaded to and downloaded from
MinIO. The Codex fixture is a release-inventoried local script with no network
access or model credential. Its trace/result/provenance and complete Job event
stream are scanned for prompt and credential canaries; failed and cancelled
runs must publish no partial output and the Agent must return to idle after
each case. This is execution-boundary evidence, not a Factory deployment or a
real model call.

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

Factory bootstrap and the final gateway use different caller identities, so
their idempotency receipts are separate. If pool creation returns HTTP 409,
bootstrap looks up the existing `local-macos-microsandbox` pool and reuses its
current ID and version only when its policy exactly matches the standard stand
policy. It does not overwrite a customized pool or ignore other API failures.
This allows an interrupted Factory startup to resume without resetting data.

### Agent registration fails after Docker volumes were recreated

If Compose reports newly created PostgreSQL/MinIO volumes while the private
stand root still exists, the server and host state belong to different
installations. The Agent can exit before readiness with `HTTP 401:
credential_rejected` in `logs/agent.log`: its retained registration credential
is unknown to the new database. Factory resource and Run receipts are also
stale. Increasing readiness timeouts or deleting only the credential cannot
restore the original installation.

Restore the matching database and object-store backups if those data are
needed. For a disposable stand, use the confirmed reset procedure above to
clear both host and server state together, then run `tools/factory-stand` to
create a fresh Factory demonstration. The launcher does not reset data or
replace a rejected credential automatically.

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
