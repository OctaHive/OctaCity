# Task 6.1 — Hermetic composed integration pilot

## Public boundary and location

`server/app/tests/factory_work_source_pilot.rs` is an ordinary integration test
target. It imports the server's public composition API, application commands and
queries, and authoritative store contracts. It does not import a private runtime
module, expose production internals for testing, or implement another admission
or reporting path. The standard workspace CI test command discovers it automatically.

Existing Source fixtures and configured action/Reporter fixtures are shared by
the component tests and the pilot through `server/app/tests/support/work_source.rs`
and `work_reporter.rs`. They were moved rather than copied. The pilot's management
setup lives in `tests/support/work_source_pilot.rs`. The only new crate edge is a
dev-dependency on the already existing `octacity-server-secrets` port; no new
package, production capability, protocol, schema or storage migration was added.

## Scope and independently specified outcomes

The composed pilot uses isolated temporary installations of the content-verified
Source v1 / ordered Reporter v2 process peer, an authoritative in-memory store,
an injected application clock and ordinary Factory node/action/outbox execution.
Private credential publication is an explicit hermetic peer at its existing port.
It performs no network access or actual credential lookup. The two tests cover
twelve independently isolated variants:

| Scenario | Observed outcome |
| --- | --- |
| Named reusable connection created through authorized management | Metadata alone remains disabled and grants no Project access; exact command replay returns the same version. |
| Adoption interrupted at private activation | Connection remains inactive; a newly constructed administration handler resumes the same request and completes adoption without a token value. |
| Independent task-read/task-update grants | Both assessment and downstream inputs select the same explicitly granted connection, with distinct task and code resources. |
| Reviewed input publication and replay | Two stable input identities select different exact Factory targets; repeated publication retains its original immutable receipt. |
| Fixture assessment action and arbitrary effects | The ordinary accepted typed `set_status` action queues the existing outbox. Reporter projects configured additions/removals; no model call or prose-based action inference occurs. |
| Downstream intake | No downstream Run exists before the effect. A later Source pass admits exactly one distinct downstream Work/Run from the changed native labels. |
| Response lost after provider apply | A reconstructed Reporter/outbox observes the applied result and completes the original operation; the peer receives exactly one apply. |
| Credential rotation during reporting recovery | Reviewed rotation/reactivation preserves the connection/input/Work identities, and original rotation replay returns the same receipt. |
| Optional fake task consumer | A passive test-owned sink receives observations sharing both inputs' native correlation, then fresh body/state/label provenance after admission and an execution-selector mismatch. Neither frozen Work/Run nor input Artifact bytes change. |
| Consumer absent | Constructing optional observation support makes no Source request; no subscription, card storage, implicit read or extra Run is required. |
| Provider outage and access revoked | A transient Source process failure defers both inputs without creating Work. Subsequent revocation blocks execution and observation before provider dispatch. Manual Artifact admission still succeeds during the outage with no Source call or association. |
| Configured rejection and ambiguity | The same logical status can project the alternative configured label. Rejected tasks do not enter downstream; overlapping members of an explicit exclusive group retain two-match diagnostics and admit neither target, including after worker reconstruction. |

The successful path runs with and without a consumer for each peer:

| Installed peer identity | Editable label vocabulary |
| --- | --- |
| `github` | `approved` / `rejected` |
| `gitlab` | `ready-for-review` / `archived` |
| `jira` | `editorial-ready` / `needs-context` |
| `custom-tracker` | `green` / `red` |

Rejection/exclusive-group qualification also runs for all four peers with
`editorial-ready` / `needs-context`. Provider identity and labels select no
production branch. The independently configured code Repository uses a different
locator/identity from the native ticket container. Ordinary accepted-action and
manual-admission permissions remain authoritative.

## TDD evidence

Before changing the process peer, the new composed test drove the entire public
chain and failed because the peer's Reporter effects were not visible to its
Source reads:

```text
projected labels must select the distinct downstream input
left: 1
right: 2
```

The test was initially placed beside the runtime component fixtures and run with:

```sh
cargo test --locked -p octacity-server --lib composed_pilot
```

Red: one behavioral failure after successful management, admission, action and
Reporter delivery. Green: the same test passed after adding a fourteen-line
opt-in `source_effects` bridge to the existing hermetic process peer. The bridge
applies the supplied literal effects to the peer's native candidate state before
its existing lost-response injection. It has no routing, admission or card logic.
Setup/compile failures during test construction were not counted as Red.

The pilot was then moved to its integration-test target. Existing component tests
passed before and after moving their shared fixtures (72 existing tests; the
initial baseline also included the one newly added pilot). Additional recovery,
observation, rejection and ambiguity cases characterize already implemented
behavior; they required no production fixes, longer deadlines or test serialization.
The final integration target passes both tests and all twelve variants:

```sh
cargo test --locked -p octacity-server --test factory_work_source_pilot
```

## Regression commands

All Cargo commands used the existing target directory with:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

```sh
cargo test --locked -p octacity-server --lib work_source
cargo test --locked -p octacity-server --test factory_work_source_pilot
cargo test --locked -p octacity-github -p octacity-work-source-protocol -p octacity-server-work-source
cargo test --locked -p octacity-server-application --lib factory_admission_tests
cargo clippy --locked -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
python3 tools/security_matrix.py validate
python3 tools/verify_local_stand_context.py
openspec validate add-github-work-source --strict
openspec validate add-factory-work-card --strict
git diff --check
```

Observed results: 2 composed pilot tests (12 variants), 72 Source/Reporter
component tests, 142 native GitHub/protocol/host tests and 84 application admission
tests passed: **300 tests** in total. Nine existing PostgreSQL tests were ignored
by the normal opt-in contract. Clippy, formatting, architecture (63 packages,
254 edges), security-matrix validation, local-stand context, both OpenSpec changes
and diff checks passed. UI code and generated client contracts were unchanged.

## Qualification limits

This is a hermetic composed process-boundary qualification, not a live provider
account qualification. GitLab/Jira/custom identities are common-contract peers;
no native GitLab/Jira adapter is claimed. The separately run native GitHub tests
exercise controlled HTTPS listing/reporting, protected file publication/recovery,
partial/uncertain effects and existing Delivery contracts. The composed private
publisher does not qualify actual token bytes or a production filesystem mount.

Worker/handler reconstruction retains the authoritative in-memory store and
isolated external peer state; it does not claim disk/database-process durability.
The nine existing opt-in PostgreSQL tests remain opt-in. Source checkpoint and
retained-preparation recovery, independent writer revocation and ordering continue
to be covered by the unchanged component/store contracts.

No real Issue, token, repository, user's stand or paid model was used. Persistent
internal cards, import/refresh scheduling, association uniqueness, explicit card
execution and manual card creation remain owned by `add-factory-work-card`.
The passive fake consumer adds none of these capabilities. A real-card cross-change
pilot follows when implemented; cards are not a prerequisite for this qualification.
Live GitHub qualification remains task 6.2; task 6.4 owns the final wider release
regression gate.

Free space was checked before builds and during verification: approximately
13 GiB initially and 12 GiB after the builds, using the existing target cache.
No user files or build cache were deleted and no service/container was started.
