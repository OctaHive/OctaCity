# Task 5.2: Issue effects use the existing Reporter outbox and fencing

## Scope and public boundaries

Tests drive accepted manual Issue commands, the existing outbox claim/settlement
and source-projection coordination ports, PostgreSQL migration/restart, and the
content-verified Reporter process. Factory action and Issue metadata effects use
one dispatch/retry loop, one append-only outbox table and one native-item ordering
journal. No synthetic Work/Run, provider SDK, new protocol or second outbox was
introduced. Existing Run row identities and their hash encoding are preserved.

An Issue row points to its accepted revision; immutable desired material stays in
the original atomic mutation event. The current mapping, card head and Source
input cannot retarget a retry. Explicit installed `issue_delivery` policy is part
of the immutable Reporter profile: finite attempts/delay, elapsed/output budget
and a total window from original acceptance. A claim, observation or restart
cannot renew that window. Work-only projectors leave Issue events untouched.

The shared Inspect/Apply path validates accepted material and live ownership
before each native write. Native opaque field groups share the existing atomic
acceptance sequence, observation generation, permits and late-write hazards
across both origins. Provider failure never reverses Issue metadata or accepted
Factory results. Unmapped/unsupported original inputs grant no native write.

Migration 0085 extends the existing journal. Public migration resumes frozen
pre-5.2 desired inputs in bounded atomic batches; missing desired material is
never populated from current configuration. Repeated migration is idempotent.
Readiness detects an unfinished publication phase even after the SQL migration
commits; Issue claims wait until retained inputs are fully published. Recovery
progress and completion are retained by migration 0086 in one singleton cursor,
committed with each batch. Completed readiness/claim checks read this marker
instead of scanning history; original intent reads use the existing partial
desired-publication index. This cursor is migration metadata, not another outbox.
Recovery
does not order an older Issue intent after a known newer effect, another
producer, or an ambiguous scoped native link. It retains the original input
without a native ordering permit. New acceptance publishes its order atomically
with the Issue revision and outbox event.

## TDD evidence

All Rust invocations use `--offline`, `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0` and
`CARGO_BUILD_JOBS=2`. Ignored PostgreSQL scenarios use `OCTACITY_POSTGRES_URL`
pointing to the owned disposable `octacity-issue-push-52` container, limited to
512 MiB RAM and 384 MiB tmpfs. Fixture/import compilation errors are excluded
from Red/Green evidence.

| Command/filter | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server-store-postgres --test issues source_push_freezes_original -- --ignored` | Original accepted desired material produced zero claimable Reporter rows | One Issue-owned row commits with the original mutation and is claimable after store reconstruction, without a Run |
| `cargo test -p octacity-server --all-features --lib manual_issue_push_recovers -- --ignored` | Required installed Issue policy API was absent; after adding the contract, the existing worker still handled zero Issue effects | Actual process Apply loses its response; reconstructed worker observes the same intent, confirms delivery and makes one total Apply |
| `cargo test -p octacity-server --all-features --lib issue_reporter_upgrade -- --ignored` | A genuine retained mutation from the preceding technical schema remained undispatched after migration | Original material is resumed once; repeated migration/replay adds no logical effect or Run |
| `cargo test -p octacity-server --all-features --lib factory_only_dispatcher -- --ignored` | A Work-only projector claimed the Issue event | Only explicitly installed Issue delivery support includes that effect family in the shared worker pass |
| `cargo test -p octacity-server --all-features --lib issue_reporter_material_read_outage -- --ignored` | An injected failure to read immutable material terminally settled the accepted effect | The claim remains recoverable; after the dependency recovers and the lease expires, the same effect is delivered |
| Same manual recovery filter, original native locator | The Reporter received native identity in place of the distinct verified display/route locator | Observation/link publication retains the original locator; the same unchanged Reporter protocol receives it on each attempt |
| `cargo test -p octacity-server-api-rest --test v1_handlers issue_source_http_` | Attachment omitted the verified locator and its schema omitted bounds | Original locator is returned under an optional 256-byte read contract; legacy links remain readable |
| Mixed Work/Issue filter, segmented process-peer mode | The peer offered only one indivisible step, so an independently owned remaining field did not converge | The controlled native peer exposes conditional per-field steps; the existing engine applies the owned group and preserves superseded/unrelated groups without certifying full delivery of the stale intent |
| `cargo test -p octacity-server --all-features --lib issue_reporter_takeover_waits -- --ignored` | A replacement claim committed while the old native permit publication was paused before commit | The shared owner-row lock serializes takeover with permit publication; the existing Issue-to-native-item lock order is preserved |
| Same upgrade filter, interrupted publication phase | SQL-only completion reported Current despite an original unpublished input | Readiness reports Pending until public migration resumes the retained input |
| `cargo test -p octacity-server --all-features --lib issue_reporter_recovery_cannot_order -- --ignored` | Resuming a missing older publication granted it a permit after a newer accepted effect | Both original inputs remain retained; only the newer effect can acquire a native permit |
| Same recovery filter, partial publication | The shared claim port handed out the newer Issue effect while retained older material was still unpublished | Issue claims return recoverable Unavailable until public migration completes the bounded publication phase |

The recovery scenario also replaces the mapping between attempts and checks the
original native item, effects and operation throughout every process request.
The mixed Work/Issue scenario exercises overlapping, independent and partially
superseded native fields, preserves a human-owned unrelated field, and leaves original Run/Flow
and Issue outcomes unchanged. Its initial due-time fixture was corrected before
qualification; no implementation change was needed for that scheduling detail.
The final concurrent run also exposed an overly tight one-second process fixture
lease. Qualification uses the existing native suite's ten-second lease, a bounded
five-second dispatch budget and an explicit thirty-second original window; the
expiry assertions were moved accordingly. Fixture timing failures are excluded
from behavioral Red evidence. Production policy bounds were not changed.
Additional qualification verifies expired Issue fences cannot begin or settle
another write, while a known late completion can release only its original
permit. Missing legacy locators produce no process call, and restarting after
the original policy window cannot renew delivery. The real GitHub Source fixture
also preserves its distinct native ID/number through the same generic receipt.

## Final verification

Regression commands use the same offline/build environment above. PostgreSQL
commands also use the explicitly configured disposable service and
`-- --ignored --test-threads=2`.

| Command | Result |
| --- | --- |
| `cargo test --offline -p octacity-server-application --lib issue_` | 107 passed |
| `cargo test --offline -p octacity-server-application --lib source_` | 143 passed |
| `cargo test --offline -p octacity-server-application --lib factory_flow_action_tests` | 21 passed |
| `cargo test --offline -p octacity-server-store --lib` | 81 passed |
| `cargo test --offline -p octacity-server-api-rest --lib --test v1_handlers` | 12 library and 135 HTTP tests passed |
| `cargo test --offline -p octacity-server --all-features --lib reporting::issues -- --ignored --test-threads=2` | 9 passed, including both recovery races and original-target replay |
| `cargo test --offline -p octacity-server-store-postgres --test issues --test migrations --test factory_discovery --test factory_retention -- --ignored --test-threads=2` | 42 Issue, 6 migration, 27 Factory and 7 retention tests passed |
| `cargo test --offline -p octacity-server --all-features --lib runtime::issue_tests -- --ignored --test-threads=2` | 2 passed: provider-independent startup and original replay after restart |

The existing native Source/Reporter regression passed 74 tests, with PostgreSQL
cases qualified separately (nine existing Reporter database tests). Five profile
tests passed, including immutable policy binding and process deadlines. Console
API generation, formatting, lint, type checking, all unit tests (33 Node and 570
Vitest tests), architecture/bundle checks and production build passed. Generated
API files remain ignored; no Console behavior changed.

Documentation-only changes have no executable TDD cycle and use specification,
formatting and diff checks. The macOS test linker emitted its existing compact
unwind-size warning; the test binaries linked and passed.

Final `cargo clippy --offline -p octacity-server -p octacity-server-application
-p octacity-server-store-postgres --all-features --all-targets -- -D warnings`
passed. `cargo fmt --all -- --check`, `python3 tools/check_architecture.py`
(63 packages, 256 edges), `openspec validate add-factory-work-card --strict` and
`git diff --check` passed. Only task 5.2 is newly marked complete; 5.3–5.4 remain
pending. The owned PostgreSQL container was removed after verification. About
11 GiB remained free; incremental compilation and debug symbols stayed disabled.
No commit was created.

## Review corrections: indexed reads and one-time recovery checks

The original-intent query now supplies the existing partial index's `desired`
predicate and uses its numeric version expression. Migration 0086 adds one
singleton recovery marker with a keyset cursor. Each batch commits its cursor
with its original outbox/order records; the final empty batch commits completion.
Concurrent migration replicas serialize on that row. New commands continue their
existing atomic publication and do not update migration progress. Readiness and
Issue/mixed claim gates read only completion, preserving fail-closed recovery.
The previous-schema and partial-phase fixtures now explicitly rehearse the new
SQL/data-phase boundary instead of treating deletion after completed migration
as an unfinished migration.

Both corrections followed Red/Green through existing public ports, with the same
offline/build settings above. The disposable service for this pass was
`octacity-issue-push-review`, using the same 512 MiB RAM / 384 MiB tmpfs bounds.

| Command | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test --offline -p octacity-server-store-postgres --test issues source_push_freezes_original -- --ignored` | Reading one original intent increased PostgreSQL's `outbox_entries.seq_scan` from 0 to 1 | The public read returns the same frozen intent with no sequential scan; the existing fencing/restart assertions also pass |
| `cargo test --offline -p octacity-server-store-postgres --test migrations completed_issue_publication_recovery -- --ignored` | Readiness tried to access deliberately locked publication history and received SQLSTATE 57014 | Readiness returns Current and the public Issue claim port returns its empty result while history remains locked |

The first test uses PostgreSQL statistics from one isolated reader connection,
with sequential plans disabled to test index eligibility independently of small
fixture cost estimates. It does not duplicate production SQL in EXPLAIN or assert
wall-clock timings. The second uses an explicit database dependency fault; its
statement timeout only bounds the missing-behavior failure.

Final affected regressions:

- `cargo test --offline -p octacity-server-store-postgres --test migrations --test issues -- --ignored --test-threads=2`: 7 migration and 42 Issue tests passed.
- `cargo test --offline -p octacity-server --all-features --lib reporting::issues -- --ignored --test-threads=2`: 9 passed, including SQL/data-phase interruption, unknown response, takeover and mixed-origin managed fields.
- `cargo test --offline -p octacity-server --all-features --lib runtime::issue_tests -- --ignored --test-threads=2`: 2 passed, preserving provider-independent manual tasks and original replay.

`cargo clippy --offline -p octacity-server -p octacity-server-application
-p octacity-server-store-postgres --all-features --all-targets -- -D warnings`,
`cargo fmt --all -- --check`, `python3 tools/check_architecture.py`,
`openspec validate add-factory-work-card --strict` and `git diff --check` passed.
The owned review PostgreSQL container was removed after the checks; about 11 GiB
remained free. No new task was marked complete and no commit was created.
