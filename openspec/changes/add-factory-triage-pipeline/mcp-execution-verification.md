# Task 4.3 — isolated native MCP execution

## Consumed installation contract

This task consumes the archived `enable-local-stand-runtime-mcp` installation
evidence rather than introducing another MCP client, launcher, secret store or
provider wrapper:

- [1.2 / 1.4 execution and independent credentials](../archive/2026-10-28-enable-local-stand-runtime-mcp/mcp-execution-verification.md):
  exact HTTP/stdio settings, signed per-connection private-variable bindings,
  independent credential purposes/resources, executable pins and Agent revalidation.
- [1.5 native capture and cancellation](../archive/2026-10-28-enable-local-stand-runtime-mcp/mcp-evidence-verification.md):
  the ordinary version-2 native trace/provenance contract, redaction and partial
  cancelled observations without fabricated results.
- [3.1 actual packaged Agent qualification](../archive/2026-10-28-enable-local-stand-runtime-mcp/packaged-mcp-verification.md):
  macOS ARM64 Agent → Microsandbox 0.7.6 → Linux ARM64 Codex 0.161.0 → published
  Octa 0.8.0, through ordinary installation composition, Runtime/Profile
  publication, managed Builds, independent credentials and restart recovery.

The selected plugin supplies the required transport/authentication/capture
support. There is no missing upstream capability for this selected Codex
qualification. Claude/other AI plugins and particular ready-made external server
releases remain unqualified prerequisites for deployments selecting them. A
configuration row or compatible protocol name does not establish that evidence.

## Delivered qualification boundaries

`factory_triage_profile_tests` compiles and materializes separate Project and
Systems analyst packages through the ordinary Build/publisher contract. Each
receives only its declared immutable Context. The analyst receives its explicit
typed handoff and own document, without Project-private investigation text or an
inherited MCP connection. Extra undeclared Context bytes fail compilation. The
existing reviewed-goals/rules tests and common committed-record/input projection
tests remain the owners of required Project context and accepted-only handoff;
no role name grants tools or changes the code Repository.

`server/api/octacity-server-api-mcp/tests/native_triage.rs` starts the real MCP
router and ordinary authorized Issue handlers, then invokes
`tools.tests.test_factory_triage_mcp` with the actual Codex plugin and native CLI.
The Python HTTP fixture only terminates test TLS and forwards the native MCP
requests to that router; it implements no search or Issue authorization. The
store is the existing in-memory application test store and its explicit read
principal is restricted to one visible Issue. Deployment token-file verification
remains covered by task 4.2's real-server tests.

The finite Responses peer prescribes six actual native operations: OctaCity title
search, exact candidate read, hidden candidate refusal, external stdio read,
external write refusal and refusal of a caller-supplied foreign Project. The
candidate body contains instructions to borrow Reporter authority and write;
actual server/credential restrictions still reject the attempted write. Hidden
content and model/HTTP/stdio/Source/Reporter token canaries stay out of retained
native records and plugin responses. The stdio peer independently rejects
credentials belonging to other purposes and enforces its `alpha` resource scope,
even though its write tool deliberately claims a read-only annotation.

The native client is given a real ambient HOME MCP configuration. Model requests
advertise a nonempty catalogue containing only the two selected connections.
Revoking the OctaCity connection then blocks a new native invocation before any
model request or external tool call, without substituting model/Reporter tokens.
Both original Issues remain unchanged through ordinary authorized
reads after the investigation. This is finite protocol/authority qualification,
not a claim that a live model resists every semantic prompt injection.

The host-native executables used on 2026-10-28 were:

| Executable | Version | SHA-256 |
| --- | --- | --- |
| Codex macOS ARM64 | 0.161.0 | `12ac11d2c7eee27cfae34393986d7b7c9ed0dea537cb749831cdd7033893e6de` |
| Released Octa Codex plugin | 0.8.0 | `941858c2f4e32de77799c26a835d155d5b55d0096ac041d5227048e78e190766` |

The launch shim changes only the model provider to a finite local Responses
fixture and forwards the plugin's native MCP settings/environment unchanged.
It performs no paid model call or live repository mutation.

## Observed Red → Green

All application Cargo commands below use `CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`, `--locked` and `-j2`.
The existing materialization suite passed 39 tests before changes; the existing
four actual native credential/capture/cancellation tests also passed first.

| Public test filter | Observed Red | Green behavior |
| --- | --- | --- |
| `triage_connection_revoked_during_context_read` | Materializer returned a dispatchable ordinary Build request after the connection was revoked during Context I/O | Current dependency authority is rechecked after asynchronous Context/settings reads and before retaining/publishing compiler input |
| `triage_connection_revoked_during_publication` | Materializer returned a dispatchable Build request after publication revoked the connection | A final check follows input publication before returning the ordinary Build request; recovery cannot restore withdrawn authority |
| `a_role_source_without_connection_authority` | The Role-only source's default dependency check returned success for selected MCP | Default authorization refuses any selected MCP/integration unless its source implements current connection checks |

Each filter was run with:

```sh
cargo test --locked -j2 -p octacity-server-application --lib <filter>
```

The retained-Runtime replacement regression previously used an unavailable
Role-only source as implicit anonymous-MCP authorization. Its fixture now delegates
only current authority to the actual stored invocation source and panics on any
attempt to reload the Role catalog. Exact old transport pins and protected-input
identities still survive replacement; availability is no longer fabricated by a
default method.

New native/Context qualification is characterization of existing behavior, not
fabricated product Red evidence. Initial new-fixture API/selection errors and an
overly narrow expectation that revoked native startup must return `Completed`
rather than the supported `Error` are test corrections, not behavioral Reds.

## Verification and limits

All scoped behavioral regressions below passed. The same Cargo environment and
`--locked -j2` apply. These are 504 distinct tests; repeated baseline/Green runs
are not added to that count.

| Command | Observed result |
| --- | --- |
| `cargo test -p octacity-server-application --lib factory_role_materialization_tests` | 43 passed |
| `cargo test -p octacity-server-application --lib factory_codex` | 64 passed |
| `cargo test -p octacity-server-application --lib factory_node_build_tests` | 30 passed |
| `cargo test -p octacity-server-factory --lib` | 303 passed, including committed-record and closed-context/data projections |
| `cargo test -p octacity-job -p octacity-runner -p octacity-protocol --lib mcp` | 2 Agent Job + 2 executable inventory + 1 signed credential-binding tests passed |
| `cargo test -p octacity-server-api-mcp --test issue_mcp` | 26 passed |
| `cargo test -p octacity-server-api-mcp --test native_triage -- --ignored --nocapture` | Actual native OctaCity/stdio qualification passed; explicitly executed, not skipped |
| `python3 -m unittest tools.tests.test_factory_mcp_credentials.NativeMcpCredentialTests` | 4 actual native tests passed, including cancellation and partial evidence |
| `python3 -m unittest tools.tests.test_factory_pilot_launcher tools.tests.test_factory_pilot_cli tools.tests.test_console_proxy_contract` | 28 passed |

The native commands require `OCTACITY_TEST_CODEX_BIN` and
`OCTACITY_TEST_CODEX_PLUGIN_BIN` to select the exact executables above. Application
test compilation retains only the pre-existing unused
`pending_factory_profile_fixture` warning. Documentation changes have no
executable behavior and use link/OpenSpec/format checks.

Static verification passed:

```sh
cargo clippy --locked -j2 -p octacity-server-application --lib --tests
# Only the pre-existing unused test-fixture warning above.
cargo clippy --locked -j2 -p octacity-server-application --lib -- -D warnings
cargo clippy --locked -j2 -p octacity-server-api-mcp --all-targets -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
# 64 workspace packages / 263 dependency edges.
openspec validate add-factory-triage-pipeline --strict --no-interactive
git diff --check
```

All seven local links in the edited reference/evidence documents resolve. No new
dependency, persistence migration, permission grant or commit was introduced.

The actual combined packaged Agent transport journey is consumed from upstream
3.1; this task does not rerun or claim a new full PostgreSQL/VM/two-stage preset
journey. The new native test directly qualifies OctaCity MCP plus the finite
external peer. Full packaged triage remains 8.1–8.3; semantic evidence-reference
verification remains 4.4/5.2, and the complete two-stage preset remains 5.1.
No GitHub/GitLab/Jira server release or other AI plugin is certified by the finite
peer. Remote revocation follows each server's contract; local checks do not claim
an atomic cross-provider authority snapshot.

Revocation during publication can leave the original private compiler documents
under the existing fenced publication/retention owner. It returns no dispatchable
Build request and adds no separate recovery or cleanup journal. Ordinary recovery rechecks
authority without replacing retained package content.

Disk space was checked throughout. Builds reused `target` with two jobs, disabled
incremental compilation and disabled debug information. No stand data, private
credential files, installed executable or unrelated work was deleted.
Final available space was 13,368,068 KiB (approximately 12.7 GiB), remaining above
the 10 GB cleanup threshold throughout the task.
