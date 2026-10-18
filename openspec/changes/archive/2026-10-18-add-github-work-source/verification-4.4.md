# Managed-field Reporter ordering — task 4.4

Completed on 2026-10-17. OpenSpec progress: 18/29 tasks. No commits, live
provider accounts, operator credentials, paid model calls or deployment changes.

## Scope and guarantees

The existing accepted action and independently fenced outbox remain authoritative.
Reporter v2 adds read-only opaque plans and conditional single-write steps. The
host requires v2, with no bulk-v1 downgrade, and revalidates the original accepted
intent, current claim and field ownership before every mutation. Source stays v1;
legacy Reporter v1 and Delivery contracts remain independently usable.

Memory and PostgreSQL assign a per-native-item sequence atomically with accepted
status publication, independent of worker arrival or clock skew. Technical state
retains immutable operation scopes, latest per-field heads/revisions and unresolved
write permits across relevant input bindings. It creates no Work identity, Flow
transition, alternate outbox or provider lifecycle. Newer unknown scopes block
older dispatch; known disjoint fields remain independent. Observation revisions
are per-field, including writes that begin and finish between the read checks.

A stable matching observation confirms delivery only under current field ownership
and with known completion of older overlapping writes. Late older effects are
reconciled by the latest intent within its original deadline, output and retry
bounds. An expired client deadline cannot cancel an already accepted remote
request: unresolved older completion becomes explicit conflict, never false
quiescence. Known late completion releases only its original permit. Each v2 step
must be a commuting subset of the original desired effects; nonconvergent native
workflow routes cannot use this contract. Remote CAS and permanent protection
from subsequent human edits are not claimed.

The GitHub adapter owns all native semantics. Each Apply makes at most one scoped
DELETE, single-label POST or state PATCH, after checking its private conditional
snapshot. Managed groups expose both native label-id and normalized name-alias
digests, preserving overlap after rename or same-name recreation. Unrelated human
labels are preserved and do not invalidate conditional tokens. Missing/ambiguous
native identities fail closed. Shared code contains no GitHub label/state enums;
GitLab/Jira/custom process peers exercise the same open-provider boundary. Manual
Factory admission requires neither provider nor Reporter installation.

Migration 0063 adds only technical order/correlation tables. Existing accepted
history is bootstrapped deterministically from retained timestamps/digests;
historical commit order cannot be reconstructed. New acceptance uses the locked
counter. No accepted history is rewritten. Journals bound heads to 4096 and
unresolved permits to 128 per item; overflow reports conflict. Wire plans bound
fields to 512, steps to 256 and individual step fields to 256, with existing
message/token ceilings. No new dependency, manifest or lockfile change was needed.

## Observed TDD

Baseline before changes: 13 Reporter/runtime-filter tests and 10 native Issue
reporting contracts passed. Tests use public protocol, verified process,
ordinary admission/action/outbox and store boundaries. Setup/compiler fixture
mistakes and empty filters were not counted as Red. Missing newly required public
APIs were counted only when existing test environments were healthy.

| Behavior / focused command | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test --locked -p octacity-work-source-protocol --test contract ordered_reporter` | v2 Inspect rejected as invalid | Closed v1/v2 commands, correlated bounded plans/steps, token Debug redaction |
| `cargo test --locked -p octacity-server-work-source --test host ordered_steps` | advertised v2 operation unsupported | Exact v2 negotiation; v1-only peer denied before exposing a handle |
| `cargo test --locked -p octacity-github --test issue_reporting ordered_native_reporting` | native v2 response invalid | Inspect, one conditional mutation per Apply, consumed token conflict; final test uses the real executable and HTTPS peer |
| `cargo test --locked -p octacity-server --lib accepted_projection_registers_opaque_fields` | public coordination request/method absent | Scope pinning, claim/authority checks, drift denial and late own-permit completion |
| `cargo test --locked -p octacity-server --lib newer_accepted_inputs_block` | older intent Ready instead of Blocked | Later unknown scopes block; overlapping fields supersede; disjoint fields survive |
| `cargo test --locked -p octacity-server --lib older_inflight_steps_require_known_completion` | initial hazard scenario falsely quiescent; corrected expiry assertion also falsely quiet | Old in-flight hazard retained, expiry explicit conflict, no renewed original window |
| `cargo test --locked -p octacity-server --lib completed_writes_invalidate` | observation-fence API absent | Begin/Finish invalidates an earlier read, preventing an ABA success |
| `cargo test --locked -p octacity-server --lib source_status_dispatch_skips` | `(retrying, delivered) == (0, 2)` instead of `(1, 1)` | Only newest overlapping intent writes, then old intent fails without changing Factory history |
| `cargo test --locked -p octacity-server --lib source_projection_order_follows_atomic_acceptance` | timestamps made the older acceptance Ready after clock reversal | Store-assigned acceptance sequence wins independently of clock order |
| `cargo test --locked -p octacity-server --lib disjoint_field_writes_leave` | global generation invalidated an independent field's observation | Per-field generations retain the independent observation fence |
| `cargo test --locked -p octacity-github --test issue_reporting managed_field_identity_overlaps` | rename produced disjoint managed identities | Native-id plus name-alias overlap survives rename and same-name recreation |
| PostgreSQL `ordered_projection_claims_and_hazards_survive_postgres_restart` (command below with that filter) | valid accepted operation returned Unavailable from the unimplemented port | Durable scopes/heads/hazards, supersession, late completion, Project isolation and unchanged Flow |
| PostgreSQL `postgres_projection_order_follows_acceptance` (same command with that filter) | later acceptance lost under reversed timestamps | Atomic per-item counter and publication under the same transaction |

Additional composed characterizations cover late old writes both with and without
claim takeover, independent fields, concurrent human edits, output/time/retry
exhaustion, worker loss and invalid observations. Barriers pause the actual old
process before its native mutation, let the newer operation apply, then release
the old write; the newer operation cannot report delivery before correcting the
late overwrite. Original Work/Run/Flow snapshots remain equal. Controlled HTTPS
peers check unrelated-label preservation and managed-value token conflicts.
A valid 128-label mapping plus Issue state exercises 257 opaque field identities.

## Final verification

```sh
cargo test --locked -p octacity-server --lib runtime
# 129 passed; PostgreSQL cases are opt-in and run separately below
cargo test --locked -p octacity-github -p octacity-server-work-source -p octacity-work-source-protocol -p octacity-delivery-protocol
# 98 passed: 74 GitHub, 8 verified host, 12 Work protocol, 4 Delivery protocol
cargo test --locked -p octacity-server-application --lib factory_flow_action_tests
# 15 passed
cargo test --locked -p octacity-server-application --lib factory_admission_tests
# 60 passed, including manual intake and provider-neutral source admission
cargo test --locked -p octacity-server-store --features test-support --lib factory_run_testing
# 20 passed, including atomic transition rejection, stale fences and replay
OCTACITY_POSTGRES_URL=postgres://postgres:reporter-fixture-only@127.0.0.1:32778/postgres cargo test --locked -p octacity-server --lib reporting::ordering::postgres -- --ignored
# 3 passed before adding the legacy bootstrap characterization
OCTACITY_POSTGRES_URL=postgres://postgres:reporter-fixture-only@127.0.0.1:32778/postgres cargo test --locked -p octacity-server --lib migration_bootstraps_retained -- --ignored
# 1 passed: real retained accepted history migrates and delivers; the next acceptance continues the counter
OCTACITY_POSTGRES_URL=postgres://postgres:reporter-fixture-only@127.0.0.1:32778/postgres cargo test --locked -p octacity-server-store-postgres --test factory_discovery expired_outbox_work_is_retried_and_terminal_settlement_replays -- --ignored
# 1 passed: expired outbox recovery, stale settlement rejection and replay
OCTACITY_POSTGRES_URL=postgres://postgres:reporter-fixture-only@127.0.0.1:32778/postgres cargo test --locked -p octacity-server-store-postgres --test factory_discovery source_migration_preserves_existing_manual_admission_and_its_replay -- --ignored
# 1 passed: manual admission and replay remain available after migrations
cargo clippy --locked -p octacity-github -p octacity-server --all-targets --all-features -- -D warnings
# Passed
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

Architecture: 63 workspace packages, 252 dependency edges. Security regressions
retain independent grants, native identity/network checks, strict bounded wire
and native observations, private conditional tokens, verified executables,
no redirects/SDK retries and secret-free failures. Documentation-only edits have
no executable behavior requiring another TDD cycle.

PostgreSQL uses only the task-owned `postgres:18.6` container
`445d88339284d4b52f206dada4b9c67e065820cdca66f6f728a9f4c0c0b606a6`, name
`octacity-reporter-4-4-contract`, label `octacity.task=reporter-4.4`, loopback port
32778. Each test created/dropped its own UUID database. After rechecking the exact
container id/name/ownership label, the container and its owned volume were removed.
No user service was changed.

## Qualification limits

Native GitHub HTTPS/process conformance and generic verified-host/outbox/PostgreSQL
journeys are tested at their respective public seams. GitLab/Jira/custom peers
are hermetic protocol implementations, not live native integrations. Stage-local
qualification remains task 4.5; management/Console activation, the full composed
source-to-downstream pilot and opt-in live account qualification retain their
separate tasks. Reconciliation conflict/exhaustion never reverses accepted Factory
history or replaces unrelated human labels.
