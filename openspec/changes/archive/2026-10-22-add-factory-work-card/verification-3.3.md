# Task 3.3 — PostgreSQL publication and crash recovery

Subsequent [review corrections](verification-review-fixes.md) separate private
documents from retained metadata and index cleanup scheduling; the original
qualification below records the task 3.3 delivery.

## Delivered boundary

The PostgreSQL adapter implements the existing `IssueExecutionStore`,
`ProjectTextInputStore` and `ProjectInputQueryStore` contracts. Migrations
0070–0073 add execution requests, shared Project preparations/object identities,
Work associations and private-input cleanup metadata. Manual Issue creation has
no Factory, Source, provider or execution prerequisite.

Request reservation locks the reviewed Issue revision and atomically commits its
original metadata receipt, scoped replay, safe audit and outbox fact. Admission
recovery reads require the retained security partition. Request/association
validation and preparation identity/lease calculations are shared with memory;
there is no second publication algorithm or quickstart journal.

Shared admission extends the existing ordinary Factory admission transaction.
The preparation fence, frozen subject and complete input publication are checked
inside that transaction, and the original Work/Run receipt is committed with the
preparation. Ordinary admission uniqueness, policy/WIP checks, audit and outbox
remain authoritative. Issue association validates the persisted Work and original
Run, retains exact reviewed lineage, and atomically updates association, request,
replay, audit and outbox. A failed association preserves the original admission;
no compensation deletes Work or creates a replacement. Already committed admission
can be associated after private-body cleanup using only its original metadata;
current authority and frozen classification still apply.

`FactoryRuntimeResources` supplies the common intake and one retained input reader
to native compilers/profile installation. Protected transfer uses the existing
Artifact/Agent input boundary and original private expiration. Project inputs
cannot appear as producing Build evidence. Controlled PostgreSQL/MinIO tests
publish/read exact bytes and exercise a real presigned download without a forge,
provider account, model or Agent image build.

Private Project inputs use the existing supervised
`FactoryExecutionInputRetentionWorker`, including unpublished reservations.
Cleanup locks linked ordinary Builds before checking holds and recording deletion
eligibility. Existing holds preserve inputs; new holds cannot claim preservation
once deletion has started. Eligible records are filtered before the page limit,
so a held oldest preparation cannot starve cleanup. A failed physical deletion
retains its exact cleanup identity. Successful acknowledgements retire input
visibility, scrub both private bodies after the pair is deleted, and retain
immutable receipt/cleanup metadata. Hourly rechecks remove delayed orphan writes.

HTTP request windows, status/resume routes and Console controls remain tasks
3.4–3.5; quickstart owns its preset/readiness/HTTP adaptation. Recovery in this
slice is exercised through the existing authorized shared submission and Issue
publication commands after actual process termination. No new execution owner,
scheduler, intake outbox, provider integration or automatic Issue creation is
introduced.

## Test boundaries and Red/Green evidence

Tests use public application/store/transfer/retention contracts with real isolated
PostgreSQL. VCS, clock and selected byte failures are controlled external ports.
Native resource tests additionally use real disposable MinIO. Test setup and fault
injection use trusted database fixtures; outcome assertions use the public reads,
receipts and discovery/cleanup ports. Missing required API compilation is Red;
fixture/import/type errors and dependency-environment failures are not Red.

| Slice | Observed Red | Green behavior |
| --- | --- | --- |
| Request persistence | PostgreSQL execution-request contract absent | Exact digests/revision and original scoped replay survive edits/restart |
| Shared preparation | PostgreSQL Project text store contract absent | Stable objects/base, fenced claims and original deadlines survive restart |
| Atomic ordinary admission | Shared submit returned store unavailable | Lost byte acknowledgement resumes original objects/base and commits one Work/Run receipt |
| Issue association | Publication returned store unavailable | Original revision is associated durably and replays the same Work/Run |
| Protected transfer | Project input returned Artifact not found | Exact metadata/bytes transfer through the existing protected boundary; expiry and output denial remain enforced |
| Native composition | Required resource intake/reader APIs absent | Common publisher/native reader and real MinIO presigned download preserve exact bytes |
| Private retention | Expired reservations returned zero cleanup identities | Abandoned/published input cleanup, holds, deletion retry, body retirement and late-write rechecks |
| Hold race | A late hold was accepted after deletion eligibility | Ordinary Build lock orders hold placement against private deletion |
| Safe reservation evidence | Initial reservation emitted no audit fact | Exactly one safe audit/outbox and one owner under concurrent reservation; request replay has one winner |
| Cleanup fairness | A held oldest record consumed the entire one-item page | Unheld expired records progress within the requested page bound |
| Completed receipt after body cleanup | Exact Issue replay conflicted after private-body retirement | Historical receipt without private-body resurrection or byte effects; changed classification still conflicts |
| Pending link after body cleanup | Already admitted but unlinked execution could not resume after body retirement | Original committed Work/Run is associated from retained metadata with no new bytes or admission |

Additional crash characterization terminates a separate publisher process after
its first object write, both object writes, complete publication, committed
admission and committed association. Restart always uses the same objects and
frozen base, retains the exact original revision after later edits, and discovers
one original Run. Before association commits, the Issue receipt stays pending.

Database fault characterization rejects the reservation audit, pair publication,
Work insertion or association audit before commit. Every failure leaves the
appropriate original stage; retry commits one result. An association audit failure
rolls back both the link and admitted Issue status while preserving ordinary
admission. Migration qualification upgrades schema 69 with an existing manual
Issue, repeats migration, and retains the original revision and manual creation.

Baselines before refactoring: five Issue execution scenarios, 24 PostgreSQL Issue
scenarios and 44 native runtime composition scenarios passed. The unchanged task
3.2 baseline also supplies the seven shared publication scenarios.

## Commands

Build settings reuse the existing target directory and constrain disk growth:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
# OCTACITY_POSTGRES_URL points only to an owned disposable loopback service.
# Native S3 qualification additionally sets OCTACITY_MINIO_ENDPOINT,
# OCTACITY_MINIO_ACCESS_KEY and OCTACITY_MINIO_SECRET_KEY for disposable MinIO.
```

Focused Red/Green commands (one matching scenario per slice):

```sh
cargo test --locked -p octacity-server-store-postgres --test issue_execution execution_reservation -- --ignored
cargo test --locked -p octacity-server-store-postgres --test issue_execution project_text_reservation -- --ignored
cargo test --locked -p octacity-server --test project_text_intake durable_shared_text -- --ignored
cargo test --locked -p octacity-server --test project_text_intake issue_publication_associates -- --ignored
cargo test --locked -p octacity-server --test project_text_intake published_project_inputs -- --ignored
cargo test --locked -p octacity-server --test project_text_intake native_resources_publish -- --ignored
cargo test --locked -p octacity-server --test project_text_intake shared_input_retention -- --ignored
cargo test --locked -p octacity-server-store-postgres --test issue_execution project_text_reservation_commits -- --ignored
cargo test --locked -p octacity-server --test project_text_intake completed_issue_replay -- --ignored
cargo test --locked -p octacity-server --test project_text_intake admitted_pending_issue -- --ignored
```

The retention command was run Red/Green again for late-hold ordering and bounded
page fairness. Crash and transaction-fault commands:

```sh
cargo test --locked -p octacity-server --test project_text_intake killed_publishers -- --ignored
cargo test --locked -p octacity-server --test project_text_intake transaction_failures -- --ignored
```

## Qualification results

```sh
cargo test --locked -p octacity-server-store -p octacity-server-application --all-features --lib
cargo test --locked -p octacity-server-store-postgres --test issue_execution \
  --test issues --test factory_discovery --test factory_retention --test retention \
  --test migrations -- --ignored
cargo test --locked -p octacity-server --test project_text_intake -- --ignored
cargo test --locked -p octacity-server-application --all-features --lib project_text_intake_
cargo test --locked -p octacity-server-application --all-features --lib issue_execution_
cargo test --locked -p octacity-server --lib runtime::factory
cargo clippy --locked -p octacity-server-store-postgres -p octacity-server-application \
  -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
openspec validate add-factory-bugfix-quickstart --strict
git diff --check
```

| Qualification | Result |
| --- | --- |
| Full application/store libraries, all features | 526 application and 81 store tests passed |
| PostgreSQL Factory discovery and retention | 24 + 7 passed |
| PostgreSQL requests/preparations and concurrency/migration upgrade | 4 passed |
| PostgreSQL existing Issues | 24 passed |
| PostgreSQL general retention and migration rehearsal | 4 + 6 passed |
| Project intake integration target | 10 passed: nine behavioral scenarios and one subprocess-only fixture entry |
| Common/Issue boundary rechecks after retained-receipt corrections | 7 + 5 passed |
| Native Factory runtime composition | 44 passed |
| Clippy, all targets/all features | Passed with warnings denied |
| Architecture | Passed, 63 packages and 256 edges |
| Formatting, strict OpenSpec, local documentation links and whitespace | Passed |

The broad library regression preceded the final retained-receipt corrections;
after those corrections, both public-boundary selections, the complete integration
target and Clippy were rerun. The first migration rehearsal exposed an outdated
table inventory; its explicit inventory now includes the four new tables without
removing any assertions, and all six migration scenarios passed afterward.

An additional whole-server library run was **not clean on this host**. Delivery
shell-host tests intermittently returned unavailable or exceeded their one-second
startup window: the first run had 167 passed / four failed / 12 ignored; a serial
run with a dedicated temporary directory had 170 passed / one failed / 12 ignored.
The isolated cancellation case passed three consecutive retries (0.82, 0.57 and
0.14 seconds); the Delivery implementation/tests are unchanged and use their own
materializer rather than the new Project reader. This suggests host startup timing,
but its root cause is not qualified here. No Delivery assertion or timeout was
weakened. This broader host-fixture instability remains explicit; the 44 relevant
Factory composition tests and all new input/recovery scenarios passed.

Disk was checked around builds: approximately 19 GiB initially and 15 GiB after
qualification. Compilation reused the existing target with debug/incremental
outputs disabled and two jobs. Only already installed PostgreSQL/MinIO images
were used, with ephemeral data; no image builds/downloads, additional target
folder, unrelated cache deletion, live provider or paid model calls were needed.

Documentation/handoff edits have no executable behavior; formatting, local-link,
whitespace and strict OpenSpec validation apply instead of TDD. HTTP/Console and
quickstart implementation tasks remain unchecked. No commit was created.

Task 3.3 is complete; work-card progress is 17/56.
The task-owned PostgreSQL and MinIO containers were removed after qualification.
