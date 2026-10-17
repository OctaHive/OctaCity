# Task 5.5 verification

## Delivered boundary

Read-only connection diagnostics use explicitly advertised `check` operations in
the existing independently versioned Source and Reporter families. No new
provider enum, protocol family, credential store, worker or database migration is
introduced. Old manifests default to no check capability. Separate role classes,
verified executable/profile pins, original native identities and the installed
endpoint/network ceiling remain mandatory before private credential access.

The provider-neutral application query requires connection Administer, View and
Execute rights before reading metadata or dispatching. It selects an exact
resource and reviewed current metadata version, returns safe native resource and
capability readiness, and rejects stale selections rather than redirecting them.
The handler holds only connection/administration read ports and a checker port.
Configured inactive connections can be checked without activation. Missing managed
credentials never fall back to an unadopted private file or another role.

Trusted runtime composition resolves immutable installation/resource/capability/
configuration pins through the verified process host. Ambiguous profiles and zero
process bounds are rejected. Invalid installed scope, unsupported operations,
provider throttling and unavailability have distinct safe readiness outcomes.
Readiness is an observation, never a permission grant. VCS/Delivery checks are
explicitly Unsupported here and cannot borrow task credentials.

The protected opt-in REST boundary is POST
`/api/v1/integrations/{integration_id}/checks`, with only `version`, `resource` and
`purpose`. It uses a separately authorized query, no publication/idempotency
headers, verified protected ingress and `Cache-Control: no-store`. Unknown fields,
including attempted credential submission, fail without echoing input. OpenAPI
describes the closed request and credential-free result.

GitHub checks use GET only: repository metadata authenticates and confirms the
original native repository id; Source then reads at most one Issue-list page with
`per_page=1`. Reporter uses its own write credential for the metadata GET and
leaves write readiness Unverified even when metadata claims push/admin access.
No mutation is used to prove permission. Existing HTTP body bounds apply; the
diagnostic wire response has its own 8192-byte process/decoder ceiling, finite
deadline and same-check cancellation. Provider retries are not automatic.

No check can publish Project grants, activate polling, admit Work, advance a
production cursor, retain task snapshots or create/update an internal Issue.
Manual Factory intake requires none of this configuration. Input selection and
its composition remain task 5.6; Console connection setup remains subsequent UI
work. Native GitLab/Jira adapters and persistent internal Issues are not claimed.

## Test-first evidence

All commands below used `--locked` and this resource-bounded environment, with
one Cargo build at a time:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

The pre-refactor baseline was 68 passing tests:

```sh
cargo test --locked -p octacity-work-source-protocol -p octacity-server-work-source -p octacity-github --test contract --test host --test work_credentials --test issue_listing
```

Each row records an observed Red before its implementation and a subsequent Green.
Missing-API compilation failures refer to the newly required public seam, not a
broken environment. Intermediate compilation mistakes were corrected and were not
counted as behavioral Red evidence.

| Command | Observed Red | Green |
| --- | --- | --- |
| `cargo test --locked -p octacity-work-source-protocol --test connection_checks` | Closed decoder rejected `check` | Both role classes, correlation, prerequisite readiness and small response bound pass |
| `cargo test --locked -p octacity-server-work-source --test host read_only_connection_checks` | Manifest rejected explicit `check` advertisement | GitLab/Jira/custom process peers retain exact scope; unadvertised checks never spawn |
| `cargo test --locked -p octacity-github --test connection_checks` | Source returned Failure instead of Checked | Native HTTPS read readiness; only bounded GETs and no retained task payload |
| `cargo test --locked -p octacity-github --test connection_checks write_check` | Reporter returned Failure instead of Checked | Independent writer credential; capability remains Unverified |
| `cargo test --locked -p octacity-github --test connection_checks checks_reject` | Substituted endpoint reported configuration Ready | Endpoint/network/profile failures precede authenticated HTTP |
| `cargo test --locked -p octacity-github --test connection_checks missing_role` | Missing credential incorrectly reported invalid configuration | Ready configuration, Missing authentication, no fallback or HTTP |
| `cargo test --locked -p octacity-github --test connection_checks checks_distinguish` | HTTP 401 left authentication Unverified | Authentication denial, resource denial, throttling and outage stay distinct |
| `cargo test --locked -p octacity-github --test process missing_check_configuration` | Missing installation returned generic Failure | Safe correlated Missing configuration from the real binary |
| `cargo test --locked -p octacity-github --test connection_checks unsupported_write` | Absent Reporter role reported invalid configuration | Unsupported capability without credential borrowing or HTTP |
| `cargo test --locked -p octacity-github --test connection_checks cancelled_checks` | Pre-cancelled request still checked configuration | Pre-cancel and in-flight cancellation stop the original check |
| `cargo test --locked -p octacity-github --test connection_checks provider_process_cancels` | Real binary rejected same-check cancellation | Both roles return safe Unavailable and perform no follow-up request |
| `cargo test --locked -p octacity-server-application connection_checks_require --lib` | Required public query/checker API absent | Separate rights and missing credential behavior; administration/metadata/evidence unchanged |
| `cargo test --locked -p octacity-server-application checks_of_inactive --lib` | Unsupported purpose returned InvalidInput | Structured Unsupported; inactive check and stale-version rejection do not dispatch extra IO |
| `cargo test --locked -p octacity-server installed_connection_checks --lib` | Required hosted checker/profile API absent | Verified provider-neutral process checks without Factory inputs |
| `cargo test --locked -p octacity-server connection_checks_cannot_substitute --lib` | Substituted native identity still reported Ready | Native/adapter/logical-credential/purpose changes fail before spawn |
| `cargo test --locked -p octacity-server check_installation_rejects --lib` | Duplicate installed profiles silently replaced each other | Duplicate/zero-bound rejection; distinct immutable configurations coexist |
| `cargo test --locked -p octacity-server-api-rest --test integration_administration read_only_connection_check_rest` | Required `with_checks` endpoint API absent | Protected closed query, no credentials/publication headers, safe errors and OpenAPI |
| `cargo test --locked -p octacity-server --lib classified_provider_failures_remain` | Generic transient provider Failure lost Unavailable readiness | Closed failure categories/retry advice survive without implicit retry or another operation |
| `cargo test --locked -p octacity-server --lib invalid_installed_check_scope` | Host rejected profile was misreported as outage | Invalid installation is distinct from provider unavailability and never spawns |

Further qualifications exercise native-id replacement, actual GitHub binary HTTPS
checks for each role, and both check roles through the verified host's existing
deadline/pre-cancel/tree-cleanup contracts. These confirm shared behavior without
adding another production implementation. Provider peers are controlled fixtures;
no live GitHub account or user repository is contacted.

## Regression results

- `cargo test --locked -p octacity-work-source-protocol -p octacity-server-work-source -p octacity-server-api-rest`: 133 passed, including complete REST authorization inventory and OpenAPI drift checks.
- `cargo test --locked -p octacity-github --no-fail-fast`: final run passed all 107 tests, including 10 new check tests, 29 credential tests, existing Issue Source/Reporter and independent Delivery coverage. See the intermittent failure below; the successful repeat is not a claim that it was fixed.
- `cargo test --locked -p octacity-server --lib`: 145 passed; 8 service-dependent PostgreSQL tests ignored. This slice changes no database behavior; PostgreSQL service qualification from 5.4 is retained, not rerun or claimed here.
- `cargo test --locked -p octacity-server-application --lib integration_`: 7 passed.
- `cargo test --locked -p octacity-server-application --lib factory_admission_tests`: 60 passed, including manual intake, provider-neutral staged admission, replay and polling.
- `python3 tools/check_architecture.py`: passed, 63 packages and 253 dependency edges. No dependency or Cargo.lock changes remain.
- `python3 -m unittest tools.tests.test_check_architecture tools.tests.test_security_matrix`: 37 passed.
- `python3 tools/security_matrix.py validate` and `node ui/scripts/verify-architecture.mjs`: passed. This is inventory validation, not the full cross-platform security matrix.
- `cargo clippy --locked -p octacity-work-source-protocol -p octacity-server-work-source -p octacity-github -p octacity-server-application -p octacity-server-api-rest -p octacity-server --all-targets --all-features -- -D warnings`: passed.
- `pnpm --dir ui api:generate` with the same bounded Cargo environment and `pnpm --dir ui typecheck`: passed; generated API types remain ignored build artifacts.
- `cargo fmt --all -- --check`, `git diff --check`, `openspec validate add-github-work-source --strict` and `openspec validate add-factory-work-card --strict`: passed.

## Existing intermittent credential-test failure

Two intermediate parallel runs failed
`management::cleanup_after_process_death_erases_partial_private_writes` with
`ProviderCredentialError::Conflict` at the second sequential `discard` in
`tests/work_credentials/management.rs:78`. The isolated test and subsequent full
29-test credential runs passed, including the final complete GitHub suite. The
cause is not established and no fix is claimed. Its test, credential publication
and filesystem locking implementation are unchanged in this work. This remains a
separate flaky-test finding; no timeout increase, serialization, skip or unrelated
production change was introduced to obtain a green check run.

This finding was subsequently resolved in a dedicated TDD follow-up: explicit
scope-owned unlock prevents duplicate/inherited descriptors from extending the
credential lock. See [the lock lifetime regression report](verification-credential-lock.md)
for the deterministic Red/Green test and parallel qualification.

## Resource and scope preservation

Builds use two jobs, no incremental cache and no debug symbols. Free space and
`target` size are checked throughout. No live provider mutation, paid model call,
local-stand database change or disposable database service is part of this slice.
Existing completed task entries and all internal work-card planning artifacts are
preserved. Documentation-only updates have no executable behavior for TDD; their
validation uses the specification and repository checks.

At completion, approximately 18 GiB remain free and `target` occupies 5.3 GiB
(approximately 20 GiB free and 3.2 GiB target at the start). SHA-256 comparison
confirmed all seven work-card planning artifacts unchanged from the start of this
task; all 23 previously completed task lines are preserved verbatim. Progress is
24/37, with 5.6 next. Work remains on `main`; no commit was requested or created.
