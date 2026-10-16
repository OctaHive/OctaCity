# Task 2.5 verification

## Scope and public boundaries

`FactorySourceAdmissionHandlers::admit_prepared` connects frozen source
preparations to the existing atomic admission/correlation store operation. Its
command selects a current installed input and the original preparation under a
scan claim; it accepts no replacement ticket text, target, Work or intent.
Fresh admission shares the ordinary Repository resolution, immutable target
validation and dependency visibility path with manual/source admission.

Project visibility precedes private preparation lookup. The original intent's
selected dependencies are authorized before replay or VCS work. Replay uses the
original receipt without resolving a mutable base. Trusted owner time is read
after VCS returns and supplied to the store's existing scan/version/fence checks.
Work, Run and source correlation are committed by the existing store authority.

The tests use application admission, source/configuration/store ports and the
ordinary `FactoryFlowRunner` with controlled Build/output adapters. Provider
names (`github`, `gitlab`, `jira`, `custom-tracker`) exercise the same neutral
contracts; no provider enum, label-name dispatcher or alternate duplicate
analysis was added. No provider client or live source activation is involved.

## TDD and preservation evidence

The pre-change admission baseline was 31 passing tests. New behavior was driven
in small public-boundary cycles:

| Slice | Observed Red | Observed Green |
| --- | --- | --- |
| Prepared staged admission | Missing `AdmitPreparedSourceFactoryWorkCommand` and `admit_prepared` API | One native item enters two independently configured inputs/targets once each, with distinct Work/Run ids, original artifacts and common Issue correlation; restart replays without VCS or extra admission evidence |
| Owner time after asynchronous VCS | Missing trusted-clock injection API | Controlled resolution advances time to the exclusive scan deadline; no Work/Run correlation or admission evidence is committed |
| Project privacy before preparation discovery | Assertion failure: hidden Project returned `Conflict` from a missing private preparation instead of authorization `Invalid` | Both hidden and restricted visibility deny the Project before preparation existence can influence the result; no VCS or admission effect |

Commands for these Red/Green cycles:

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib staged_prepared_admission -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib staged_prepared_admission_rechecks_owner_time -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib staged_prepared_admission_denies_hidden_project -- --nocapture
```

Additional characterization tests preserve existing contracts through the new
prepared path; they require no changes to the domain's business rules:

- For each of four providers, input replacement changes the pinned target and
  mapping/selector reference while retaining its stable input id. After the
  previous scan lease ends, the current owner recovers the original snapshot,
  input version, Work/Run and unchanged journal. Changed content/label observation
  does not create another processing cycle. A fixture attempt to claim before
  the old lease ended was corrected; it is not counted as behavioral Red.
- The same literal native external key can still enter manual admission under
  its independent namespace. Manual retry returns its own original Run; neither
  source authority nor a scan claim is needed by the manual command.
- Sixteen configured Flow executions (four providers times four independently
  specified evidence cases) check eligible, business duplicate, out-of-scope and
  inconclusive Work. Source labels include `approved`, `bug` and an unrelated
  operator label; task text requests skipping checks. The normal eligibility
  Build and deterministic gate still execute. Independent verifier facts govern
  even when the model report claims the opposite: eligible selects `classify`,
  duplicate/out-of-scope select `rejection`, inconclusive selects `escalation`.
  No implementation node is dispatched before those gates.
- Replaying each already-executed case with unavailable VCS returns the original
  Run and preserves the complete current Flow journal; business checks do not
  run again.

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib staged_replacement -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib staged_source_labels -- --nocapture
```

## Regression commands

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --test manual_trigger --test factory_artifact_reader -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --lib configured_intake_research_unknown_step_and_pool_share_one_owner_interface_across_restart -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-factory supplied_eligibility_configuration_stops_duplicates_uncertainty_and_out_of_scope_work -- --nocapture
cargo clippy --locked --jobs 2 -p octacity-server-application --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

## Observed regression results

- Application admission: 36 passed, including all 5 new staged contracts and
  the existing manual/source authorization, replay and configuration limits.
- Manual-trigger integration: 34 passed.
- Artifact reader/context integration: 5 passed.
- Existing configured Flow owner/research/pool/restart journey: 1 passed.
- Existing supplied eligibility/business duplicate gate contract: 1 passed.
- Total: 77 distinct tests passed. Provider/evidence combinations inside those
  tests are not counted as separate tests.
- Application Clippy, all targets/features, warnings denied: passed.
- Architecture check: passed (61 workspace packages, 245 dependency edges).
- Formatting, strict OpenSpec validation and diff checks: passed.

## Operational boundary

No schema, external provider API, credentials, UI, source polling, Reporter
process or running user stand was changed or activated. Runtime composition
remains the separately planned tasks 3.x; this task completes the prepared
admission bridge and staged pipeline contracts. The existing manual management
endpoint and source namespace remain available independently of those workers.

The new command exposes only typed ids/claims. Original private snapshots and
Artifact bytes are not diagnostics, and labels/model prose grant no execution
or acceptance authority. Existing admission audit/idempotency facts remain the
only authority; no second deduplication journal or lifecycle was introduced.
No dependencies were added. Security review covered scope substitution, Project
visibility, immutable intent reuse, stale owner time and untrusted label/model
claims. Documentation-only changes are validated by architecture, OpenSpec and
diff checks; TDD does not apply to their prose.

No commit was requested. Prior changes from tasks 2.1–2.4 and the unrelated
`add-factory-role-profiles` planning directory are preserved.
