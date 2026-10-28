# Issue MCP boundary verification — task 4.2

Verified locally on 2026-10-28, on the work tree following `0b566e4e`.
This is the public server-side MCP foundation. It does not qualify a native AI
plugin or complete model-driven triage, MCP capture or reporting (tasks 4.3–4.4
and subsequent tasks remain separate).

## Public boundaries and ownership

- `server/api/octacity-server-api-mcp/tests/issue_mcp.rs` sends native JSON-RPC
  HTTP requests through the actual router and existing authorized Issue reads.
  Its ordinary Create/Edit/Get application handlers supply and verify real
  in-memory Issue revisions. Paused/unavailable query ports exercise controlled
  asynchronous dependency failures without mocking private implementation calls.
- `server/app/tests/process_lifecycle.rs` starts the normal server and sends
  real HTTP requests. Independent private files verify token purpose, Project /
  Issue read scope, expiry, rotation and revocation. PostgreSQL is deliberately
  unavailable; no fake Issue query handler establishes this installation check.
  Successful Issue search and exact revision behavior are established at the
  public MCP boundary above. No packaged or live-provider claim follows from
  this finite composition test.
- Gateway contract tests check the exact shared `/mcp/issues` route, independent
  bearer forwarding and retained ordinary Console credential stripping. These
  are configuration contract checks, not a live Nginx/container qualification.

`IssueReadPrincipal` owns the closed Project/Issue/View policy and query-bound
pagination over existing `IssueHandlers` and storage visibility. The composition
root reuses the existing private runtime-file loader, zeroing credential input
and fixed-size digest comparison. Process configuration holds only installation
metadata and private-file references, without a new credential database, issuance
API, global search index, duplicate decision endpoint or external MCP wrapper.
Protocol sessions retain bounded correlation metadata; a session ID never grants
Issue access or replaces current credential verification.

## Observed Red → Green slices

Each behavioral change was driven by a failing public-boundary test; passing
characterizations are identified separately. Initial fixture/compiler setup
errors were repaired before recording behavioral Red. Introducing the new public
MCP/application API first produced unresolved API imports, followed by observable
HTTP failures as each operation was added.

| Slice / focused filter | Observed Red | Observed Green |
| --- | --- | --- |
| `scoped_connection_initializes` | HTTP 501 instead of successful MCP initialization | Scoped native initialization |
| `scoped_connection_discovers` | No successful native tool catalogue | Exactly two bounded read-only tools |
| `title_search_filters_hidden` | Missing scoped search result | Visible matching summaries and page coverage, unchanged Issue revisions |
| `current_credential_project_visibility_and_expiry` (initial revocation case) | Revoked outstanding read returned HTTP 200 | HTTP 401 with no result/coverage; later characterization adds Project, visibility and expiry changes |
| `detail_read_preserves_exact` | Exact retained revision unavailable | Original version/body preserved after ordinary edit |
| `title_search_paginates` | Continuation rejected | Empty matching pages retain continuation; changed query rejected |
| `hidden_absent_and_foreign` | Missing opaque tool failure | Same result for hidden, foreign and absent IDs/current or retained versions |
| `unavailable_search` / `unavailable_detail` | Protocol failure or wrong availability classification | Explicit tool failure, never a successful empty page |
| `mutation_tools` | Unknown write tool classified as method error | Unknown/mutation tool and caller Project overrides rejected as invalid tool requests |
| `cancellation_stops` | HTTP 200 instead of accepted cancellation | Pending read stops, releases request ID and returns no page |
| `stateless_http_accepts` | Initialized notification returned a response | Accepted notifications/ping and denied browser Origin |
| `malformed_envelopes` | Invalid IDs produced a successful catalogue; a later focused Red caught invalid IDs echoed into error responses | Bounded envelopes and arguments rejected, with a valid null response ID for malformed IDs |
| `stalled_reads` | Read remained pending after virtual 31 seconds | Whole-request HTTP 504 deadline, no payload |
| `oversized_detail` | Valid Issue produced more than 256 KiB encoded response | Explicit `too_large`, no truncated successful document |
| `active_reads` | 65th read joined an unbounded stalled queue | HTTP 429 at 64 active reads; cancellation remains usable |
| `native_http_enforces` | `text/plain` request returned HTTP 200 | Content type/version checks and non-cacheable responses |
| `initialization_negotiates` | Older initialization had no supported version result | Explicit negotiation, malformed initialization rejection and ignored unknown notifications |
| `independent_native_clients` | No independent protocol session correlation | Parallel same-ID calls and isolated cancellation |
| `issue_mcp_connections_require` | Valid scoped installation configuration rejected | Strict references/read scope accepted; missing Project, inline token, writes and invalid bounds rejected |
| `configured_issue_mcp` | Real server returned HTTP 404 | Independent tokens, expiry, empty Issue scope, rotation and revocation verified over HTTP |
| `offline_issue_mcp` | Offline validator read the former live credential path | Existing runtime-file rebase includes MCP credentials |
| Gateway route contract | Exact MCP location absent | Exact native route, no SPA fallback or change to ordinary credential handling |

Additional passing characterizations verify that trusted-network/header identity
cannot authenticate MCP, another verified connection cannot cancel this reader,
and protocol sessions are time-bounded, connection-bound and rejected after
credential revocation. Existing behavior tests remained passing during extraction
of protocol, tool projection and pending-correlation modules and the Clippy-driven
replacement of a large HTTP error value with a status code.

Focused Red commands used:

```sh
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test --locked \
  -p octacity-server-api-mcp --test issue_mcp <focused_filter>
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test --locked \
  -p octacity-server --test process_lifecycle <focused_filter>
python3 -m unittest tools.tests.test_console_proxy_contract.IssueMcpProxyContractTests
```

The initial new-crate dependency setup used the same Cargo commands without
`--locked` once to add the workspace package. No new external dependency or
architecture-policy exception was added. The only later gateway regression was
an existing second contract's exact proxy-route count; its expected count was
updated from four to five for the newly tested MCP route.

## Validation

See [the endpoint contract and installation guide](../../../docs/reference/issue-mcp.md)
for exact protocol, bounds, result coverage, availability and token semantics.
Documentation and archive/link edits have no executable behavior; validate their
links, OpenSpec consistency and formatting rather than fabricate TDD results.

Observed final checks:

```sh
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test --locked \
  -p octacity-server-api-mcp -p octacity-server \
  --test issue_mcp --test process_lifecycle --test configuration_examples
# 22 MCP + 11 real-server/configuration + 2 configuration-example tests passed.
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test --locked \
  -p octacity-server-application --lib issue
# 168 passed.
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test --locked \
  -p octacity-server-application --lib management_security
# 21 passed.
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test --locked \
  -p octacity-server --lib runtime::admission
# 6 passed, after repairing the shared fixture compilation described below.
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo clippy --locked \
  -p octacity-server-api-mcp --all-targets -- -D warnings
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo clippy --locked \
  -p octacity-server --lib --test process_lifecycle -- -D warnings
python3 -m unittest tools.tests.test_console_proxy_contract \
  tools.tests.test_build_local_stand_gateway
# 18 passed.
python3 tools/check_architecture.py
# 64 workspace packages, 263 dependency edges accepted.
cargo fmt --all -- --check
openspec validate add-factory-triage-pipeline --strict --no-interactive
git diff --check
```

The capacity and deadline tests use paused Tokio time, avoiding short wall-clock
CI timing assertions. No coverage or bundle thresholds were changed.

### Existing shared-fixture compilation repair

The final server unit-test compilation exposed an error in the previous GitHub
fixture change: `env!("CARGO_BIN_EXE_octacity-github")` was inside a support module
also included by server library tests. Cargo supplies that variable only to the
binary-owning package's integration tests. This compilation blocker was not
counted as behavioral Red for MCP.

Before refactoring the fixture, both native GitHub test targets were Green
(22 listing + 3 preview tests). The support helper now receives the executable
path explicitly; only the owning integration-test call sites use Cargo's binary
variable. The same 25 tests remained Green, and the six server admission tests
compiled and passed. This is a behavior-preserving fixture refactor; no timeout,
provider implementation or executable-hashing policy changed.

```sh
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test --locked \
  -p octacity-github --test issue_preview --test issue_listing
# Before: 25 passed. After: 25 passed.
```

## Disk and limits of evidence

Builds used two Cargo jobs and disabled incremental compilation. At the 10 GB
threshold, the inactive native-release intermediates were removed earlier; a
later crossing was handled with `go clean -cache` after confirming there was no
Go build/test process. Active Cargo artifacts, stand data, private credentials,
installed executables and unrelated work were preserved. Free space recovered
to approximately 17 GB; approximately 13 GB remained after final compilation.

No paid model, live GitHub mutation, Agent-side native MCP execution or full
packaged Nginx/server/Agent pilot was performed for this task. The server protocol
and installation checks do not mark tasks 4.3, 4.4 or packaged qualification done.

## Review corrections: explicit detail projection and session lifecycle

The detail response now selects its wire fields explicitly in the MCP adapter,
including content and optional planning. No response-format change was needed.
The public exact-revision test now compares an independently specified JSON
object, rather than serializing the store revision as its expected value. A
characterization covers nonempty planning and acceptance/priority. Existing 22
tests passed before the refactor; all 23 contract/behavior tests passed before
and after it. This is behavior-preserving refactoring, so a fabricated failing
behavior test would not be appropriate.

Three subsequent public-boundary Red/Green slices fixed session behavior:

| Focused filter | Red | Green |
| --- | --- | --- |
| `session_termination` | DELETE returned 405 without ownership checks | Current-owner DELETE returns 204, releases the session and cancels only its outstanding reads; foreign/revoked access is denied |
| `session_capacity` | 65th live session from one connection returned 200 | Per-connection ceiling returns 429 while another credential/connection can still initialize; terminating a session immediately releases its slot |
| `a_session_closed` | A paused real HTTP request returned 200 with a search page after session termination | It returns 404 with no page; admission validates the session under the same lock as termination |

The capacity test also completes 1,025 sequential initialize/DELETE cycles,
exceeding the global ceiling without accumulating finished sessions. The delayed
body test uses a real loopback HTTP server and `Expect: 100-continue` as an explicit
body-read barrier; it needs no sleep, test-only production hook or new dependency.
Session and read maps share one private mutex so closure and read registration
have one owner. Connection capacity is computed from the bounded session map,
without a second quota counter, registry or background cleanup worker.

Commands and final checks for these corrections:

```sh
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test --locked \
  -p octacity-server-api-mcp --test issue_mcp <focused_filter>
# Red and Green observed for each behavior correction above.
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test --locked \
  -p octacity-server-api-mcp
# 26 passed.
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test --locked \
  -p octacity-server --test process_lifecycle --test configuration_examples
# 11 + 2 passed.
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo clippy --locked \
  -p octacity-server-api-mcp --all-targets -- -D warnings
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo clippy --locked \
  -p octacity-server --lib --test process_lifecycle -- -D warnings
python3 -m unittest tools.tests.test_console_proxy_contract \
  tools.tests.test_build_local_stand_gateway
# 18 passed.
python3 tools/check_architecture.py
# 64 packages / 263 edges accepted; no dependency or policy addition.
cargo fmt --all -- --check
openspec validate add-factory-triage-pipeline --strict --no-interactive
git diff --check
```

Both Clippy runs, formatting, architecture, strict OpenSpec validation and updated
local documentation links passed. Free disk remained above 13 GB during these
corrections; no cache or stand-data cleanup was necessary.

The existing 4.3/4.4 and packaged qualification limits above remain unchanged.

## Windows fixture correction — 2026-10-29

The reported Windows CI run supplied Red for
`issue_mcp_connections_require_explicit_project_read_scope_and_private_credential_references`:
the positive fixture used `/run/secrets/issue-mcp-triage`, which has no drive or
UNC prefix and therefore is not absolute on Windows. The production
`Path::is_absolute` requirement was correct and remains unchanged.

The fixture now obtains a native absolute reference from a test-owned temporary
directory and serializes it with the existing TOML string serializer. The
relative-path, scope, expiry, duplicate-reader, inline-token and write-field
negative cases remain intact. The original test passed on macOS before this
test-only correction. Afterward:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -j2 -p octacity-server --test process_lifecycle
# 11 passed, 0 failed, 0 ignored on macOS.
cargo fmt --all -- --check
git diff --check
# Both passed.
```

Windows execution is unavailable on this host; the supplied CI failure is the
Windows Red observation, and Windows Green still requires rerunning that job.
Approximately 40 GiB remained available; no cleanup or commit was performed.
