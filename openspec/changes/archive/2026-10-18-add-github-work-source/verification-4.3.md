# Uncertain and partial Reporter writes — task 4.3

Completed on 2026-10-17. OpenSpec progress: 17/29 tasks. No commits, live
provider accounts, operator credentials, paid model calls or deployment changes.

## Scope and behavior

Recovery reuses the existing independently fenced status outbox and original
accepted action. No new lifecycle, deduplication authority, protocol message,
store schema or dependency was added. The shared runtime retains opaque provider
identities and arbitrary statuses; native Issue effects remain inside GitHub.
Manual Factory intake still requires neither Source nor Reporter installation.

Three production boundaries now retain uncertainty rather than terminally lose
confirmation of potentially applied effects:

- A successful native mutation with an empty, malformed or oversized body is
  transient. Disconnects and truncated bodies remain transient. Body limits are
  still enforced before decoding; Source/Observe reads retain strict validation.
- A malformed or incomplete final native read-back after Project is transient.
  Scope/identity conflicts and denied authority retain their closed failures.
- A missing or invalid Project process response is unavailable to the outbox.
  An invalid Observe response cannot authorize Project. No invalid process
  response is accepted as an applied receipt.

All retries retain the original operation, mapping, native resource/item, profile,
permissions, action deadline and finite retry policy. A retry observes before
requesting missing effects. Already completed label removals/additions and state
updates are skipped. Unrelated labels are retained. Exhaustion records failed
reporting without compensating external effects or changing accepted Factory data.

## Observed TDD

Tests use the existing public provider protocol and ordinary
admission/action/outbox/projector boundaries. Each behavior correction was driven
by its own failing behavioral assertion before implementation; passing existing
behavior was characterized without manufacturing a Red failure.

| Cycle | Command | Observed Red | Observed Green |
| --- | --- | --- | --- |
| Lost successful mutation body | `cargo test --locked -p octacity-github --test issue_reporting lost_success_body` | `ProtocolFault` instead of `Transient` after the peer applied POST | Fresh provider observes matching effects, one POST only |
| Lost Project process response | `cargo test --locked -p octacity-server --lib lost_project_response_is_recovered` | `retrying == 0` instead of `1`; the effect was applied but its malformed acknowledgement exhausted reporting | Observe/Project/Observe across worker restart; no second Project for GitHub/GitLab/Jira/custom identities |
| Lost final confirmation | `cargo test --locked -p octacity-github --test issue_reporting lost_final_confirmation` | `ProtocolFault` instead of `Transient` after DELETE/POST/PATCH had all applied | Read-only recovery confirms the same effects without any additional write |
| Oversized successful mutation body | First command above, extended after its initial Green | `ProtocolFault` instead of `Transient` for an oversized declared body | Both oversized declared and chunked bodies remain bounded and recover by observation |

Before changes, all 6 native Reporter contracts and all 7 hosted Reporter journeys
passed. The focused commands above passed after their corresponding changes.
Fixture build/layout failures and empty test-filter matches were not counted as Red.

## Controlled boundary evidence

Four new native recovery tests use a private-CA HTTPS peer with retained mutable
Issue state. The peer applies actual requested mutations before dropping or
corrupting the response, rather than supplying a predetermined successful final
observation. It rejects duplicate completed mutations and unexpected write paths
or bodies. Coverage includes:

- Empty/malformed success bodies, abrupt TLS disconnect, truncated body and
  oversized declared/chunked bodies after apply.
- Each of DELETE, POST and PATCH losing its response after apply, receiving 503
  before apply, and receiving 503 after apply. Intermediate external state is
  checked before read-only observation and convergence of only missing effects.
- Loss of final confirmation after all three mutations; restart of the real native
  executable after partial HTTPS effects, preserving operation/mapping/status.
- Preservation of `human-owned` alongside the exact configured effects.

Five new hosted recovery journeys use the content-verified process, ordinary
Factory admission and accepted action, the authoritative in-memory store and the
existing outbox worker. They cover response loss across all four open provider
identities, invalid observation, exhausted confirmation, claim takeover while
observation is in flight, takeover after apply, and abrupt worker termination
before settlement. Barriers control the in-flight processes. New workers use fresh
projectors/registries; a stale owner cannot write after observation or settle a
replacement claim. Captured protocol intents remain identical across ownership
changes. Whole Work/Run/Flow snapshots and Run counts prove no rollback, new
transition, repeated accepted action or second Run comes from reporting results.

The existing PostgreSQL contract additionally verifies expired-claim recovery
under the same operation with incremented attempt, rejection of stale settlement
and exact terminal settlement replay. Native HTTPS recovery, hosted outbox
recovery and durable store fencing are tested at their respective public seams;
the full composed source-to-downstream pilot remains task 6.1.

## Regression commands

```sh
cargo test --locked -p octacity-server --lib runtime
# 119 passed, including 12 hosted Reporter journeys and manual intake during outage
cargo test --locked -p octacity-server-application --lib factory_flow_action_tests
# 15 passed
cargo test --locked -p octacity-server-application --lib factory_admission_tests
# 60 passed
cargo test --locked -p octacity-github -p octacity-server-work-source -p octacity-work-source-protocol -p octacity-delivery-protocol
# 92 passed: 70 GitHub, 7 verified host, 11 Work protocol, 4 Delivery protocol
OCTACITY_POSTGRES_URL=postgres://postgres:reporter-fixture-only@127.0.0.1:32777/postgres cargo test --locked -p octacity-server-store-postgres --test factory_discovery expired_outbox_work_is_retried_and_terminal_settlement_replays -- --ignored
# 1 passed
cargo clippy --locked -p octacity-github -p octacity-server --all-targets --all-features -- -D warnings
# Passed
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

Architecture: 63 workspace packages, 252 dependency edges. Manifest/lockfile
changes: none. Relevant security regressions retain independent grants, native
scope checks, private material, verified executables, strict observations, body
bounds, no redirects/SDK retries and secret-free failures. Documentation-only
changes have no executable behavior requiring a separate TDD cycle.

PostgreSQL used only the task-owned `postgres:18.6` container
`2983553d54c772320e6cf49cbadee52a8e6da682e027711eb346f72dafc865e3`, name
`octacity-reporter-4-3-contract`, ownership label `octacity.task=reporter-4.3`,
loopback port 32777. The contract created/dropped its own database. After inspecting
the exact container id/name/label, the container and its owned volume were removed.
No user service was changed.

## Remaining scope

Multi-request provider writes remain non-atomic. Cross-input managed-field
ordering, superseded intents and reconciliation of late older writes are task
4.4. Stage-local effect qualification/authoring, management/Console activation,
the full composed pilot and opt-in live account qualification retain their
separate tasks. GitLab/Jira/custom journeys are hermetic common-protocol peers,
not claims of live native integration with those services.
