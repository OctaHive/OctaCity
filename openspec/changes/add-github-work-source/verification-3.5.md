# Source worker composition — task 3.5

Task 3.5 completed on 2026-10-17. OpenSpec progress: 14/29 tasks. No live provider or paid model calls.

## TDD evidence captured during implementation

Commands below use a task-owned Cargo target directory, debug symbols and incremental compilation disabled, `--locked --jobs 2`.

| Public behavioral boundary | Observed Red | Observed Green |
| --- | --- | --- |
| Work Source protocol group selection | Closed decoder rejected additional selector references | 10 protocol contract tests passed; missing/empty/duplicate/foreign matching proofs fail closed |
| GitHub group observation | Group scan returned protocol fault instead of candidates matching either selector | One page yields A-only, B-only and A+B match proofs from one observation |
| GitHub rate limit | `retry_after_millis` was absent instead of 2000 | Retry-After preserved; missing hint waits 60s; excessive delay requires repair |
| Source store discovery | Missing public discovery operation | Current enabled inputs/groups only; exclusive members never independently scheduled |
| Source store scheduling | Missing public scheduling operations/state | Retry lower bound and exhaustion survive takeover; exact replacement resets schedule |
| Source store group selection | Missing public fenced selection operation | Zero/one/several matches classified; group replacement revokes old owner |
| Runtime absent/pre-cancelled input | Missing public source worker API | No provider inventory/grant required and no provider activity |
| Runtime ordinary admission | Claimed/completed counts were zero | GitHub/GitLab/Jira/custom process fixtures each produce one ordinary Run and two immutable inputs |
| Runtime restart/backoff | Provider failure escaped the pass instead of durable deferral | Provider 2000ms lower bound survives worker restart and exceeds local 40ms backoff cap |
| Runtime installation validation | Zero operation timeout accepted | Invalid deadlines and duplicate/foreign/mismatched profiles rejected before activity |
| Runtime exclusive group | Group completed count zero | Ambiguous observation produces no admission; subsequent unique match uses exact selected target |
| Runtime batch/concurrency | Peak process concurrency 1 instead of 2 | Peak 2, bounded first batch of 2, next pass visits remaining input |
| Runtime shutdown | Shutdown consumed attempt 1 instead of retaining 0 | Verified process cancellation drains promptly; no admission; restart reuses logical scan identity |

Focused runtime command: `cargo test --locked --jobs 2 -p octacity-server --lib <test-name> -- --nocapture`.
Provider tests: `cargo test --locked --jobs 2 -p octacity-github --test issue_listing <test-name> -- --nocapture`.
Protocol tests: `cargo test --locked --jobs 2 -p octacity-work-source-protocol --test contract`.
Store contracts are exercised through application source polling tests and PostgreSQL contracts.

GitHub retry interpretation follows the official [REST best practices](https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api).

Further vertical slices:

- Complete pass deadline: `a_source_pass_deadline_bounds_revision_resolution_and_leaves_original_intent_recoverable` first timed out at the test's 2s watchdog while VCS was pending; Green bounds the entire slice inside the durable lease, leaving original preparation recoverable.
- Original intent before discovery: extending that driver with input replacement and an unavailable provider first produced zero Runs; Green settles retained intent before fresh provider discovery, without republishing artifacts.
- Runtime opt-in: `native_profile_installation_exposes_only_its_project_catalog_without_dependency_io` initially failed for missing Source opt-in APIs. Green preserves default absence and installs explicitly using existing PostgreSQL/S3/VCS ports without provider I/O.
- Runtime lifecycle: `installed_source_polling_is_supervised_and_stops_with_the_runtime` initially failed for the missing supervisor binding; Green observes an ordinary Run through the real fixture process, then drains cancellation.
- Retention lifecycle: `runtime_retention_clears_orphaned_source_snapshots_after_the_input_is_disabled` initially failed for the missing cleanup worker binding; Green clears a retained private snapshot while Source polling is absent and the input disabled, with no provider activity.
- Primary GitHub rate limit: `primary_rate_limit_reset_is_a_lower_bound_even_with_a_shorter_retry_after` observed 1000ms instead of the ~120000ms reset lower bound; Green respects the later bound.
- Current Source negotiation on recovery: extending the retained-intent driver with a newly incompatible installed Source protocol first admitted a Run; the corrected authorizer must negotiate before cached data can be admitted.

Characterization of the composed policies: unavailable providers persist safe diagnostics and exhaust the finite retry budget; disabled inputs need no process/grant; disabling a current input during a provider call prevents preparation/admission; a stalled provider is classified as timeout rather than an empty page. These behaviors were already driven by the preceding slices and their existing store/host contracts.

- Retained-page bound: `retained_preparations_are_drained_in_finite_pages_before_fresh_discovery` first completed zero slices for 65 retained candidates (a full 64-record recovery page led to an unresolved-preparation checkpoint conflict). Green finishes the bounded recovery page and defers fresh discovery until the next page is drained.
- Independent read access protocol: `source_access_checks_are_read_scoped_and_cannot_substitute_container_or_profile` first rejected the new request as invalid. Green validates correlated `authorize` / `authorized` frames; neither a Reporter grant nor substituted native scope/profile is accepted.
- Access capability host: `source_access_verification_requires_explicit_advertisement_before_exposing_read_handles` first rejected the new manifest field. Green requires explicit `authorize=true`, uses the verified process and rejects unadvertised support before exposing a handle. Legacy Source-only discovery manifests remain loadable.

- GitHub grant verification: `source_authorization_rechecks_the_current_private_grant_without_listing_tickets` initially could not compile because the provider did not implement the newly required Source command. Green verifies the current private grant's class/resource scope and returns correlated access facts without HTTPS or Issue selection. Replacing that file with a Reporter grant is denied.
- Runtime current grant on replay: `retained_replay_rechecks_the_current_read_grant_in_a_verified_process` first completed instead of deferring after the fixture revoked current read access. The runtime now delegates every admission/replay check through the content-verified Source process, independently of fresh discovery.

- Pending raw snapshot authorization: `pending_unmapped_snapshots_require_current_read_authority_before_artifact_publication` first published private artifacts under the revoked fixture grant; Green checks current verified read authority before mapping/publication and again before admission. Its retained-page fixture uses a 20s finite claim so 64 separate real Python process checks fit without weakening the production deadline test.

After the passing 16 runtime source tests, process translation and candidate admission were extracted into private modules; the full 103-test server unit suite preserves existing installation, readiness, ordinary runtime and shutdown behavior. No production behavior was intentionally changed by this extraction. Documentation/formatting-only updates are outside executable TDD and are checked separately.

- New exclusive group during an old standalone call: `installing_an_exclusive_group_revokes_an_in_flight_standalone_scan` first completed the old standalone scan instead of deferring. Green rejects that owner in the shared current-target store guard, so selection/preparation/checkpoint/admission cannot bypass new group ownership. The following group pass admits normally. PostgreSQL uses the same Project lock as immutable group publication. The legacy cross-store fixture now uses separate standalone and grouped inputs rather than incorrectly exercising an exclusive member as an independent scan.

- Native WIP backpressure: `factory_wip_backpressure_does_not_exhaust_the_provider_retry_budget_or_republish_inputs` first exhausted its Source after the third candidate hit the existing two-active-Run limit. Green must defer that retained candidate across more passes than the provider retry budget without disabling the Source or republishing its six original input documents. The initial driver with only two candidates did not reach the configured capacity and was corrected before establishing Red.

- Private expiry while recovery is pending: `expired_pending_snapshots_wait_for_retention_without_exhausting_or_resurrecting_work` first permanently exhausted the Source while awaiting cleanup. Green must defer without spending attempts, let the independent retention worker clear the original snapshot, then classify a repeated observation of that same expired item without renewing it, publishing bytes or admitting Work.

A full regression exposed a fixture timing assumption after adding separate grant checks: the original 400ms claim could expire during preparation before the deliberately stalled VCS port was reached. Its finite test claim was widened to 1500ms (500ms per process operation, 3s test watchdog), preserving the deadline assertion while allowing the positive grant/preparation setup to finish. No production timeout was widened.

## Regression commands and boundaries

All Cargo commands use the task-owned directory `/tmp/octacity-source-35-build.PRI4PX` with `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`. Builds ran sequentially. No workspace target directory was removed or replaced.

```sh
cargo test --locked --jobs 2 -p octacity-server --lib -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --test manual_trigger --test factory_artifact_reader -- --test-threads=2
# OCTACITY_POSTGRES_URL selects only the owned disposable PostgreSQL fixture.
cargo test --locked --jobs 2 -p octacity-server-store-postgres --test migrations --test factory_discovery -- --ignored --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-store -p octacity-work-source-protocol \
  -p octacity-server-work-source -p octacity-adapter-protocol -p octacity-server-adapter-host \
  -p octacity-github -p octacity-delivery-protocol -p octacity-server-delivery --all-features
cargo clippy --locked --jobs 2 -p octacity-server -p octacity-server-application \
  -p octacity-server-store -p octacity-server-store-postgres -p octacity-work-source-protocol \
  -p octacity-server-work-source -p octacity-github -p octacity-delivery-protocol \
  -p octacity-server-delivery --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --jobs 2 -p octacity-server \
  -p octacity-server-application -p octacity-server-store -p octacity-server-store-postgres \
  -p octacity-work-source-protocol -p octacity-server-work-source -p octacity-github --all-features --no-deps
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
cargo audit
cargo deny --locked check licenses sources
git diff --check
```

Observed behavioral results: **390 passing tests and one compile-fail doctest**:
106 server unit tests (19 Source worker contracts), 60 application admission tests,
34 ordinary/manual trigger tests, 5 Artifact reader tests, 71 core store tests,
85 protocol/host/provider tests, 23 PostgreSQL Factory contracts, 6 migration tests.
The final Source suite includes exact group revocation, current installed read
access, original intent recovery, bounded concurrency/fair discovery, remote retry
lower bounds, unavailable-provider exhaustion, absolute process/pass deadlines,
shutdown, native WIP backpressure and retention coordination.

Clippy passed for all selected targets/features with warnings denied. Architecture
passed with 63 workspace packages and 252 allowed edges. Audit found no denied
advisories; existing allowed unmaintained warnings remain `bincode` and
`unsafe-libyaml`. Licenses and sources passed. Only existing internal workspace
crates were added to the composition root; no external package was introduced.

PostgreSQL image `postgres:18.6` ran on loopback port 32773 with dummy credentials
and label `octacity.task=source-3.5`. Each contract created/dropped its own database.
The exact owned container id and volume were inspected, then removed together;
no user service was touched. Fixture processes and HTTPS peers use generated private
files and disposable resources. GitHub/GitLab/Jira/custom names in the runtime tests
are provider-neutral process peers, not claims of live GitLab/Jira qualification.

Source activation remains explicit through the trusted runtime installer; neither
ordinary CI/CD nor manual Factory admission needs Source credentials or a provider
call. Source availability is excluded from admission readiness. Source cleanup
runs independently even with polling/Factory disabled. No Reporter writes, REST/UI
activation, real repository mutation or paid model execution was performed. Those
remain later tasks. No commit was requested or created for task 3.5.

Rustdoc passed for all seven selected packages with warnings denied. Formatting, strict OpenSpec validation and whitespace checks passed. Only task 3.5 was marked complete; tasks 4–6 remain open.
