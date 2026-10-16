# Task 3.4 verification

## Scope

`FactorySourcePoller` runs one bounded application polling slice using an injected
clock, a controlled normalized page reader and a trusted candidate processor.
The reader owns provider translation; the processor must use existing fenced
selection, preparation and admission and return success only for durable
admission/replay or a classified disposition. No provider SDK, GitHub DTO, label
vocabulary, URL interpretation or second admission/deduplication authority enters
the application state machine. Fixtures exercise GitHub, GitLab, Jira and an
unrelated tracker through the same contracts.

Before any candidate effect, the complete first page is retained in the existing
private fenced scan record. Settled candidates are removed one at a time; the
covered continuation advances only after the entire page is settled. A partial
failure, changed provider listing, process restart or lease takeover therefore
cannot skip its original remaining snapshots. Provider failures retain the same
scan identity/window/cursor. An expired owner cannot invoke candidate processing.
The composed admission fixture loses a reply after commit, retries the retained
page, then observes edited content: it retains one Run, one VCS resolution and
only the two original task/acceptance Artifact publications.

Item, page and canonical normalized-byte budgets bound each invocation. A reader
cannot enlarge the installed page limits. Hard ceilings are 100 items and 4 MiB
per page, 100 pages, 10,000 settled candidates and 64 MiB per invocation; each
retained candidate is at most 1 MiB. Installed policies may narrow them. The
reader receives its page ceilings before reading; the verified process/provider
separately bounds raw wire bytes. A resumed private page counts against the byte
budget. Before another read, the owner reserves enough remaining capacity for a
complete maximum-sized page, so a budget stop may conservatively leave capacity
unused. It never marks an incomplete pass as complete.

Incremental windows are inclusive and overlap the last covered boundary. A
periodic full pass alternates page capacity with incremental discovery. Each
lane retains its original operation identity, fixed window and opaque cursor
across restarts; equal timestamps and repeated/edited observations are forwarded
to ordinary admission replay. Timestamps are not a duplicate authority.

Private pages have fixed, non-renewing retention, bounded by the existing 30-day
source retention ceiling. Expiry during processing stops before the next
candidate. The existing source-input retention worker also clears expired pages,
including ones outside current enabled selection. Cleanup is bounded to 64 pages
and shares the PostgreSQL Project writer lock, rechecking expiry after locking.
Discarding an unfinished page resets covered progress for a conservative epoch
rescan. Legacy checkpoints lacking an original polling context also restart
conservatively. Existing preparations retain their independent recovery/cleanup
policy; an unresolved preparation still blocks checkpoint advancement.

Additive migration 0062 widens the existing scan-document ceiling to accommodate
bounded normalized pages plus JSONB whitespace and the existing envelope, and
adds an expiry index. It does not rewrite legacy documents or add a second
progress/admission table. Both store implementations retain the existing owner,
fence, current-definition and compare-and-set checks. Cursor/snapshot bytes are
absent from Debug diagnostics.

Concrete verified provider composition, enabled/group selection, scheduling,
concurrency, backoff, timeout and shutdown remain task 3.5. Reporter writes remain
section 4. This slice starts no worker or live provider call. Ordinary manual
Factory admission remains independent of integrations and credentials.

## TDD evidence

The public boundaries are application polling, ordinary source preparation and
admission, `FactorySourceStore`, and the existing retention worker. Existing
admission contracts passed before implementation: **40 tests**. Tests use injected
time and controlled provider/byte/VCS boundaries; the actual memory and PostgreSQL
stores provide authoritative persistence and replay behavior.

| Slice | Observed Red | Observed Green |
| --- | --- | --- |
| Restart mid-page | Polling public API missing (E0432) | Remaining original candidate resumes without another provider read for all four provider fixtures |
| Complete page byte ceiling | Oversized page was accepted | Rejected before processing and before covering its continuation |
| Invocation page/byte budgets | Four reads occurred where the independent byte budget allowed two | Page and byte stops retain the unfinished cursor, including empty pages |
| Trusted policy validation | Zero page-item budget accepted | Zero/inconsistent budgets rejected before activity |
| Overlap and equal timestamps | Completed scan performed no later read | Next inclusive window overlaps; repeated/edited observations reach existing admission |
| Fair reconciliation | Every read stayed incremental | Alternating full/incremental pages preserve both original contexts |
| Store bounds and legacy JSON | 101-candidate private page accepted | Item/snapshot/page/cursor/state bounds enforced; legacy JSON round-trips unchanged |
| Expired lease | Retained tail reached the processor after deadline | Failure occurs before another candidate callback |
| Private page retention | Policy/cleanup APIs absent (E0609/E0599) | Fixed expiry clears private bytes and resets progress safely |
| PostgreSQL page capacity | A valid 50 KB retained page failed at checkpoint with `Unavailable` under the old 32 KB constraint | Migration 0062 allows the bounded page; restart, fencing, CAS and expiry pass in both stores |
| Existing cleanup worker integration | Cleanup result/API lacked private-page expiry (E0609) | Existing worker purges the expired polling page without Artifact objects |
| Obsolete cursor context | Legacy private cursor was reused | Legacy/expired positions restart from epoch without borrowing the old cursor |
| Installed page item ceiling | Three candidates accepted under an installed ceiling of two | Whole page rejected before any processing, even within the byte budget |
| Expiry during processing | Both candidates processed after the first advanced the clock to expiry | Only the first reaches admission; its checkpoint does not renew page retention |
| Safe request diagnostics | Debug contained the original private provider cursor | Cursor is redacted |

Focused commands used for Red and repeated for Green (or included in the complete
passing admission suite):

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib restart_mid_page -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib oversized_pages -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib page_and_byte_budgets -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib polling_policies -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib completed_windows_overlap -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib reconciliation_and_incremental -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib storage_rejects_unbounded -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib expired_owner_cannot_process -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib retained_pages_expire -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib existing_retention_worker_purges -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib legacy_or_expired_private -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib provider_cannot_enlarge -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib private_page_expiry_during -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-application --lib polling_request_diagnostics -- --nocapture
# OCTACITY_POSTGRES_URL names only the owned disposable fixture service.
cargo test --locked --jobs 2 -p octacity-server-store-postgres --test factory_discovery \
  source_publication_is_atomic_owned -- --ignored --nocapture
```

Additional characterization tests passed before subsequent changes: partial
processor failure preserves the original candidate and tail; provider failure
reuses its exact frozen context; uncertain admission/repeated observations reuse
the authoritative Run without republishing artifacts or resolving VCS again.
Legacy-upgrade and database document-bound checks exercise migration 61 → current,
unchanged legacy progress, and rejection of arrays and oversized documents.
These checks use the public existing behavior rather than inventing another
receipt registry. Named ceiling extraction and formatting kept the behavioral
suite passing. Documentation-only edits have no executable behavior to drive
with TDD.

## Regressions and checks

Passed on macOS on 2026-10-16–17:

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --test manual_trigger --test factory_artifact_reader -- --test-threads=2
# OCTACITY_POSTGRES_URL selects the owned disposable PostgreSQL fixture.
cargo test --locked --jobs 2 -p octacity-server-store-postgres --test migrations --test factory_discovery -- --ignored --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-store -p octacity-work-source-protocol \
  -p octacity-server-work-source -p octacity-delivery-protocol -p octacity-server-delivery \
  -p octacity-server-adapter-host -p octacity-adapter-protocol -p octacity-github --all-features
# Dedicated temporary build directory; no shared target cleanup is performed.
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo clippy --target-dir /tmp/octacity-source-poll-34-checks.BYkngO --locked --jobs 2 \
  -p octacity-server-application -p octacity-server-store -p octacity-server-store-postgres \
  -p octacity-work-source-protocol -p octacity-server-work-source -p octacity-github \
  -p octacity-delivery-protocol -p octacity-server-delivery --all-targets --all-features -- -D warnings
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUSTDOCFLAGS='-D warnings' \
  cargo doc --target-dir /tmp/octacity-source-poll-34-checks.BYkngO --locked --jobs 2 \
  -p octacity-server-application -p octacity-server-store -p octacity-server-store-postgres --all-features --no-deps
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
cargo audit
cargo deny --locked check licenses sources
git diff --check
```

Observed: **274 distinct behavioral tests**, plus one passing compile-fail
doctest: 57 application admission tests (40 existing + 17 added), 34 manual
trigger tests, 5 Artifact reader tests, 71 core store tests, 78 protocol/host/
provider tests, 23 PostgreSQL Factory discovery contracts and 6 migration tests.
Clippy and rustdoc passed with warnings denied. Architecture passed with 63
packages and 250 edges; formatting, strict OpenSpec, diff whitespace, licenses
and sources passed. Audit passed with the two existing allowed unmaintained
warnings (`bincode`, `unsafe-libyaml`). Task 3.4 adds no dependency.

The first Clippy run was interrupted when the shared workspace `target`
directory disappeared, producing missing-temporary-directory errors. This is an
environment failure, not a Red behavior result. Repeating the complete affected
Clippy selection in the owned temporary target directory succeeded; rustdoc
used that same directory. No behavior changed after the passing regressions.

The PostgreSQL fixture used image `postgres:18.6`, loopback-only port 32772 and
dummy credentials. Each contract created its own disposable database. The owned
container id and label `octacity.test=source-poll-34` were verified before removing
only that container and its anonymous volume. No user stand, live provider,
external repository or paid model was used. Only task 3.4 is completed here;
progress is **13/29**. Task 3.5 remains unchecked. No commit was requested or
created.
