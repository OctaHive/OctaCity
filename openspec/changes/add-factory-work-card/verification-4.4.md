# Task 4.4: reviewed field ownership and separate native status mappings

## Scope

The optional `IssueSourceHandlers` owner publishes a reviewed mapping through
`PublishIssueSourceMappingCommand` and `IssueSourceMappingStore`. Definitions
have their own immutable per-link versions, separate from Issue revisions,
Project workflows and Flow action selection. Publication installs derived
`IssueFieldPolicy` atomically without editing task content or starting execution.
Manual Issue handlers retain their existing ports and need no provider.

Title and description support source-owned pull. Status supports source-owned
pull or OctaCity-owned push through explicitly supported installed effects.
Other fields remain unsynchronized unless their direction is actually supported;
unsupported writes are rejected. Push activation needs current connection
visibility/use authority and the existing independent TaskUpdate grant. A
TaskRead grant cannot authorize it. No credential values enter this contract.

Status predicates use stable internal status IDs and the exact pinned workflow.
The shared Issue transition policy owns permitted direct edges, origins,
ownership, evidence guards and applicability invalidation. Source history
retains exact mapping/native/observation references. Text and status from one
observation share one ordinary immutable revision and atomic history publisher.
Echoes create no revision, status event or execution. Local status ownership
prevents native divergence from becoming an incoming move.

Explicit and paged consumers share one observation scope/lifetime/digest check.
Mapping publication invalidates old retained-link read owners while preserving
native freshness and explicit pull enablement. Mapping replacement cannot move
its observation epoch backwards; a read must begin strictly after publication
to acquire new mapping authority. Older versions and original replay remain
available after later edits and replacements.

Existing `IssuePushIntent` retains the exact approved mapping for local changes.
Reporter dispatch, originating execution/producer qualification and outbound
recovery composition remain tasks 5.1–5.3. REST/Console mapping authoring remains
task 4.5; no new provider adapter, poller or execution owner is introduced here.

## TDD evidence

Builds use `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2` and the existing target directory.
PostgreSQL commands use `OCTACITY_POSTGRES_URL` pointing to a dedicated disposable
PostgreSQL 18.6 container with 512 MiB memory and a 384 MiB temporary data directory.

| Command/filter | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test --offline -p octacity-server-application --lib issue_source_mapping_publication` | Mapping publication API absent | Reviewed source-owned description rejects local replacement, local priority remains editable, and replay preserves the original mapping after a later Issue edit |
| `cargo test --offline -p octacity-server-application --lib issue_source_status_pull_uses` | Status observation result/provenance API absent | One declared direct Source edge commits; a later echo preserves the same revision/history |
| `cargo test --offline -p octacity-server-application --lib issue_source_status_mapping_cannot` | A cached read predating mapping publication applied a status move | Old reads remain NotFresh and a new independent read applies the reviewed mapping |
| `cargo test --offline -p octacity-server-store-postgres --test issues source_mapping_publication_and_transition -- --ignored` | PostgreSQL mapping capability absent | Mapping and combined text/status refresh survive restart and exact replay as one original revision |
| `cargo test --offline -p octacity-server-application --lib issue_source_mapping_push_requires` | OctaCity-owned push contract rejected as unavailable | Installed finite effects produce the existing exact push intent; native divergence stays data and revoked TaskUpdate blocks a new publication |
| `cargo test --offline -p octacity-server-application --lib issue_source_mapping_rejects_oversized` | A definition over 64 KiB published ownership | The byte ceiling rejects it before mapping/policy/replay/audit publication |
| `cargo test --offline -p octacity-server-application --lib issue_source_mapping_issue_administration` | Issue administration bypassed denied connection use | Independent connection visibility/use checks reject push activation |
| `cargo test --offline -p octacity-server-application --lib issue_source_mapping_rejects_status_facts` | Substituted native status acquired authority from an unrelated snapshot digest | Shared explicit/paged validation rejects it without task or mutation evidence changes |
| `cargo test --offline -p octacity-server-store-postgres --test issues source_mapping_replacement_retains -- --ignored` | An old owner checkpointed after mapping replacement | The existing generation fence rejects it, a new read starts with uncovered progress, and historical mapping/replay stays immutable |
| `cargo test --offline -p octacity-server-application --lib issue_tests::source::mapping` | Same-millisecond reads applied new mapping authority and backdated replacement reopened an old epoch | Both require explicit later ordering; the eight mapping scenarios then passed |
| `cargo test --offline -p octacity-server-application --lib issue_source_mapping_a_different_refresh_profile` | A mapping pinned to another installed read profile exhausted a shared discovery page | A durable typed skip preserves freshness; the remaining item imports after restart without another provider read, the selected profile can refresh, and manual creation remains available |
| `cargo test --offline -p octacity-server-application --lib issue_source_mapping_supports_advertised_opaque` | A supported native state containing spaces failed Factory-key parsing | The existing generic predicate matcher accepts bounded opaque native text for status mappings while intake retains its typed Factory key; all 18 Source/Factory tests passed before and after extraction |

The completed mapping module has 13 public application tests. Additional cases
exercise unmapped and ambiguous targets, missing direct edges and execution
evidence, source-owned manual denial, unsupported fields/values/workflow pins,
revoked read authority, foreign scope and all five memory publication failure
points. Rejected publication leaves mapping, ownership, generation, replay,
audit and outbox unchanged; an exact retry succeeds.

## Qualification

All commands below use the build controls and disposable database described
above. Setup/import errors were corrected before implementation; they are not
counted as behavioral Red. Documentation-only changes have no executable
behavior and are checked by diff/OpenSpec validation.

| Command | Result |
| --- | --- |
| `cargo test --offline -p octacity-server-application --lib` | 584 passed, including all 13 new mapping scenarios, existing manual/pipeline/Source contracts and full Factory journeys |
| `cargo test --offline -p octacity-server-store --all-features --lib` | 81 passed |
| `cargo test --offline -p octacity-server-factory --lib` | 296 passed, retaining existing Source predicates and Factory behavior |
| `cargo test --offline -p octacity-server-store-postgres --test issues --test issue_execution --test issue_workflows --test factory_discovery --test migrations -- --ignored --test-threads=2` | 100 passed: 40 Issue, 19 execution, 8 workflow, 27 Factory/Source and 6 migration scenarios |
| `cargo test --offline -p octacity-server --lib work_source` | 73 passed; 9 PostgreSQL scenarios qualified separately |
| `cargo test --offline -p octacity-server --lib work_source -- --ignored --test-threads=1` | All 9 native PostgreSQL Source/Reporter recovery, ordering and grant scenarios passed |
| `cargo test --offline -p octacity-server-application --lib issue_` | 102 passed after the final equivalent transition-condition cleanup; all 81 store tests also passed again |
| `cargo clippy --offline -p octacity-server -p octacity-server-application -p octacity-server-store-postgres --all-features --all-targets -- -D warnings` | Passed after combining the equivalent rejection conditions, with no lint suppression |
| `cargo fmt --all -- --check` | Passed |
| `git diff --check` | Passed |
| `python3 tools/check_architecture.py` | Passed: 63 packages, 256 dependency edges |
| `openspec validate add-factory-work-card --strict` | Passed |

PostgreSQL failure injection interrupts the existing outbox publication for
both mapping and mapped text/status refresh. Issue head, field policy, read
generation, history and replay remain unchanged; the exact original operation
then succeeds. Competing first-publication reviews retain exactly one mapping
version. Independent store instances recover the same mapping/history and
original receipts; historical versions and read-owner fencing remain intact.

Clippy identified two adjacent rejection branches with the same result. After
the above behavioral baselines passed, their predicates were combined without
changing policy; the 81 store and 102 Issue tests remained green. No behavioral
test was added for this structural cleanup.

Disk space was checked throughout builds (observed 8.1–9.7 GiB free). The
existing target directory was reused with incremental/debug data disabled;
no second build tree or downloaded database image was needed. The disposable
PostgreSQL container and its temporary data were removed after database
qualification. Existing native tests exercise installed/fake boundaries; this
slice adds no live-provider qualification claim.
