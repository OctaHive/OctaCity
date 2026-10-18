# Stage-local source effects — task 4.5

Completed on 2026-10-17. OpenSpec progress: 19/36 tasks after the Integrations
planning revision. No commits, live accounts, credentials, provider mutations
outside hermetic peers, paid model calls or changes to the user's stand.

## Qualified behavior

Stage effects already fit the existing public Flow/action/outbox contracts.
This task adds executable qualification rather than another stage implementation,
provider DTO in Factory, new wire field or lifecycle primitive. No production
behavior, schema, manifest or dependency was changed.

Three new application contracts exercise ordinary graph ownership, the actual
`FactoryWorkStatusAction` and its independently claimed reporting outbox:

- Declared success, escalation and typed failure routes select different arbitrary
  status presets. Exactly the selected action queues, before the next stage or
  overall Flow completion. Escalation is an arbitrary declared outcome key; a
  typed failure is an accepted configured branch, not an inferred exception route.
- A nested child queues and delivers its update before returning to the parent.
  The parent's call deliberately reuses the child's stage key. Exact definition,
  Flow Run, cycle, source-record digest/attempt, outcome, action record and parent
  call prove which execution originated the effect. The outbox's retained input
  digest identifies the accepted action rather than mutable stage display text.
- Outbox failure/restart retains the original operation and action/source records,
  respects its retry lower bound, and delivers without another accepted action.
  The enclosing graph subsequently finishes normally. Work/Run/Flow snapshots
  remain equal across reporting failure, retry and delivery.
- A declared outcome named `approved` routes directly onward without a source
  action. Neither its stage nor terminal completion queues a report or calls the
  projector. Outcome spelling has no implicit effect.

One new native process/HTTPS contract independently qualifies the same three
arbitrary values in a single pinned mapping: removal plus addition, removal-only,
and removal/addition plus Issue closure. Each Apply performs one native mutation;
all preserve `human-owned`, and a fresh process confirms the result without
another mutation. The graph and native contracts meet at the existing arbitrary
status/pinned-mapping boundary. Full source-to-downstream composition remains 6.1.

The graph/application contracts contain no GitHub SDK or label semantics. Existing
verified-host journeys additionally pass for GitHub/GitLab/Jira/custom identities,
including manual Factory admission during a Reporter outage. These common-protocol
peers do not claim live native GitLab/Jira support. Integration administration,
credential management and Console authoring remain pending section-5 tasks.

## TDD and characterization evidence

Before writing the new graph contracts:

```sh
cargo test --locked -p octacity-server-application --lib factory_flow_action_tests
# Green baseline: 15 passed
```

The new valid stage-route, nested retry/provenance, no-action and native preset
contracts were Green against the existing production implementation. They are
characterization of previously implemented behavior, not new observable behavior
implemented before its test. No missing feature was found, so no artificial
production Red or replacement implementation was introduced. Temporary fixture
compile errors, incomplete mapping JSON and a too-small subflow budget were
corrected as test setup and are not counted as behavioral Red.

A broader Reporter regression did expose a test expectation that confused the
per-dispatch timeout with terminal exhaustion:

| Command | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test --locked -p octacity-server --lib reporting` | Continuous human edits produced `failed == 0` instead of the test's immediate `1`: a loaded host exhausted its finite operation timeout before its output budget and legitimately scheduled the already configured retry | The contract requires no delivery, honors the 100ms retry lower bound if needed, and requires terminal failure within the original two-attempt policy. It retains unchanged Work/Run/Flow and unrelated fields and forbids another terminal dispatch |

The corrected regression also includes an explicit barrier after each native write
while a human edit undoes its managed value. This deterministically loses Apply
confirmation and exercises the retry path, with two writes/two attempts followed
by terminal failure. No retry/deadline policy or production implementation was
changed to accommodate the test. Valid existing behavior was kept passing before
extracting shared graph fixture helpers.

## Final verification

```sh
cargo test --locked -p octacity-server-application --lib factory_flow_action_tests
# 18 passed, including the three new stage-local contracts
cargo test --locked -p octacity-server-application --lib factory_flow_tests
# 10 passed, including nested ownership, frozen predecessor inputs and bounded cycles
cargo test --locked -p octacity-server-application --lib stage_reporting
# 3 passed again after fixture extraction and lexical lock-scope cleanup
cargo test --locked -p octacity-server --lib reporting
# 23 passed; 4 PostgreSQL contracts remain opt-in (qualified in task 4.4)
cargo test --locked -p octacity-github -p octacity-server-work-source -p octacity-work-source-protocol -p octacity-delivery-protocol
# 99 passed: 75 GitHub, 8 verified host, 12 Work protocol, 4 Delivery protocol
cargo clippy --locked -p octacity-github -p octacity-server-application -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

Architecture remains 63 workspace packages and 252 dependency edges. No new store
or migration behavior requires PostgreSQL qualification in this slice. The retained
4.4 durable-ordering evidence remains applicable. Documentation-only updates have
no executable behavior requiring an additional TDD cycle.
