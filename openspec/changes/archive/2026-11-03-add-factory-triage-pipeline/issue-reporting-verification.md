# Task 6.2: independent canonical Issue and Source reporting

## Supported execution boundaries

Source-only triage uses its original Work-backed Reporter without creating a card.
Canonical Issue import/attachment is independently enabled and supplies neither a
reviewed Issue execution nor local-transition authority. Issue-associated local
execution uses the existing reviewed Issue request, exact Work/Run association,
requirements applicability and pinned workflow/version/edge. Existing guarded
transition and field-ownership checks remain authoritative.

Automatic Source intake still rejects an enabled target containing
`transition_issue`: that intake cannot supply the reviewed association. Disabled
repair drafts remain valid. Missing or legacy association cannot authorize a
local move or silently adopt a card/workflow. No automatic association, imported
acceptance fallback, admission lifecycle or new journal was introduced.

The reachable missing producer check was Source-only Work reporting against an
independently linked canonical Issue with active push. The ordinary Hosted
Reporter now checks this policy before any provider I/O and in its existing
per-write material recheck. It reuses `validate_issue_push_producers` for every
configured Issue push choice. Overlapping labels/state are refused; an opaque
legacy Work mapping cannot establish disjoint ownership. Disabled or disjoint
Issue push permits the unchanged original Work effect.

The private `IssueSourceProjectionStore::work_source_issue_push_mapping` read
accepts Project and Work identity. PostgreSQL derives native correlation from the
settled preparation and original admitted Work security-partition digest, then
uses the existing canonical link and immutable mapping reader. It never accepts a
caller-selected card or scope and performs no provider, credential, import or
mutation operation. Missing/ambiguous preparation and unsupported read backends
fail closed. The Factory-only in-memory fixture has no Issue namespace and
explicitly reports no canonical policy.

Migration 106 adds lookup indexes for settled `(project_id, work_id)` preparations
and `(project_id, native_correlation)` links. Existing schema/records and journal
ownership remain unchanged. The implementation resolves Work effects once for
both the original intent and producer review; it does not add another Flow
interpreter. Gates/subflows cannot bypass the unconditional Reporter check.

## TDD observations

Cargo invocations used:

```sh
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_INCREMENTAL=0
```

`OCTACITY_POSTGRES_URL` pointed only to an owned temporary PostgreSQL 18.6 container
using an already installed image, a loopback dynamic port and disposable test
credentials. Every PostgreSQL test creates its own database.

1. **Producer Red/Green**

   ```sh
   cargo test --locked -j2 -p octacity-server --lib \
     work_reporting_rejects_overlapping_canonical_issue_push -- --ignored
   ```

   Red: the ordinary Source admission/action/outbox/verified Reporter process
   delivered one effect while the canonical Issue's push owned the same label
   (`delivered: 1`, expected `0`). Green: the existing conflict guard is applied
   through the new private read; delivery fails before Inspect/Apply and original
   Work/accepted Flow/Issue remain unchanged. The retained test also qualifies
   rejection of opaque legacy Work mapping while Issue push is active.

2. **Migration/index Red/Green**

   ```sh
   cargo test --locked -j2 -p octacity-server-store-postgres --test migrations \
     source_work_producer_review_has_bounded_native_lookup_indexes -- --ignored
   ```

   Red: the migrated public schema lacked an index for the Work preparation lookup.
   Green: migration 106 supplies both independently specified lookup prefixes. This
   is a schema contract test, not a claimed latency benchmark.

Existing-behavior qualifications do not claim additional new Red/Green cycles:

- `work_reporting_preserves_source_target_without_adopting_issue_authority`
  exercises disabled/disjoint Issue push and a different security partition after
  Work action acceptance. It checks original native ticket/literal effects,
  unchanged Work/Flow/Issue and no invented Issue Work association.
- `issue_status_action_without_reviewed_association_refuses_a_move_or_push`
  exercises the actual action/node owner: absent reviewed association returns
  `capability_unavailable`, accepts no business result and publishes no Issue move
  or push. An initial expectation of a retained `ExecutionStopped` receipt was
  corrected to the existing error contract; no production behavior was changed.
- The existing `issue_pipeline_action` tests cover exact association, requirements,
  workflow/adoption/version, current authority, ownership, evidence and replay
  guards. Existing Source activation and configured action tests qualify the
  separate execution paths and original retained push review.
- The existing mixed native-ordering test initially failed because it selected an
  opaque Work producer and active Issue push simultaneously. Its historical
  delivery case now explicitly disables new Issue push after retaining the old
  intent, asserts that intent/receipt/time remain identical, and keeps all prior
  native ordering/supersession/unrelated-field assertions. Active conflicting
  producers remain covered by the separate negative test.

Incidental test setup/compilation corrections were not treated as behavioral Red.
Scope-aware Issue fixture publication reuses the original fixture and ordinary
store mutation contracts; it does not bypass production write grants.

## Review and qualification limits

Separate Standards and Spec reviews confirmed the existing reviewed-association
boundary and found the Source-only producer gap. The Standards review identified
lookup index requirements and duplicate effect selection; both were addressed.
The Spec review confirmed that the unconditional material check covers nested
reporting without another graph traversal for the explicitly unsupported
Source/local-transition composition.

These are hermetic contract/owner and verified-process qualifications. They do
not establish a packaged two-model triage journey, live GitHub writes or general
Agent/MCP installation readiness. Extended recovery remains 6.3; management and
packaged qualification remain 7.x/8.x. Documentation changes have no executable
behavior and use formatting/specification validation rather than fabricated TDD.

## Final verification

With the Cargo environment above, the following commands passed:

- `cargo test --locked -j2 -p octacity-server --lib reporting:: -- --include-ignored --test-threads=2`:
  **77 passed**, including all ordinarily ignored disposable PostgreSQL reporting
  cases, the new finite/opaque producer conflict and isolation cases, historical
  mixed ordering, normal management/installed Reporter paths and verified process
  delivery. No ignored tests were counted as executed without enabling them.
- `cargo test --locked -j2 -p octacity-server-store-postgres --test migrations -- --ignored`:
  **8 passed**, including fresh/reentrant migration, upgrade/rollback rehearsal,
  bounded document contracts, no parallel stage journal and the new index contract.
- `cargo test --locked -j2 -p octacity-server-store --lib --quiet`:
  **109 passed**.
- `cargo test --locked -j2 -p octacity-server-application --lib issue_pipeline_action --quiet`:
  **16 passed**.
- `cargo test --locked -j2 -p octacity-server-application --lib issue_source_ --quiet`:
  **37 passed**, including independent native import/read/refresh boundaries.
- Application filters `configured_issue_action_retains_exact_push_review`,
  `source_input_cannot_enable_a_pipeline_requiring_an_issue_association`, and
  `issue_status_action_without_reviewed_association` each passed **1 test** using
  `cargo test --locked -j2 -p octacity-server-application --lib FILTER --quiet`.
- `cargo clippy --locked -j2 -p octacity-server-store -p octacity-server-store-postgres -p octacity-server --tests -- -D clippy::all`
  and the same check for `-p octacity-server-application`: passed. The pre-existing
  unused `pending_factory_profile_fixture` compiler warning remains. Native test
  linking emitted the existing macOS compact-unwind size warning; tests succeeded.
- `python3 tools/check_architecture.py`: **64 packages / 263 edges**, passed.
- `cargo fmt --all -- --check`, scoped `pnpm exec prettier --check`,
  `openspec validate add-factory-triage-pipeline --strict` and `git diff --check`:
  passed.

Final repeat Standards and Spec reviews found no remaining actionable findings.
The owned PostgreSQL container and its disposable storage were removed after the
checks. Disk was monitored with `df -h .`: **51 GiB before 6.2**, **49–50 GiB during
validation**, **50 GiB after cleanup**. Existing build caches were reused, Cargo
incremental/debug information remained disabled, and compilation used two jobs.
No user data was removed. No commit was created and task 6.3 was not started.
