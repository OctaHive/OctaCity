# Review fixes verification

## Addressed findings

1. **Replay after private input expiry/cleanup.** Memory and PostgreSQL retain
   current input/version, authorization, scan fence and original-intent checks.
   The private materialization lifetime gate now applies only to fresh Work.
   Exact replay and correlation recovery reuse the original authoritative receipt
   without new artifact reads, VCS resolution, Work/Run or admission evidence.
2. **Cleanup failure isolation.** A failed object or preparation acknowledgement
   increments the page's safe `failed` count. Other preparations are still
   cleaned, the page cursor is returned, and failed records remain eligible on
   the next cursor sweep. Failure never clears that preparation's snapshot.
3. **Scheduled cleanup reconciliation.** Initial private expiry and due late-write
   checks are separate indexed PostgreSQL selections. Completed raw/rejected
   preparations with no objects leave the queue. Byte tombstones retain a
   trusted configurable recheck interval; successful checks become sparser with
   age since first cleanup. Late physical writes remain removable, without an
   invented deadline for uncertain storage requests. Duplicate acknowledgements
   cannot postpone the current schedule and do not rewrite frozen preparation
   documents. Migration 0061 adds one scheduling column and relevant indexes;
   existing cleaned byte reservations receive an initial reconciliation.
4. **Canonical admission probe.** Direct and prepared ingress now call the same
   `factory_source_admission_probe` constructor. Existing domain separation,
   namespace, field order and transport key spelling are preserved.
5. **Query-only reader dependency.** `FactoryArtifactReader` accepts
   `SourceArtifactQueryStore`, which has one read method and no configuration,
   scan, admission or cleanup methods. Both real store adapters implement it.
6. **Glossary consistency.** `CONTEXT.md` distinguishes ordinary Build-owned
   artifacts from Project-owned prepared Factory inputs. The architecture
   document describes the updated replay, reader and cleanup contracts.

The shared input byte/title/body and lifetime ceilings are named constants.
Installed mappings can narrow them. Historical migration literals remain frozen
schema history, not runtime configuration. Protocol identity strings also remain
fixed protocol constants rather than operator-selected parameters.

## TDD evidence

| Slice | Observed Red | Observed Green |
| --- | --- | --- |
| Memory replay across expiry and cleanup | `staged_exact_replay_survives` returned a source Conflict after expiry | Four providers recover the same original Run before and after physical cleanup, with one correlation and one idempotency outcome |
| PostgreSQL replay | Expanded shared source contract returned Conflict at the expired exact replay | Memory/PostgreSQL exact expired and cleaned replays preserve original receipt and admission counts |
| Per-preparation cleanup isolation | `source_cleanup_isolates` returned Unavailable for the page after the first failed object | One preparation stays retryable, the other is cleared, cursor survives and retry removes the remaining bytes |
| Cleanup schedule and duplicate ACK | Missing explicit interval/acknowledgement scheduling API; shared PostgreSQL contract subsequently returned Conflict for duplicate ACK | No repeated cleanup before due time; old ACK cannot defer it; delayed physical bytes are deleted at the scheduled check; later checks back off and first-cleanup/publication/expiry timestamps remain unchanged |
| Query-only input reader | Missing `SourceArtifactQueryStore` interface | The existing ownership/digest/size/retention reader contract passes with an adapter implementing only one query method |

Refactoring the probe and shared ceilings had passing behavioral coverage first:
`staged_direct_and_prepared` passed before and after extraction, with one Run,
VCS resolution and admission outcome for both ingress paths across four providers.
`source_mapping_` passed before the constant extraction and remains in the full
admission regression. No domain admission, eligibility, business duplicate or
manual-input rule was changed by those refactors.

Commands used for the slices:

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib staged_exact_replay_survives -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib source_cleanup_isolates -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib source_cleanup_schedules -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib source_cleanup_ -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib staged_direct_and_prepared -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib source_mapping_ -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib source_inputs_are_read -- --nocapture
OCTACITY_POSTGRES_URL=<owned-disposable-url> cargo test --locked --jobs 2 \
  -p octacity-server-store-postgres --test factory_discovery source_publication_is_atomic -- --ignored --nocapture
```

## Regression commands

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

## Observed regression results

All regression commands above passed on 2026-10-16:

- Application admission: 40 tests.
- Manual trigger: 34 tests; ordinary artifact reader: 5 tests.
- Factory core: 293 tests; store core: 71 tests; 2 compile-fail doctests.
- PostgreSQL Factory discovery: 23 tests; migration compatibility: 5 tests.
- Clippy with warnings denied, formatting, architecture (61 packages, 245
  dependency edges), strict OpenSpec validation and diff whitespace checks passed.

Total: 471 regression tests plus 2 doctests, without counting repeated slice runs.
The PostgreSQL suite used the disposable service described below, including the
single-connection contract and manual-admission migration/replay preservation.

## Scope and operations

Manual admission remains a separate namespace and requires no Source capability,
provider credentials, scan claim or cleanup worker. The unchanged installed-source
contracts exercise GitHub, GitLab, Jira and a custom provider. No provider-specific
branch, SDK, credential flow or business classification was introduced.

Worker composition remains in tasks 3.x. It must supply the trusted positive
recheck interval, observe the failure count and wrap the cursor between sweeps.
Tombstone metadata remains for provenance and occasional late-write checks;
private input retention deadlines and read authority are never renewed.

PostgreSQL tests use only the owned loopback `postgres:18.6-bookworm` container
labelled `octacity.test=source-review-fixes`. No provider process, live repository,
paid model or user stand was activated. No commit was requested or created.
After successful database regressions, its identity and ownership label were
checked and the owned `--rm` container was stopped and removed.
Existing work from tasks 2.1–2.5 is preserved. OpenSpec progress remains 9/29;
this review follow-up does not start task 3.1.

Glossary, documentation and SQL whitespace-only edits have no executable behavior
to drive with TDD. Architecture, format, strict OpenSpec and diff checks validate
the prose/formatting; behavioral changes and migration are covered above.
