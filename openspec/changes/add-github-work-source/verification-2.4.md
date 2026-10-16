# Task 2.4 verification

## Implemented boundary

Immutable normalized ticket snapshots map to private task and acceptance JSON
Artifacts through `FactoryWorkSourcePreparer`. The same contracts exercise
`github`, `gitlab`, `jira` and `custom-tracker`; provider names select no behavior.
Installed settings pin the complete Source binding, acceptance mapping,
Repository source selection and classification. Ticket text remains data.

Acceptance must explicitly select a normalized text field, the entire body or
an exact Markdown heading. Missing required content, invalid snapshots and
oversized input produce safe classifications, with no invented criteria,
truncation, artifact publication or Run. The mapper has no provider client or
comment/attachment/link-fetch capability. Literal links remain private text.

Bounds are UTF-8 bytes: title at most 4 KiB, body/normalized field at most
256 KiB, normalized snapshot and each encoded artifact at most 1 MiB. Metadata
is bounded as well (32 fields, 64 labels, 256-byte display/update identities).
Task and acceptance retain the original installation, native container/item,
input/version, mapping, labels/state, target Configuration, Repository, root
Flow, selection and raw snapshot digest provenance.

Source materializations reserve exact artifact/upload identities and cleanup
metadata before physical I/O. Partial/uncertain writes remain unpublished and
restart reuses the first snapshot, original settings digest, object identities,
retention deadline and admission intent. Atomic publication is fenced by the
current selected input/group owner. No new deduplication authority is added.

Source inputs belong to a Project/source preparation, without fabricated Build
or Agent output provenance. Explicit Factory context reads verify Project,
identity, size, SHA-256 and retained read window. Protected Agent input transfers
resolve source publication metadata and cap download lifetime at expiry.
Ordinary output/evidence reads still require ordinary Build publication.

Private raw snapshots have a hard 30-day deadline from initial reservation.
Materialization policy can shorten that lifetime and never renews on retry.
Fresh prepared admission rejects expired, cleaned or substituted materializations.
The bounded retention worker deletes exact reserved objects before clearing raw
snapshot bytes; this covers successful, interrupted, rejected and pre-mapping
orphans. Deletion failure remains retryable. Cursor sweeps revisit tombstones
so a late in-flight write is deleted on a subsequent sweep. Correlation and
immutable identity metadata remain available after private bytes are cleared.

Migration 0060 is additive. Legacy 0059 snapshot documents use a conservative
fixed epoch-based 30-day deadline, matching the SQL default, so unknown-age
private snapshots do not gain a fresh retention period. Existing manual
admission and replay survive migration unchanged.

## TDD evidence

Each behavior was tested before implementation through the application or
store public boundary. Environment/fixture compilation repairs are excluded
from Red/Green evidence.

| Behavior | Observed Red | Observed Green |
| --- | --- | --- |
| Original bounded task/acceptance and provider-neutral provenance | Missing normalized mapping API | Literal untrusted title/body, explicit field acceptance and original provenance for four providers |
| Missing, oversized, foreign or unknown content | Missing safe classification API | Ten independently specified rejection cases, including UTF-8 size and complete artifact cap |
| Explicit body/section acceptance, literal links | Missing explicit Body/BodySection contract | Exact configured section/body, missing section rejected; no fetch port exists |
| Restart after uncertain byte publication | Missing preparer/materialization API | First private bytes remain unpublished; restart repeats exact original artifacts/intent despite changed observation |
| Missing acceptance disposition | Missing retained rejection API | Safe retained reason; zero objects, intent, admission or idempotency evidence |
| Metadata publication, expiry and cleanup in memory/PostgreSQL | Missing store lifecycle API | Four-provider parity, current-owner fencing, invisible pending bytes, fixed expiry and revisitable cleanup |
| Expired input admission | Assertion failed: fresh Work was accepted after input expiry | Fresh admission rejected with zero admission evidence under a new valid scan owner |
| Physical cleanup after interrupted/late writes | Missing retention worker API | Failed deletion keeps snapshot; retry clears bytes; a late write is removed on the next sweep |
| Private Factory input reading | Missing `with_source_inputs` reader API | Exact source data readable; wrong Project, digest, limit and retention rejected; ordinary output reading rejects it |
| Protected Agent input metadata | `published_execution_input` returned Artifact NotFound for published source input | Exact upload/digest/size/expiry returned; ordinary Build output absent; cleaned source unavailable |

Commands used for the individual Red/Green cycles:

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib source_mapping_freezes -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib source_mapping_rejects -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib source_acceptance_ -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib source_preparation_recovers -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib missing_acceptance_is -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib expired_source_artifacts -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib source_cleanup_retries -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib source_inputs_are_read -- --nocapture
OCTACITY_POSTGRES_URL=<owned-disposable-url> cargo test --locked --jobs 2 \
  -p octacity-server-store-postgres --test factory_discovery source_publication_is_atomic -- --ignored --nocapture
```

The schema inventory regression initially reported the two new additive tables;
the expected migration inventory was updated, after which the migration and
previous-binary snapshot rollback rehearsal passed.

## Regression evidence

- Before changes: application admission 23 passed; ordinary Artifact reader 5 passed.
- After changes: application admission 31 passed, including all 8 added mapping,
  preparation, retention and input-reader tests.
- Manual trigger integration: 34 passed. Manual Factory admission remains
  available through its existing management command and source identity.
- Ordinary Artifact reader/context integration: 5 passed.
- Factory/store all-feature suites: 293 + 71 passed, plus two compile-fail doctests.
- PostgreSQL Factory discovery/source contracts: 23 passed.
- PostgreSQL migrations: 5 passed, including reentrant migration and rollback
  rehearsal. The manual compatibility test preserves the original Work/Run,
  audit/idempotency counts and replay receipt across migration/restart.

Regression commands:

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --test manual_trigger --test factory_artifact_reader -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-factory -p octacity-server-store --all-features -- --test-threads=2
OCTACITY_POSTGRES_URL=<owned-disposable-url> cargo test --locked --jobs 2 \
  -p octacity-server-store-postgres --test factory_discovery --test migrations -- --ignored --test-threads=2
cargo clippy --locked --jobs 2 -p octacity-server-application -p octacity-server-store \
  -p octacity-server-store-postgres --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

- Clippy for application/store/PostgreSQL, all targets/features: passed with
  warnings denied. Enum allocation and conditional/style warnings were corrected
  with behavior tests kept passing.
- Formatting, architecture (61 packages / 245 edges), strict OpenSpec validation
  and diff checks: passed.
- Total: 462 distinct behavioral tests and two compile-fail doctests passed.

## Scope

No provider process, credentials, live repository or user stand was activated.
PostgreSQL verification used only an owned disposable `postgres:18.6-bookworm`
container bound to loopback. Source process/polling and runtime worker scheduling
remain tasks 3.x; composition must install the source input reader and schedule
bounded retention sweeps with cursor wraparound. This task supplies and verifies
the preparation/retention owners without activating a poller.

Documentation changes have no executable behavior; architecture, OpenSpec and
diff checks validate them. The owned disposable PostgreSQL container was stopped
and removed after testing. No commit was requested for this task. Prior changes
from tasks 2.1–2.3 remain in the working tree.
