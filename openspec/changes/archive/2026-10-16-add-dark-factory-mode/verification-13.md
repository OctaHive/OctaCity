# Verification: Delivery and optional Source reporting (13.1–13.4)

## Scope and architecture

Delivery has its own provider-neutral v1 process contract and capability metadata.
The host reuses `octacity-server-adapter-host` for verified executables, cleared
environment, bounded framing, timeouts and process-tree cleanup. Read-only VCS
and repository Job credentials cannot satisfy `delivery_write`.

There is one GitHub provider package, `octacity-github`. Octocrab 0.54.3 is confined
to it by an executable architecture-policy test. The SDK uses the existing
reqwest transport without automatic retries, redirects, tracing or JWT features;
responses are bounded before decoding. Other GitHub capabilities extend this
same plugin and transport with independently scoped protocols/grants. Concrete
automatic intake/reporting remain the explicitly separate task 16.9.

The adapter verifies retained canonical capture bytes, exact candidate/direct
parent/original ancestry in a private bare quarantine. Empty push leases cannot
overwrite a remote head. PR creation/metadata repair uses one stable delivery
marker, followed by fresh observation. An ambiguous accepted write is observed
before replay; neither silent rebase nor merge is available. Git/API redirects
cannot change the selected destination. GitHub repository-name case normalization
does not change branch/revision identity checks.

The composition bridge resolves an immutable target policy, verifies exact
Project/Repository and plugin/network/secret ceilings before exposing handles,
and pins the provider configuration digest. It binds the frozen input/provenance
proof and respects original start/deadline plus the current observer window.
Active cancellation addresses the original operation's token before acknowledging
its stop. Common append-only results checkpoint branch, verified remote head,
review identity/state and normalized receipt digest. Existing independently
verified promotion/deployment records remain under the same configurable owner.

Trusted action results can carry bounded deferred effects. The owner accepts the
effect atomically with the common result, binding the event to its exact record
digest; other node primitives cannot originate these effects. Status projection
uses an arbitrary configured value and optional published retry policy. An
independent worker selects only its effect kind, resolves the exact installed
projector and immutable accepted input, and uses the original operation ID.
Reporter failure/restart/success changes outbox rows only, never authoritative
Run, current-state or Flow data. Runtime installation supplies independent
dispatchers without business-stage or provider-name dispatch in the graph owner.

No commit was requested or created. Documentation/configuration formats are
development-only: no released Factory data migration or legacy fallback is added.

## TDD evidence

Rust commands below ran with `CARGO_PROFILE_DEV_DEBUG=0` and
`CARGO_PROFILE_TEST_DEBUG=0`. Each meaningful Red preceded implementation; fixture
permissions/type mistakes and environment failures were corrected separately and
were not counted as Red.

| Boundary / behavior | Command | Observed Red → Green |
| --- | --- | --- |
| Write credential class | `cargo test -p octacity-delivery-protocol --offline` | missing decoder API → canonical contract accepted; VCS/Job/reporter classes rejected |
| Exact correlated observation | same protocol command | missing response API → substituted scope/head/decision rejected |
| Publication requires observation | same protocol command | missing manifest API → unsafe write capability rejected |
| SDK stays in one provider | `python3 -m unittest tools.tests.test_check_architecture.ArchitecturePolicyTests.test_github_sdk_is_confined_to_the_single_provider_plugin` | isolated provider denied → allowed there; other layers remain denied |
| Stored grant cannot be reclassified | `cargo test -p octacity-github --test credentials --offline` | missing provider API → stored read credential rejected, secret-free result |
| Existing exact PR observation | `cargo test -p octacity-github observes_the_existing --offline` | Unsupported → exact branch/review observed without writes |
| Immutable provider mapping | `cargo test -p octacity-github changed_provider_configuration --offline` | missing frozen-profile API → changed mapping rejected before network |
| Exact Git capture publication | `cargo test -p octacity-github publishes_only_the_captured --offline` | Unsupported → real bundle imported/pushed; exact PR checkpoint observed |
| Correlated process cancellation | `cargo test -p octacity-github --test process --offline` | empty output → actual binary emits one correlated cancellation result |
| Profile-pinned process host | `cargo test -p octacity-server-delivery --offline` | missing host API → verified/correlated execution and pre-spawn mismatch rejection |
| Metadata repair under one PR ID | `cargo test -p octacity-github repairs_review_metadata --offline` | required PATCH absent → same PR repaired, then observed |
| Immutable target policy | `cargo test -p octacity-server-application publication_rejects_an_unversioned --offline` | mutable alias published → rejected before effects |
| Frozen proof/time/window | `cargo test -p octacity-server-application exact_candidate_publication_recovers --offline` | missing requested scope fields → original input/start/observer window retained on replay |
| External facts in common journal | same application command | missing typed facts API → exact branch/head/review retained with result |
| Trusted host ceiling | `cargo test -p octacity-server delivery_cannot_resolve --offline` | missing bridge API → denied ceiling cannot stage or spawn |
| Exact hosted publication | `cargo test -p octacity-server exact_delivery_is_hosted --offline` | Unavailable → qualified process result normalized into exact publication |
| Stop original active process | `cargo test -p octacity-server cancelling_delivery_stops --offline` | second process falsely acknowledged stop → original host cancelled before acknowledgment |
| Outbox kind selection, memory | `cargo test -p octacity-server-store outbox_workers_claim_only --offline` | missing filtered-claim API → unrelated events left pending |
| Outbox kind selection, PostgreSQL | `cargo test -p octacity-server-store-postgres --test factory_discovery outbox_kind_selection --offline -- --ignored` | reporter consumed admission event → exact-kind selection isolated |
| Atomic optional report intent | `cargo test -p octacity-server-application optional_status_reporting_is_queued --offline` | unsupported published outbox parameters → node accepted with one pending event and no source call |
| Independent reporting/recovery | same queued-report command | missing dispatcher API → bounded retry under same ID; authoritative data unchanged |
| Supervised independent runtime | `cargo test -p octacity-server configured_factory_outbox_dispatchers --offline` | missing runtime dispatcher entry → separate supervised worker |
| Private quarantine | capture-publication command | trusted Git guard rejected public directory → private 0700 quarantine |
| GitHub repository case | existing-observation command | canonical case treated as conflict → same repository observed |
| Selected Git destination | `cargo test -p octacity-github git_transport_cannot_follow --offline` | redirect followed → no follow-up request |

Per-slice logs use `/tmp/octacity-13-*-red.log` and matching Green files.
Additional regression assertions cover unknown-response replay without another
POST, altered capture bytes, conflicting remote heads, reporting retry exhaustion,
source failure without rollback and owner replay without duplicate queue events.

## Regression and qualification

- Protocol/provider/shared-host suites passed; the final provider has seven unit
  scenarios plus stored-credential and actual-process integration checks.
- `cargo test -p octacity-server-factory -p octacity-server-store --offline`:
  256 Factory and 69 store unit tests passed, plus store contract integrations.
- Full application/runtime regression: 219 application unit tests and existing
  configurable nested-flow/quality/human/promotion journeys passed. Final targeted
  action rerun has 14 passing scenarios; final runtime unit rerun has 27 passes.
- Disposable PostgreSQL `factory_discovery -- --ignored`: all 14 contracts passed,
  including kind isolation, atomic rollback, restart, old-writer fencing and
  arbitrary configured-node persistence. The task-owned service is removed after
  verification.
- `cargo clippy` for all changed packages, `--all-targets --all-features -- -D warnings`:
  passed. Findings were fixed without lint suppression: compact observation enum,
  named projection material, scoped test lock and simplified cleanup.
- Architecture check passed: 59 packages / 218 dependency edges. Python architecture
  and graph-semantics checks passed (28 tests). `cargo fmt --all -- --check`, strict
  OpenSpec validation and `git diff --check` passed.
- `cargo metadata --manifest-path fuzz/Cargo.toml --format-version 1 --locked --offline`
  passed; the separate fuzz lock remains consistent.

The first broad run exposed one outdated readiness fixture lacking the newly
required external facts; it was updated to the new contract and all Factory
tests passed. One one-second host fixture timed out during concurrent compilation;
an isolated rerun passed. Neither event was used as feature Red evidence.

These are local process/network/Git and durable-owner contracts. No live GitHub
account was mutated. Live HTTPS forge qualification and concrete deployment
materialization are part of the separately planned runtime qualification work.
Disk space was monitored; it remained above the user's 5 GiB cleanup threshold.
Documentation-only changes required link/format/spec validation rather than TDD.

## Review corrections

Six review findings were corrected at existing public boundaries with Red/Green:

| Behavior | Command (with the Rust profile environment above) | Observed Red → Green |
| --- | --- | --- |
| Reusable target policy | `cargo test -p octacity-server one_installed_publication_policy --lib --offline` | fixed branch failed exact receipt validation → two operations use distinct branches; original replay is stable |
| Schema compatibility before effects | `cargo test -p octacity-server-application publication_rejects_a_probe_only --offline` | external publication happened before schema rejection → incompatible enum rejected before any write |
| General shape containment | `cargo test -p octacity-server-factory configured_schema_compatibility --offline` | missing public containment API → structural ranges/fields/enums prove compatibility |
| Git transport uncertainty | `cargo test -p octacity-github a_lost_git_push_reply --offline` | accepted push with lost acknowledgment classified Conflict → Transient; replay observes and completes the same branch/PR |
| Actual provider origins | `cargo test -p octacity-github actual_provider_origins --offline` | missing frozen network ceiling → either undeclared API or Git host rejects before network activity |
| Strict network contract | `cargo test -p octacity-delivery-protocol network_ceilings --offline` | wildcard ceiling accepted → only bounded, unique exact canonical hosts admitted |
| Existing PR metadata | `cargo test -p octacity-github observation_requires_repair --offline` | incorrect metadata reported complete → typed repair-required observation retains the same PR identity |
| Host routes repair | `cargo test -p octacity-server hosted_observation_routes --lib --offline` | valid repair observation became InvalidInput → original publication path remains recoverable |
| Observed external merge | `cargo test -p octacity-github externally_merged_review --offline` | advanced base rejected before reading the merged PR → exact owned merged review observed with original accepted base and current review base |

Logs: `/tmp/octacity-13-fix-*-red.log` and corresponding Green files. Required-API
compile failures establish Red only where explicitly listed. Additional protocol
checks ensure repair cannot qualify Publish, and closed reviews cannot request
repair. The provider uses the same shared plugin/SDK/host; no business Flow or
node dispatch was introduced. Configuration/protocol updates remain unreleased.
No commit was made.

Final correction regression (all passed):

- `cargo test -p octacity-server-factory -p octacity-server-application -p octacity-server --lib --offline`: 257 Factory, 221 application and 29 runtime tests (507 total).
- `cargo test -p octacity-github -p octacity-delivery-protocol -p octacity-server-delivery --offline`: 11 provider unit, 2 provider integration, 4 wire contract and 1 host tests (18 total).
- `cargo clippy` for the six changed Rust packages, `--all-targets --all-features --offline -- -D warnings`: passed.
- Architecture graph: 59 packages / 218 edges; Python architecture-policy suite: 25 passed.
- `cargo fmt --all --check`, `git diff --check`, strict OpenSpec validation and locked/offline fuzz metadata resolution: passed.

Logs: `/tmp/octacity-13-fix-owner-regression.log`,
`/tmp/octacity-13-fix-adapters-regression.log`,
`/tmp/octacity-13-fix-clippy.log` and `/tmp/octacity-13-fix-python.log`.
No production code changed after these checks. Free space remained approximately
11 GiB; no target cleanup was required.
