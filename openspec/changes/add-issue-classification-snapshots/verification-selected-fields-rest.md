# Selected-field REST contract verification (task 10.6)

## Scope

The existing schema validation query accepts either an authored `shape` or a
canonical `issue_fields` selection. The same `IssueFieldForm` drives the generated
result and the admitted save scope. No mapping endpoint, second form schema,
caller provenance, or compatibility translation is introduced.

Authoring discovery advertises supported field identities, server value keys,
Project scalar kinds, current choices and labels, bounds, catalog version and
required-field availability. Retired properties are omitted. Read-only sources
have no write target. In particular, dates remain distinguishable from text.

Node-completion diagnostics expose safe form/operation/receipt identities and a
separate application status. Accepting an analysis does not imply that its Issue
save succeeded. Values remain private unless the existing public-result policy
explicitly publishes them. The projection only reads the existing effect journal.

## Observed TDD

All Cargo commands used `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0`. PostgreSQL tests used a disposable PostgreSQL 18
container, not the user's stand.

| Public boundary / command | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test --locked -p octacity-server-api-rest --test v1_handlers selected_issue_field -- --nocapture` | Selected fields returned HTTP 400 instead of a generated schema; a second `shape: null` was subsequently accepted when it should have been rejected. Discovery lacked write targets and could not distinguish dates. | Generated priority/optional size schema, closed exclusive source, foreign/stale/empty/forged targets rejected, unauthorized request denied, exact scalar types/keys/labels and `no-store` verified. |
| `OCTACITY_POSTGRES_URL=… cargo test --locked -p octacity-server --test factory_configured_flow ordinary_form_captures_invocation -- --ignored --nocapture` | Accepted analysis lacked the independent pending application projection. | Real persisted accepted analysis projects pending, trusted writer commits the save, diagnostics then project applied with the same operation and exact receipt; reads do not change the journal or reveal private results. |
| `OCTACITY_POSTGRES_URL=… cargo test --locked -p octacity-server --test factory_project_policy_http selected_issue_fields_http -- --ignored --nocapture` | Real HTTP publication succeeded, but the published request failed the OpenAPI contract because the selected form was undocumented. | Normalized form publishes through the real server/PostgreSQL owners. After a catalog change and server restart, the original idempotency key returns the original resource/form. |
| `cargo test --locked -p octacity-server-api-rest --test v1_handlers preset_resolution_rechecks_project_visibility_after_metadata_reads` | A revoked Project grant produced a partial HTTP 200 metadata response. | Current catalog authorization failure propagates; the existing visibility regression passes. |
| `pnpm exec vitest run src/features/factory/IssueClassificationEditing.test.tsx src/features/factory/IssueSnapshotEditing.test.tsx` (from `ui`) | Baseline: 15 passed. With the canonical server fixture, four editor assertions failed because old slot-based selections hid direct Project fields. | Existing consumers now bind canonical field identities and requiredness. The expanded five-file editor/publication/example selection passes all 63 tests. |
| `pnpm exec vitest run src/features/factory/FactoryAuthoring.test.tsx` (from `ui`) | Current threshold metadata hid the Size control and permitted resolution without its required value. | The existing Size control uses the advertised condition ID; a renamed `specification_cutoff` is sent in `choice_thresholds`, without a hardcoded node name. |

Fixture compilation/setup errors were corrected before recording behavioral Red;
they are not counted as TDD evidence. Tests for older example composition,
Project defaults, provenance and the generated-client request union were updated
to the current contract without legacy-format fallback.

## Regression checks

- REST handler suite: 294 passed.
- Application schema normalization: 2 passed.
- PostgreSQL ordinary-form regressions: 7 passed sequentially (including conflict,
  denial, lost acknowledgement, consumed documents and transitive Context).
  The initial concurrent run had one `Store(Unavailable)` in the existing
  consumed-document test; that test passed independently and the complete seven
  passed with `--test-threads=1`. No retry or failure suppression was added.
- UI unit suite: 47 script checks and 1,264 Vitest tests passed.
- Browser `factory-canvas.spec.ts` and `factory-ticket-delivery.spec.ts`: 20 passed
  (EN/RU, keyboard, narrow layout, existing reviewed publication/start behavior).
- API client/authoring constraints regenerated with `pnpm api:generate`.
- TypeScript, ESLint and Prettier passed.
- Rust formatting, Clippy (`--lib` plus REST handlers/configured Flow/HTTP policy
  integration targets, `-D warnings`), architecture and strict OpenSpec validation
  passed. `git diff --check` passed.

## Limits

Task 10.7 supplies the new ordinary-agent **Fill Issue fields** controls and
removes the old manual apply/setup UI. Task 10.8 completes generic template and
snapshot/condition editing. This task only adapts existing client consumers to
the new REST contract. These tests do not claim a live-provider/native UI/PR
qualification run; that remains task 10.10. Documentation-only edits require no
executable TDD.

## Review corrections for tasks 10.5–10.6

The stand retains the configured structural Flow limits after using the common
resolver. The selected execution policy still supplies the exact budget and
permissions. No second schema compiler or validation owner is introduced.
Discovery now describes Project Context as optional, matching the resolver;
Role/Profile selections remain required.

- `python3 -m unittest discover -s tools/tests -p test_factory_stand_flow.py -k reviewed_limits`:
  observed Red with broader resolver limits replacing the reviewed stand policy.
  Green restores a deliberately different configured limit set and the selected
  budget/permissions, without mutating the resolver's response. The complete file
  passes all 4 tests.
- `cargo test --locked -p octacity-server-api-rest --test v1_handlers preset_discovery_and_resolution_agree -- --nocapture`:
  observed Red after successful resolution without Context, because discovery
  incorrectly advertised it with `required: true`. Green advertises optional
  Context, resolves its omission without placeholders and retains required
  Profile selections, with no publication or execution mutations.

Affected regression checks: installer draft (4), triage setup (15), policy
ceilings (6), and restart/recovery (17) passed. The opt-in real-Server setup file
reported 2 environment skips; no native/model qualification was attempted.
The complete REST handler suite passed all 295 tests after updating the previous
discovery expectation to optional Context.
`cargo clippy --locked -p octacity-server-application -p octacity-server-api-rest
--lib --test v1_handlers -- -D warnings`, `cargo fmt --all -- --check`, the
architecture check, strict OpenSpec validation and `git diff --check` passed.

These are behavioral regressions through the existing installer draft boundary
and the public HTTP discovery/resolution boundary. Documentation-only edits have
no executable behavior and are checked with the normal hygiene/validation checks.
