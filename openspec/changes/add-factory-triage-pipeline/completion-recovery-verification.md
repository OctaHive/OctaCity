# Task 3.3: fenced completion preparation and recovery

## Scope and existing owners

Recovery continues to use the existing Source preparation, materialization,
scan claim, ordinary admission and private-input cleanup owners. The tests
exercise public worker/admission interfaces with controlled external byte and
VCS ports, the verified Source process fixture, and both memory and real
PostgreSQL stores. Worker teardown/recreation is controlled at awaited I/O
barriers; the publication matrix also reconstructs the PostgreSQL store handle.
This is semantic recovery qualification, not an OS/process-kill or live Agent
qualification claim.

Admission races required production changes. Revision resolution can yield
after initial authorization. Before fenced commit, prepared Source admission
now rechecks the current installed Source grant, exact native/input proof and
selected owner, followed by current Project/document/dependency visibility,
including both generated task and acceptance artifacts.
A previously authorized Source copy or a completed VCS read confers no continuing
authority. Commit time and existing scan fences remain authoritative.

## TDD and fault cases

All Rust commands use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`, `--locked`, and two test threads for the
database/runtime suites. Baseline application Source tests: 59 passed. The
original two runtime completion tests also passed on memory/PostgreSQL before
their fixture setup was shared with the new recovery cases.

| Public interface / test command filter | Observation |
| --- | --- |
| `cargo test --locked -p octacity-server-application --lib prepared_completion_cannot_be_admitted -j2` | **Red:** extending the existing hidden-document test to revoke only the original document during VCS resolution still admitted Work. **Green:** both initial denial and in-flight revocation reject admission; no association is retained. |
| `cargo test --locked -p octacity-server-application --lib completion_admission_rechecks_source_authority -j2` | **Red:** revoking the installed Source grant during revision resolution still admitted completion inputs. **Green:** current Source reauthorization prevents admission after the real asynchronous boundary. |
| `cargo test --locked -p octacity-server-application --lib completion_admission_rechecks_each_generated_input_after_revision_resolution -j2` | **Review Red:** revoking the generated task artifact during VCS resolution still admitted Work. **Green:** revoking either generated task or acceptance artifact rejects admission, without an admission receipt or idempotency mutation. |
| `cargo test --locked -p octacity-server --lib completion_publication_crashes -j2 -- --include-ignored --test-threads=2` | Four external-write barriers: before/after task publication and before/after completion publication. Recovery retains the exact original artifact identities, criteria, source revision policy, snapshot and expiry after current settings change and the provider page becomes empty. Memory and PostgreSQL scenarios pass. |
| `cargo test --locked -p octacity-server --lib published_completion_recovers_before_admission -j2 -- --include-ignored --test-threads=2` | Stop during VCS resolution after both private documents and the complete intent are retained. A new worker admits once without republishing, keeps original criteria and replays rediscovery without another VCS resolution. Both stores pass. |
| `cargo test --locked -p octacity-server --lib completion_takeover_and_retention_races -j2 -- --include-ignored --test-threads=2` | A second live claim is denied. After lease expiry, takeover admits one original Work; a late old writer cannot publish new authoritative metadata. Cleanup racing a late write removes private policy/snapshot, retains expiry, reconciles late bytes and prevents resurrection. Both stores pass. |
| `cargo test --locked -p octacity-server-application --lib completion_admission_acknowledgement_loss -j2` | The caller discards a successful public admission response before acknowledging progress. A fresh handler recovers original IDs with an unavailable VCS resolver; two replays retain the original Work/Run, criteria and one admission mutation. No new backend or response-loss hook is installed. |

The fault matrix characterizes existing recovery behavior; it did not require a
triage-specific recovery implementation. The authorization Reds above drove
production changes. These authorization tests use finite
external-grant/VCS peers at the existing application ports. The worker tests use
the real installed, digest-verified Source process fixture.

The PostgreSQL publication matrix explicitly imports the same title-only native
ticket through `IssueSourceHandlers`, then compares its full canonical Issue
revision before/after each crash/recovery. Description remains empty and
acceptance remains absent. The criteria never become Issue-authored requirements.
The existing durable Source store contract separately verifies atomic admission,
receipt reconstruction and replay under live fences; no TCP-level lost-COMMIT
response is claimed by the discarded-response scenario.

An initial PostgreSQL matrix fixture reused a database across several root
Projects with the same fixture name and hit its real unique-name constraint.
Each case now owns a disposable database; this environment/setup failure is not
TDD Red and required no production schema or behavior change.

## Section 3 review

The section 3 review inspected all tracked and untracked changes against
`b1d745f4126ce0b07b19888515f535c3c16e4580`. A subsequent review identified two
gaps: generated input visibility was not rechecked after VCS resolution, and
negative Project-document tests reused a frozen preparation policy. The former
now uses the existing intent authorization gate immediately before fenced
admission; no new policy owner or abstraction is introduced.

For the latter, the existing exact-Project-document test first passed as a
baseline. Requiring the expected public failure classification exposed a Red:
`changed_digest` returned `Conflict` from the frozen policy rather than `Invalid`
from document validation. Each short-retention, foreign-Project, changed-digest,
missing-document and valid case now owns a fresh preparation. The test retains
both immediate and delayed publication scenarios and exact provenance checks.
Invalid references must return `Invalid`; a missing document must return
`CapabilityUnavailable`. Shared Project/byte checks and bounds remain with their
existing owners.

Document-test Red/Green command:
`cargo test --locked -p octacity-server-application --lib
source_preparation_binds_an_exact_project_completion_document -j2`.
The tightened assertion failed before the fixture isolation and passed afterward
for all ten document/publication-time scenarios.

The reader deliberately requires coverage through the current fenced read
window. Near-expiry documents may therefore fail safely rather than acquiring
renewed retention. The review treated relaxing that conservative requirement as
an optional resilience improvement, not a violation or permission to extend
retention. The production authorizer retains its immutable management context
across the repeated grant check, as required by its existing contract.

## Final checks

| Command | Result |
| --- | --- |
| `cargo test --locked -p octacity-server-application --lib installed_sources -j2 -- --test-threads=2` | 62 passed after the review corrections, no failures/ignored tests; includes all three admission-race regressions, isolated document failures, acknowledgement-loss recovery and all original Source mapping/preparation/admission behavior. |
| `cargo test --locked -p octacity-server --lib runtime::work_source_tests -j2 -- --include-ignored --test-threads=2` | 136 passed again after the review corrections, no failures/ignored tests, against the disposable PostgreSQL service where required. |
| `cargo test --locked -p octacity-server --lib runtime::work_source_tests::completion -j2 -- --include-ignored --test-threads=2` | 8 passed on the final fixture code, including memory/PostgreSQL recovery and unchanged canonical Issue acceptance. |
| `cargo test --locked -p octacity-server-application --test factory_artifact_reader -j2` | 5 passed. |
| `cargo test --locked -p octacity-server-store-postgres --test factory_discovery source_publication_is_atomic_owned -j2 -- --ignored --test-threads=2` | 1 passed; shared memory/durable store contract including fenced admission, replay and private cleanup. |
| `cargo clippy --locked -p octacity-server --lib --tests -j2 -- -D warnings` | Passed after making the test byte-lock lifetime lexical before subsequent awaits; no lint was suppressed. |
| `cargo clippy --locked -p octacity-server-application -p octacity-server-store -p octacity-server-store-postgres --lib -j2 -- -D warnings` | Passed. |
| `cargo clippy --locked -p octacity-server-application -p octacity-server --lib -j2 -- -D warnings` | Passed after the review corrections. |
| `cargo clippy --locked -p octacity-server-application --lib --tests -j2` | Passed after simplifying test expressions; only the pre-existing unused `pending_factory_profile_fixture` warning remains. No lint was suppressed. |
| `python3 tools/check_architecture.py` | Passed: 63 packages and 261 dependency edges. |
| `cargo fmt --all -- --check`, `openspec validate add-factory-triage-pipeline --strict`, `git diff --check` | Passed. |

Database commands set `OCTACITY_POSTGRES_URL` to this task's loopback service.
Local command output uses `/tmp/octacity-triage-3-3-*.log`. The complete 781-test
application-library regression recorded under task 3.2 remains prior evidence;
this task reruns the affected Source suite and public reader tests rather than
claiming a new full-library result.

Review-correction Red/Green and regression logs use
`/tmp/octacity-triage-review-fixes-*.log`. Application test Clippy also identified
three test-expression simplifications (`expect_err` and two redundant borrows),
removed without changing behavior after the 62-test Source baseline passed.

## Environment and limits

PostgreSQL 18.6 runs in a task-owned container on an allocated loopback port with
public test credentials and disposable databases. No personal stand data, live
GitHub writes, paid model requests or Agent/VM execution is involved. Packaged
two-stage execution remains later work; the previously deferred native GitHub
timeout is unchanged. Available disk space remained about 15–16 GiB, above the
10 GB cleanup threshold. The task-owned PostgreSQL container and anonymous volume
were removed after verification. No commit was created.
