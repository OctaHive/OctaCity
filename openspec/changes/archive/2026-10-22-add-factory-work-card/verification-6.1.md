# Task 6.1: safe card and exact execution/result projections

## Boundaries

Application queries compose current Issue metadata and its pinned workflow,
existing status history, original execution request/Work associations, common
Factory discovery/diagnostics and ordinary authorized Job reads. No persistence
schema, provider port, admission path, executor, execution journal or lifecycle
owner is added. Manual cards need only the existing Issue/workflow store. Detailed
Job reads are a separate optional composition.

Every selected execution remains independent. Preparation is distinct from an
available Run or unavailable technical observation; parallel/queued Runs retain
their own identities. Requirement digests identify matching versus historical
content without claiming action applicability or current-candidate acceptance.
Artifact-only lineage retains absent request/revision identities.

Result collections reuse the existing safe typed projections and exact original
Work/Run/Flow/cycle/node/input/candidate/evidence/plan/Decision subjects. Missing
records do not imply a pass; indeterminate assessments remain indeterminate.
Live collection pages do not claim an atomic card snapshot, complete verification
coverage or overall acceptance. The original model/tool profile is optional and
separate from actual enrolled-worker assignment. Exact Build/Attempt/Job log
references grant no content authority and never select a later retry. Responses
exclude requirement bodies, Context contents, JobSpec, placement, output bodies,
leases, rendered prompts, transcripts and private transfer URLs.

Existing Issue visibility precedes association lookup; existing Factory/Project
authorization precedes diagnostics. Job expansion verifies retained linkage and
independent Build/Job visibility before the ordinary Job read. No query invokes
a provider, starts work, changes status or publishes inputs. REST/merged timeline
observation, Console and qualified detailed tool/check/artifact paths remain their
separately scheduled section 6 tasks.

## TDD evidence

Commands use `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2`, `--offline`, and
`cargo test -p octacity-server-application --lib <filter>`.

| Filter | Observed Red | Observed Green |
| --- | --- | --- |
| `issue_card_projects_pinned` | `GetIssueCardQuery` was absent | Manual card returns its exact workflow/status without private bodies or any Factory/provider dependency |
| `issue_execution_projection_keeps` | Selected execution projection and observation contracts were absent | Pending preparation, two separate admitted Runs and historical requirements after a card edit retain original identities without mutation |
| `issue_execution_projection_reads_artifact` | Associated-Work query was absent | Legacy Artifact admission reads without fabricated request/requirements provenance; another Issue cannot claim the Work |
| `issue_execution_records_keep` | Issue-bound collection query was absent | Original indeterminate candidate/Flow/cycle/attempt survives, absent evidence/Decision is not success, safe frozen call profile contains no private invocation material |
| `issue_execution_job_projects` | Job expansion query/optional handler were absent | Ready/unassigned versus Running/actual worker, independent original profile and exact Build/Attempt/Job log/event target; foreign record rejects |
| `issue_card_retains_planning` | Safe card priority/planning fields were absent | Planning remains visible, replaced Project workflow does not repin the card and business history preserves its exact original manual transition |

Missing public APIs established Red; fixture/import repairs and Clippy/style
corrections do not count as behavioral Red. The existing journal-only seed did
not populate the admission index required by common discovery. A separate
test-support seed now validates and retains that original admission/index under
one lock; existing worker fixtures and production admission remain unchanged.
The real shared journal contract drives accepted records before projection tests.

Additional characterization checks the reused mechanisms: separate current
Factory/Project and Build/Job denial, hidden Issue reads, common page bounds and
collection cursor binding, and exact ChangeSet/Evidence/Decision subjects without
business-status mutation. These checks required no replacement authorization,
result evaluation or cursor implementation.

## Regression verification

Before the final full regression, `cargo test --offline -p
octacity-server-application --lib issue_` passed 114 tests. The additional
candidate/evidence/Decision collection characterization passed independently.
All final checks passed:

| Command | Result |
| --- | --- |
| `cargo test --offline -p octacity-server-application -p octacity-server-store --lib` | 597 application and 81 store tests passed, including all eight new projection tests and existing admission, status/evidence, Source/Reporter, control and manual-task coverage |
| `cargo clippy --offline -p octacity-server-application -p octacity-server-store --all-features --all-targets -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `python3 tools/check_architecture.py` | Passed: 63 packages, 256 dependency edges |
| `openspec validate add-factory-work-card --strict` | Passed |
| `git diff --check` | Passed |

The full application suite includes existing long-running Factory requirements
and release journeys; all completed without failures. Clippy's large-enum and
unnecessary-closure findings were corrected without changing serialized behavior
or suppressing checks. No PostgreSQL schema/adapter, REST schema/generated client,
Console or dependency changed, so database services and frontend builds were not
needed for this application-only slice. Documentation-only edits use
specification/diff validation rather than executable TDD.

Builds kept incremental compilation/debug symbols disabled and two compilation
jobs. Workspace checks retained approximately 10–11 GiB free, with no disposable
database or additional checkout. Task 6.1 is complete: 33 of 56 tasks checked.
No commit was created.
