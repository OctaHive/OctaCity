# Task 1.2: installed programs and native MCP settings

## Delivered boundary

The existing Agent `tool_executables` inventory accepts an explicit
`standalone = true` entry for an operator-installed native program, such as a
ready-made stdio MCP server. Omission preserves the plugin compatibility contract.
The Agent configurator forwards the reviewed flag; composition inventories the
program through the ordinary Runner and execution projection rather than another
registry, launcher or MCP client.

Standalone programs retain trusted path/ownership, guest-platform, SHA-256 and
immediate pre-start revalidation checks. A plugin-owned executable cannot opt out
of its release compatibility contract. Missing approvals, substituted bytes and
unqualified interpreted scripts fail closed. Native interpreters and reviewed
supporting files can use the existing executable/input mechanisms; runtime
dependencies still belong to the qualified guest image.

Inventory does not add command, network, credential or resource grants. Existing
Runtime validation and ordinary compilation retain exact transport/reference,
selected-tool/environment and permission checks. The verified program projects
through the existing read-only guest tool mounts. Guest names belong to the exact
installed inventory; host paths are not MCP guest commands.

Role-selected compilation now always supplies an explicit MCP map, including
`{}` when no server was selected. The Codex adapter then emits its explicit
client isolation/configuration options. The internal projection returns the map
directly; the outer optional Role selection owns absence. Legacy non-Role
compilation retains its existing path. Published Role/Profile definitions are
not rewritten.

## Red/Green evidence

Each test below was written and observed failing before its production change,
then rerun successfully. Cargo commands use `--locked -j2`,
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`.

| Command | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-runner --lib explicitly_installed_standalone_mcp_program` | Required public inventory API absent (`E0599`) | Explicitly approved native peer is verified/projected; replacing its bytes fails revalidation |
| `cargo test -p octacity-agent --test cli validates_an_explicit_standalone_mcp_program` | Real Agent CLI rejects unknown `standalone` field | Real configuration/composition accepts one pinned standalone program without a plugin contract |
| `python3 -m unittest tools.tests.test_configure_local_stand_agent.LocalStandAgentConfigurationTests.test_explicit_standalone_mcp_inventory_preserves_independent_workload_grants` | Operator policy rejects the new explicit field | Generated Agent configuration retains the selection without adding workload/network grants |
| `cargo test -p octacity-runner --lib standalone_approval_cannot_name_an_absent_program` | Unmatched approval silently succeeds | Approval for an absent program is rejected |
| `cargo test -p octacity-runner --lib standalone_mcp_program_requires_native_guest_bytes` | A portable shell script is accepted as a standalone guest program | Native guest bytes are required independently of the script's version comment |
| `cargo test -p octacity-server-application --lib role_without_selected_mcp` | Empty Role selection compiles to `null` | It compiles to `{}` for explicit native-client isolation |

An obsolete API-first test draft for the abandoned server-side duplicate lookup
prevented application tests from compiling. It and its module registration were
removed before compiler verification; this restoration is not reported as a TDD
Red. The planned replacement remains triage's OctaCity MCP task 4.2.

## Qualified plugin boundary and remaining scope

Task 1.2 initially consumed Octa 0.7.2. The inspected plugin source was
`octa_plugin_codex` 0.7.2 at Octa commit
`c60202e90eb8b99eaab9ba46b446a3cde7f846a8`. That slice changed no upstream source;
the subsequently approved fixes and published 0.7.3 qualification are recorded
under task 1.4 below.

The actual plugin-manager/process tests were run with:

```shell
cargo test --locked --manifest-path ../octa/Cargo.toml \
  -p octa_plugin_codex --test plugin_manager explicit_ --target-dir target -j2
```

All three passed: additive CLI options, HTTP bearer environment-name mapping with
credential redaction, and stdio argv/tool/environment selection. They exercise
the real plugin and a finite CLI process matching its pinned 0.161.0 contract;
they do not call a paid model or certify a live remote server.

| Boundary | Evidence and limit |
| --- | --- |
| Codex plugin/native CLI configuration | Exact HTTP/stdio options and selected environment/tool schemas are exercised; ambient configuration is explicitly disabled. No Claude or other AI plugin is certified by these tests. |
| HTTP authentication | The plugin's explicit HTTPS bearer environment-name contract is qualified. OctaCity's independent per-connection secret materialization and access enforcement remain task 1.4. Other authentication methods are not implicitly supported. |
| Stdio installation | Real Agent CLI, file/platform/hash checks and the ordinary guest projection are exercised. A metadata row alone is insufficient. Dependencies and actual packaged guest execution still require the exact installed image/program qualification in 3.1. |
| Source/Reporter | Existing server synchronization remains independent; no new provider wrapper or model-owned writer is installed. |
| Full stand and evidence | State-preserving updates remain 1.3, independent credential isolation 1.4, retained native tool evidence 1.5 and the complete HTTP/stdio packaged journey 3.1. This task alone is not an operational-stand certificate. |

Initial free space was 18 GiB. Agent/plugin builds reused the existing target
directory with debug information and incremental compilation disabled. Space
remained above the 10 GB cleanup threshold; no retained stand data was removed.

## Regression and repository checks

The same Cargo environment and `--locked -j2` apply below.

| Command | Observed result |
| --- | --- |
| `cargo test -p octacity-runner -p octacity-config -p octacity-execution --lib` before changes | 103 passed |
| `cargo test -p octacity-runner -p octacity-config -p octacity-execution -p octacity-agent --tests -- --test-threads=2` | 146 passed, 6 existing opt-in tests ignored |
| `cargo test -p octacity-execution-microsandbox --lib` | 14 passed, including verified host-to-guest mapping and bounded Factory roots |
| `cargo test -p octacity-server-application --lib factory_role_materialization_tests` after the map-return refactor | 35 passed |
| `cargo test -p octacity-server-application --lib factory_codex` | 59 passed, including legacy/static compilation and retained result checks |
| `cargo test -p octacity-server-application --lib mcp` baseline | 11 passed |
| `python3 -m unittest tools.tests.test_configure_local_stand_agent tools.tests.test_local_stand_launcher tools.tests.test_factory_local_stand tools.tests.test_factory_local_runtime` | 66 passed, 4 existing platform/opt-in skips |
| `python3 -m unittest tools.tests.test_local_stand_documentation` | 3 passed |
| `cargo clippy -p octacity-runner -p octacity-config -p octacity-agent --all-targets -- -D warnings` | Passed |
| `cargo clippy -p octacity-server-application --lib -- -D warnings` | Passed |
| `cargo fmt --all --check`, `python3 tools/check_architecture.py`, strict OpenSpec validation and `git diff --check` | Passed; architecture: 63 packages, 261 edges |

Application test builds retain the pre-existing unused
`pending_factory_profile_fixture` warning; library Clippy remains clean. Final
free space was approximately 15 GiB. No model call, real stand reset, paid API or
live repository mutation was used as a test boundary.

## Task 1.4: independent MCP credentials

The credential path now reuses the existing installed Runtime support, logical
secret references, signed managed execution, Agent private profile files and
protected Octa variables. There is no new credential registry or MCP client.
An authenticated immutable configuration includes `authorization` with an
independent `credential_profile`, logical `secret`, transport `environment`,
`purpose: mcp_read` and immutable `resource`. Runtime publication requires the
exact operator-approved peer/server/resource contract in
`FactoryRuntimeSettingsSupport.authenticated_mcp`; an allowed hostname alone
cannot authorize redirecting a token. Changing this metadata changes the pin.
Anonymous configurations keep their existing serialized content and digest.

The selected Role maps each approved secret reference to its transport
variable. The compiler retains only references and a signed private-variable to
Agent-profile map, alongside the separate model profile. Independent worker and
Project permission ceilings must allow all selected profile identities. Model,
evaluator, Source and Reporter profiles cannot substitute for MCP authority;
peers cannot share the same profile/reference or materialized token value.
Unselected profiles are not materialized. Compiler variable names are bounded
opaque identities, independent of the keys used by operator private files.

Each dedicated MCP private profile is the existing private JSON object with
exactly one non-empty credential value, for example `{"TOKEN":"<private value>"}`.
Its name in Agent `factory_credential_profiles` must match the approved connection
metadata; its path and value stay in operator-owned private Agent configuration.
The file must satisfy the existing private filesystem/owner/size rules. It is
reread before input transfer and again before execution authorization, so deleting
or invalidating it blocks new work without an Agent restart. Rotation supplies
the new private value without rewriting any published profile. The aggregate
private-value budget remains 1 MiB and the existing finite entry limit.

The qualified methods are HTTPS bearer environment mapping and one explicitly
mapped stdio read token through `octa_plugin_codex`/native Codex 0.161.0. Other
authentication methods are not inferred. Native bootstrap/TLS environment names
cannot carry secrets because Codex forwards those names to unrelated stdio peers.
Actual read/resource/write enforcement belongs to the qualified server and its
credential issuer. A metadata label, tool annotation or prompt does not grant or
enforce access. Local installation/connection withdrawal blocks new and retained
package use; remote expiry/revocation follows that server's enforcement contract.

### Actual native qualification

`tools.tests.test_factory_mcp_credentials` invokes the real plugin over its SDK
socket and the pinned native Codex executable. A launch shim only selects a
finite local Responses model fixture; it forwards all plugin-generated MCP
arguments and environment unchanged. It does not strip or reorder arguments.
HTTPS uses a temporary trusted certificate, both MCP peers are finite local
servers, external traffic is blocked by the fixture proxy configuration and no
paid inference or repository mutation occurs.

The two opt-in tests passed: distinct HTTP/stdio/model credentials; reads of
resource `alpha`; rejection of foreign resource `beta` and a write; missing,
expired and revoked HTTP credentials without a model/Reporter fallback; and no
credential canaries in plugin responses or retained public run records. The
stdio peer deliberately mislabels `write` as read-only: actual token enforcement
still refuses it. This tests access independently of model/tool annotations.
The ordinary direct peer test also verifies read, resource and write refusal.

Run the native tests with explicit pinned binaries:

```shell
OCTACITY_TEST_CODEX_BIN=/absolute/path/to/codex \
OCTACITY_TEST_CODEX_PLUGIN_BIN=/absolute/path/to/octa_plugin_codex \
python3 -m unittest tools.tests.test_factory_mcp_credentials
```

This isolates task 1.4's plugin/native credential boundary. The packaged
Microsandbox/stand journey is still 3.1; comprehensive retained native evidence
and cancellation qualification remain 1.5. No other AI plugin is certified.

### TDD and dependency status

The following tests were observed Red before implementation and then Green.
Use `--locked`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`,
`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2` for Cargo commands.

| Command/test filter | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server-factory --test mcp_authorization` | Missing authorization contract, then missing Runtime field | Independent immutable reference/purpose/resource retained |
| `cargo test -p octacity-protocol --lib v3_binds_independent_mcp` | Missing managed credential map | Exact independent signed profile grants |
| `cargo test -p octacity-job --lib independent_mcp_credentials` | `SecretProfile` refuses independent profiles | Only selected model/HTTP/stdio private variables reach Runner |
| `cargo test -p octacity-job --lib removed_mcp_credential` | Missing file-backed profile API | Deleted profile blocks before input I/O or backend start |
| `cargo test -p octacity-runner --test private_variables independent_credential_selection` | Reused model token accepted | Reuse, ambiguous input and aggregate budget overflow rejected |
| `cargo test -p octacity-server-application --lib role_mcp_authentication` | Independent grants fail compilation | Exact required-secret declarations and transport destinations |
| `cargo test -p octacity-server-application --lib stored_role_source_resolves_only` | No secret locators, then hyphenated alias invalid, then retained use survives withdrawal | Selected references resolve, portable aliases compile and withdrawn authority blocks retained/new invocation |
| `cargo test -p octacity-server-application --lib authenticated_runtime_settings_require` | Missing qualified support API, then withdrawn peer still selectable | Exact destination approval; withdrawal removes choices and retains history |
| `cargo test -p octacity-server-application --lib installed_model_policy_accepts` | Dedicated MCP grant rejected | Separate MCP ceiling accepted; other credential consumers rejected |
| `cargo test -p octacity-server-application --lib mcp_tokens_cannot_use_native` | `LANG`, then Windows/bootstrap names accepted as secrets | Inherited Unix/Windows/TLS names rejected |

Actual native qualification exposed two upstream Octa 0.7.2 incompatibilities
that the earlier finite CLI fixtures did not model: the root
`--ask-for-approval` flag was placed after `exec`, and a normal final
`item.completed` assistant message was not accepted as the answer preceding
`turn.completed`. The operator explicitly approved extending task 1.4 to fix
Octa and update the dependency. Public PluginManager tests reproduced both
failures before the fixes. The native grammar test and plain/schema result
completion test now pass; the complete Codex plugin regression and library/test
Clippy pass too. The result fix retains only the last bounded sanitized assistant
message, preserves raw trace events and reuses existing result/schema validation.

The initial candidate required an immutable qualified dependency pin before
closing task 1.4. The operator approved committing/pushing Octa and
preparing patch release 0.7.3. The fixes are committed as `a4eda07`, matching
package/lockfile versions and release notes as `1b8d6f9`. GitHub security audit
then exposed four vulnerabilities in the existing zstd chain; native audit was
observed failing before updating zstd to 0.14.3, zstd-safe to 8.2.0 and zstd-sys
to 2.1.2. Audit now reports zero vulnerabilities (two existing unmaintained
warnings); all 159 cache tests passed both before and after the upgrade, and
cache Clippy/release metadata checks pass. That fix is committed as `3d06163`.
GitHub security audit passed at `3d06163`. Windows coverage on the earlier
`1b8d6f9` candidate completed tests but failed the unchanged 95.7% threshold
with 95.67%. The new process test omitted the ordinary graceful plugin shutdown,
so process-owned LLVM counters could be lost on forced disposal. Commit
`310f838` uses the same explicit `shutdown_all()` as existing public PluginManager
tests and adds public malformed/schema/oversized native-answer rejection and
secret-redaction characterization. No production behavior or coverage threshold
was changed. All 142 plugin tests and all-target plugin Clippy pass locally;
GitHub coverage at that commit passed on Linux, macOS and Windows. Windows CLI
tests then exposed an independent completion-delivery race: a short command
printed its successful output but reported `Connection closed` instead of
consuming the queued `Completed` response. A public PluginClient regression
keeps the local socket open and sends `Started`, `Stdout`, `Completed` for 2,000
successive commands. It reproduced the same failure in three of three local
runs before the fix. The auxiliary progress sender closes when the command
route is removed; the receiver must recheck the reliable response queue rather
than treat that closure as a transport failure. The candidate fix preserves
cancellation and overflow priority and passes that regression in three further
runs. Commit `1492f0d` includes the fix and its regression. All 73 plugin-manager
tests and 142 Codex plugin tests pass, as do all-target manager Clippy, format
and release metadata checks. The original
`test_global_default_plugin_for_short_commands` also passes (five repeated
executions after its initial Green). GitHub [Tests](https://github.com/OctaHive/octa/actions/runs/40654965917)
and [Coverage](https://github.com/OctaHive/octa/actions/runs/40654965943) passed at
`1492f0d` on Linux, macOS and Windows; Windows production line coverage is 95.75%
against the unchanged 95.7% threshold. The Cargo manifest/lock are unchanged from
the audited `3d06163` dependency graph. Annotated tag `v0.7.3` points to the
verified `1492f0d26827b06446c72d955c18e0d42b30812a` commit. Its existing
[Build workflow](https://github.com/OctaHive/octa/actions/runs/40656456795) passed
all five platform builds, package contracts and archive attestation. The operator
published [Octa v0.7.3](https://github.com/OctaHive/octa/releases/tag/v0.7.3).
Anonymous release metadata confirms a public, non-prerelease version with all
five archives and five checksum files uploaded.

### Published dependency pin and native requalification

OctaCity now pins 0.7.3 at
`1492f0d26827b06446c72d955c18e0d42b30812a` in the workspace dependencies,
CI source revision, stand input manifest, staging allowlist and runner defaults.
Cargo.lock changes only the three path-package versions. The immutable source
archive SHA-256 is
`f4ee86aeb4e444455669ce94d06bced02c96d7c9fe7329eddcea0272626d9cad`;
the Linux ARM64 release archive SHA-256 is
`4eceb32d3378a8c8ebfb0ffe10984c86f531e39105b31f6f73af9132a8891fb6`.

The existing input-pin test was observed Red against 0.7.2 before updating the
manifest. Existing native installation-name and staging-allowlist tests also
failed against the old version, then passed after the coordinated pin update.
Commands:

```shell
python3 -m unittest tools.tests.test_verify_local_stand_inputs.LocalStandInputTests.test_pins_the_released_native_mcp_qualified_octa
python3 -m unittest tools.tests.test_stage_local_stand_native.NativeStandStagingTests.test_names_bind_every_native_version_and_the_agent_revision
python3 -m unittest tools.tests.test_verify_local_stand_context.LocalStandContextTests.test_staging_allowlist_tracks_the_immutable_manifest
```

`tools/stage_octa_release.py --version 0.7.3 --revision
1492f0d26827b06446c72d955c18e0d42b30812a` succeeded separately for
`--platform linux-arm64` and `--platform macos-arm64`, validating archive/sidecar,
release and Codex compatibility contracts and the exact embedded build revision.
The two native credential tests above then passed again with the published
macOS package's `plugins/octa_plugin_codex`, rather than a workspace-built plugin,
and the pinned native Codex 0.161.0. Source download limits remained enforced.
This closes the dependency blocker; full packaged Microsandbox execution and
native call/result retention remain tasks 3.1 and 1.5 respectively.

The initial offline lock update could not resolve a missing registry-index entry;
the online targeted update succeeded. Its unrelated tempfile dependency-edge
rewrite was discarded before locked Cargo regression. Only the three Octa
package versions changed. These environment/setup issues are not behavioral Red.
Free disk space remained approximately 14 GiB during the final pin qualification;
existing target directories were reused and no retained stand state was deleted.


### Task 1.4 regressions

| Command | Result |
| --- | --- |
| `cargo test -p octacity-server-application --lib factory_codex` | 60 passed |
| `cargo test -p octacity-server-application --lib factory_role_materialization_tests` | 38 passed |
| `cargo test -p octacity-server-application --lib factory_runtime_settings_tests` | 13 passed |
| `cargo test -p octacity-job -p octacity-runner -p octacity-protocol --lib` | 53 + 52 + 74 passed |
| `cargo test -p octacity-server-factory --test mcp_authorization -p octacity-runner --test private_variables -p octacity-agent --test cli` | 2 + 9 + 10 passed |
| `cargo clippy -p octacity-server-application -p octacity-server-factory -p octacity-protocol -p octacity-job -p octacity-runner -p octacity-agent --lib -- -D warnings` | Passed |
| `cargo check -p octacity-server --lib` | Full server/API/store composition passed |
| `cargo check -p octacity-server -p octacity-release-harness -p octacity-server-store-postgres --tests` | Passed after correcting the managed-field insertion in a test fixture |
| `cargo test --manifest-path ../octa/Cargo.toml -p octa_plugin_codex --target-dir target` | 141 passed; one explicit native-binary test ignored by default |
| `OCTA_TEST_CODEX_NATIVE=/absolute/path/to/codex cargo test --manifest-path ../octa/Cargo.toml -p octa_plugin_codex --test plugin_manager --target-dir target native_cli_accepts_global_approval -- --ignored` | Native argument grammar passed |
| `cargo clippy --manifest-path ../octa/Cargo.toml -p octa_plugin_codex --all-targets --target-dir target -- -D warnings` | Passed |
| `python3 -m unittest tools.tests.test_factory_pilot_cli tools.tests.test_local_stand_documentation` | 18 passed |
| Explicit native qualification command above | 2 passed |
| Format checks for both workspaces, architecture, strict OpenSpec and `git diff --check` | Passed; architecture: 63 packages, 261 edges |

The initial broad application/Agent regression had 788 application tests pass
and two failures in the subsequently fixed bootstrap-secret guard and obsolete
model-token Runtime fixture. The final targeted suites above include both
corrections. Compilation/fixture setup errors were repaired before evaluating
behavioral Red; they are not counted as feature failures. The known existing
unused application test-helper warning remains. Documentation changes have no
executable behavior and were checked with documentation/OpenSpec validation.

After the published pin update, the same relevant boundaries were verified with
the 0.7.3 path packages and lockfile:

| Command | Result |
| --- | --- |
| `python3 -m unittest tools.tests.test_verify_local_stand_inputs tools.tests.test_stage_local_stand_native tools.tests.test_verify_local_stand_context tools.tests.test_stage_octa_release tools.tests.test_validate_octa_release_contract` | 66 passed |
| `cargo test --locked -p octacity-protocol -p octacity-job -p octacity-runner --lib` | 74 + 53 + 52 passed |
| `cargo test --locked -p octacity-server-application --lib factory_codex` | 60 passed |
| `cargo test --locked -p octacity-server-application --lib factory_role_materialization_tests` | 38 passed |
| `cargo test --locked -p octacity-server-application --lib factory_runtime_settings_tests` | 13 passed |
| `cargo test --locked -p octacity-server-factory --test mcp_authorization -p octacity-runner --test private_variables -p octacity-agent --test cli` | 2 + 9 + 10 passed |
| Native credential qualification above with the published macOS plugin | 2 passed |
| The six-package library Clippy command above with `--locked` | Passed |
| `cargo check --locked -p octacity-server --lib` | Full server composition passed |

The final pin qualification started with 14 GiB free and finished with 13 GiB.
No local image build or stand restart was needed for this boundary; the complete
packaged stand journey remains task 3.1. No OctaCity commit was created.
