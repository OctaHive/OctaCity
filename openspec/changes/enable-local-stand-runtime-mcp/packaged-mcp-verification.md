# Task 3.1 — packaged MCP qualification

Status: complete on 2026-10-28. The full native packaged journey passed,
including revocation, cancellation and retained evidence after restart.

## Boundary and installation

The existing ignored `factory_pilot` role journey has an opt-in real-client leg
driven by `PackagedMcpTests` in `tools/tests/test_factory_mcp_credentials.py`.
It reuses the finite HTTP/Responses peers from the native credential tests and
the existing packaged Server/Agent, ordinary Build, role publication, Artifact
and restart helpers. It adds no release workflow or paid model/provider call.

Before starting the role server, the journey passes its complete operator
catalog and separate Agent policy through the ordinary local-stand server and
Agent configurators. They validate actual release inventories, executable
digests, independent private credential files and worker ceilings. The server
loads the generated catalog; the worker uses the generated Factory permission
projection. The fixture retains its existing service addresses, enrollment and
isolated database; no additional MCP permission is added after composition.
The ordinary Build Configuration and Agent separately approve the finite host
destination before publishing or admitting a Run.

The real Codex executable and stdio peer are separately pinned standalone
executables. A test-only native launch shim forwards the plugin's MCP arguments
and credentials unchanged, selecting only a finite Responses peer and the
test-owned HTTPS certificate. The role profile cannot change the model provider
or process trust environment. Ambient repository instructions/MCP configuration
remain outside the read-only role workspace.

## Qualification scope

The qualified composition is macOS ARM64 Agent → Microsandbox 0.7.6 →
Linux ARM64 Codex 0.161.0 → published Octa 0.8.0 Codex plugin
(`590ecfe4a8de4c2e7d54a9a60b7075781ab0cac4`). The guest image is the existing
finite Node 24.21.0 image built from the immutable stand inputs. Peers are finite
test servers, not a qualification of a particular GitHub/GitLab/Jira MCP release.
The verified Codex Linux ARM64 archive has SHA-256
`5c90bf4be3c97fc62fcafb28ec2335ee318e6ecb332e1e8ee3d2c63814a5a26a`;
the imported finite guest image is
`local.octacity/factory-pilot@sha256:ee1e56d0adfd06f3d219f0e064219acb23deb0c6579bf6f418a75810c489bee9`.
Server and Agent use checksummed current native bundles; this is local
qualification of the working tree, not a new published OctaCity release.

HTTP uses Streamable HTTP JSON-RPC over HTTPS with a dedicated bearer token;
stdio uses JSON-RPC lines and an independently mapped token environment entry.
Both use an explicit immutable resource scope. A token/peer must enforce that
scope and reject writes: an MCP `readOnlyHint` is insufficient authority.
AI-provider login and server Source/Reporter credentials remain independent.

The complete journey verifies the selected model/reasoning/MCP settings,
actual consumed content and refused operations, cancellation/partial capture,
credential revocation, redaction, immutable Runtime settings and retained
trace/provenance after Server/Agent restart. Native capture uses the existing
version-2 trace/provenance Artifact contract; it creates no search parser or
parallel tool journal.

## Observed TDD and checks

The finite CLI launcher first failed with exit 64 when invoked with the
published plugin's root-level approval option. Normalizing that fixed prefix
and updating the packaged command permission restored the current native CLI
contract. The public CLI regression passed 18 tests:

```shell
python3 -m unittest tools.tests.test_factory_pilot_launcher tools.tests.test_factory_pilot_cli
```

The four existing real host-native credential/capture tests passed before and
after extracting their finite peers for reuse. Combined native/CLI regression
passed 22 tests. The original finite packaged role journey passed, including
actual VM execution, role/catalog replacement and restart.

The new authenticated packaged journey exposed an installation composition
defect: role invocation retained a pre-installation store with disabled Factory
capabilities, while management had the installed catalog. The runtime now binds
its executor store to the same capability source returned to management.
The initial full Red stopped before an evaluator Build at the finite deadline.
With the repair, actual HTTP/stdio calls, typed capture and exact retained
Runtime/evidence after restart passed. The final complete negative journey also
passed after the cancellation repairs below.

Review of the failed native Job also exposed an accounting failure: without
trustworthy usage the adapter returned `Unavailable` from observation, which
could prevent the same reconciliation pass from reaching other Runs. Observation
and cancellation now return the existing waiting state with the exact accepted
Build/Attempt. The budget reservation remains held; late measured usage still
settles it, and no zero consumption or semantic completion is invented.
Physical Job cancellation and budget settlement are distinct: a cancelled Job
can remain unsettled until real usage is available.

Each behavior had its own public node-adapter Red/Green cycle:

```shell
cargo test --locked -p octacity-server-application --lib failed_build_executor_requires_measured_consumption_without_provider_results
cargo test --locked -p octacity-server-application --lib node_build_executor_cancellation_requires_actual_terminal_consumption_without_parsing_results
cargo test --locked -p octacity-server-application --lib factory_node_build_tests
```

The first two each failed with `Err(Store(Unavailable))` before their respective
repair. All 30 adapter tests then passed, including exact execution identity,
late measured consumption and single-dispatch assertions.

Qualification additionally exercises failure isolation in the shared owner.
The public batch test `a_run_failure_does_not_skip_other_claimed_runs` first
observed zero transitions instead of two when one snapshot failed and worker
concurrency was one. The reconciler now continues filling its bounded queue
after a peer error while retaining the first error for retry reporting. The
same test covers concurrency two; all nine reconciliation tests passed,
including shutdown and concurrency bounds. This introduces no per-Run retry
queue or new ownership state.

The remaining native cancellation failure was `Flow(InvalidInput)`: cancellation
rebuilt a static `FlowBuildIntent` and compared it with the retained selected-role
intent, losing its profile selection. The focused public adapter test
`selected_role_cancellation_stops_the_original_ordinary_build` first dispatched
one ordinary Build and then failed with `InvalidInput` on cancellation. The
adapter now reuses `FlowBuildIntent::verify_admission`, the existing validator
for both static and selected contracts. It preserves the original operation,
profile and admission without another mutable catalog lookup.

The selected-role test then passed. After the final production repairs, the
three relevant suites passed 78 tests (30 node adapter, 39 role materialization,
9 reconciliation):

```shell
cargo test --locked -p octacity-server-application --lib selected_role_cancellation_stops_the_original_ordinary_build
cargo test --locked -p octacity-server-application --lib factory_role_materialization_tests
cargo test --locked -p octacity-server-application --lib factory_reconciliation_tests
```

The complete application library regression passed 795 tests before the final
batch-isolation and selected-intent repairs; the focused suites above were rerun
after those repairs. The server library passed 220 tests (51 environment tests
ignored). The script regression passed 524 tests (13 opt-in environment tests
skipped), and the CLI regression passed 18 tests. Final library and packaged-pilot
Clippy passed with warnings denied. Architecture (63 packages, 261 edges), Rust
formatting, diff whitespace and strict OpenSpec validation passed.

The final packaged native command passed in 114 seconds with no skipped case:

```shell
# Supply the existing packaged-pilot environment from docs/testing/backend-contracts.md.
# Use the verified Linux Codex, rebuilt finite pilot CLI and native policy helper.
OCTACITY_TEST_PACKAGED_MCP=1 \
  OCTACITY_FACTORY_PILOT_CLI=/absolute/path/to/codex-aarch64-unknown-linux-musl \
  OCTACITY_FACTORY_MCP_STDIO_EXECUTABLE=/absolute/path/to/pilot-cli \
  OCTACITY_FACTORY_MCP_POLICY_HELPER=/absolute/path/to/octacity-release-harness \
  python3 -m unittest tools.tests.test_factory_mcp_credentials.PackagedMcpTests
```

This invokes the existing
`packaged_factory_pilot_runs_real_jobs_before_work_admission` test with its native
MCP leg. It observes four native HTTP/stdio calls and exact model/reasoning,
refused cross-resource reads and writes, a rejected revoked bearer without
fallback, and cancellation before the finite peer/Job deadlines. Both native
trace and provenance remain inspectable after cancellation, with status
`cancelled`, an unfinished MCP call and no invented completion. An initial
fixture expectation of generic `partial` was corrected to the published native
contract's explicit `cancelled`; no production capture behavior changed.
The successful Run retains identical Runtime settings and evidence after
Server/Agent restart, without another model request. Negative Runs publish no
semantic node completion. All five credential canaries remain absent from the
retained native records.

Two subsequent qualification setup failures are not behavioral Red evidence:
a cancelled prior fixture left a protected disposable workspace for orphan
cleanup, and concurrently running CPU-heavy application tests made the Agent
report zero available CPU. The latter correctly left the new Job ready and
unassigned. Final VM qualification runs separately from those load tests.

## Operator and triage handoff

Use the [ordinary explicit installation and update path](../../../docs/operations/local-stand.md#select-an-operator-owned-factory-installation),
then publish Project Runtime settings, explicitly select the exact MCP pins in a
new role/profile version, and admit an ordinary Factory Run. Configuration
success alone is not proof of connectivity or tool execution.

Triage 4.3–4.4 consumes the installed executor/transport/auth/capture capabilities,
separate credential-purpose/resource bindings and the
[common native evidence handoff](mcp-evidence-verification.md). It owns the
OctaCity MCP server, duplicate interpretation and accepted business effects.
This change does not complete those tasks or qualify untested AI plugins/MCP
servers.

## Disk

Free space is checked during builds. Only old duplicate Cargo artifacts,
task-owned image archives and the identified task-owned Docker build cache were
removed; no stand volumes or user work were removed. An active Cargo build had
to be restarted after duplicate cleanup removed three already selected library
artifacts; cleanup of active targets was then stopped. Final cleanup removed
only this qualification's PostgreSQL/MinIO containers and anonymous database
volume, unused test server image, copied bundles, VM state and private environment
snapshot. Qualification logs remain under `/tmp/octacity-mcp-3-1/`.
Final free space: approximately 12.8 GB. User stand volumes, installed runtimes and unrelated
work were preserved.

## Review follow-up: Agent authority source

The packaged native pilot now consumes all four Agent authority fields from the
ordinary configurator output: `factory_permissions`,
`factory_credential_profiles`, `tool_executables` and `allowed_network_hosts`.
The harness no longer reconstructs native credentials, executable pins or network
hosts separately. Native setup details remain private to their owning module.

Before refactoring, a characterization assertion was added at the generated
Agent TOML boundary to compare those four fields with the configurator output.
The complete packaged MCP journey passed with that assertion before refactoring
(106.813 seconds), and again afterward (115.743 seconds). This was a behavior
preserving refactor, so baseline Green followed by regression Green applies;
there is no claimed behavioral Red for this slice.

The command was the `PackagedMcpTests` invocation documented above. Both runs
used the same native Codex/Octa/MSB versions and the same guest image digest:
`sha256:ee1e56d0adfd06f3d219f0e064219acb23deb0c6579bf6f418a75810c489bee9`.
HTTP/stdio calls, refusal, revocation, cancellation evidence and restart
retention all remained covered. Restoring the disposable test environment
required fixing its writable MinIO client config and shortening the VM socket
path; these setup failures are not behavioral Red evidence.

Additional checks:

```shell
cargo test --locked -p octacity-release-harness --test release_vertical_slice backend_config_tests
cargo clippy --locked -p octacity-release-harness --test factory_pilot -- -D warnings
```

The Agent configuration regression passed 10 tests with one separate opt-in
installed-process test ignored; the complete native journey above did execute.
Clippy passed with warnings denied. Native before/after logs remain under
`/tmp/octacity-mcp-review-fixes/`. The task-owned containers, anonymous database
volume, copied bundles and VM state were removed after verification; source
runtime installations and user stand data were preserved. Approximately 12.6 GB
remained free after the final Rust checks.

## Real Agent inventory fixture correction

The standalone MCP inventory test originally supplied plain text as an executable.
Its shell validator accepted that fixture, but the real Agent correctly rejected
it with `external executable format is unsupported`. Before changing the fixture,
the reported test was reproduced with the actual Agent:

```shell
OCTACITY_AGENT_BIN="$PWD/target/debug/octacity-agent" \
  python3 -m unittest tools.tests.test_configure_local_stand_agent.LocalStandAgentConfigurationTests.test_explicit_standalone_mcp_inventory_preserves_independent_workload_grants
```

It failed before the correction and passed afterward. The fixture now supplies
an ELF64 little-endian AArch64 header matching its declared `linux-aarch64`
inventory platform. This boundary inspects inventory format and identity; it
does not execute a guest program. The existing permission/network assertions
remain unchanged, as do production validation and private diagnostic redaction.

With the real Agent selected, `python3 -m unittest
tools.tests.test_configure_local_stand_agent -v` passed all 27 tests. With the real
Server and Agent selected, `python3 -m unittest
tools.tests.test_configure_local_stand_server -v` passed 18 tests with one opt-in
case skipped. Architecture, strict OpenSpec and diff whitespace checks passed.
Approximately 12.6 GB remained free; no stand data or installed runtime was removed.
