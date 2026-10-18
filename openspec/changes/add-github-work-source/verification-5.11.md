# Task 5.11 verification

Task 5.11 is complete. No commit was requested or created.

## Scope and public boundaries

The guided **Pipeline inputs** section is attached to the inspected published
Factory Configuration. It reuses the existing manual Artifact intake, selection
menus and confirmed-command/idempotency mechanism. Automatic inputs select a
visible connection and authorized task container independently of the code
Repository. No credential form or internal-Issue creation API is added.

The user explicitly approved the backend/runtime extension needed for editable
state and all/any/excluded label filters. An optional immutable normalized
`WorkSourceInputDraft.predicate` narrows an explicitly unrestricted installed
Source profile. Only its trusted owner can advertise editing/state choices through
`with_predicate_states`; legacy installed-only selectors remain unchanged. The
shared evaluator runs in hosted preview and before exclusive selection/admission.
There is no new profile table, migration, provider protocol or Flow dependency.
Absent predicates retain the old serialized shape. Repository REST responses now
return the server's canonical definition digest for exact input references.

Tests use the existing public Factory model, authorized application commands and
queries, REST router/OpenAPI, worker/hosted-preview and rendered UI boundaries.
Memory and disposable PostgreSQL run the same management storage contract.
Provider-shaped fixtures cover GitHub, GitLab, Jira and an open custom identity;
this does not claim native GitLab/Jira adapters were implemented.

## Observed Red/Green

All Cargo commands below used `--locked` with bounded build settings:

```sh
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2
```

- `cargo test --locked -p octacity-server-factory --lib input_predicate --quiet`:
  the immutable predicate contract was absent; Green retains literal labels,
  evaluates normalized state/label data, permits unrestricted unlabeled tasks and
  rejects repeated/contradictory predicates.
- `cargo test --locked -p octacity-server-application --lib input_predicates --quiet`:
  Red accepted a predicate without explicit installed editing support. Green
  rejects it. The companion `advertised_normalized_predicates...` test qualifies
  immutable publication and unadvertised-state rejection for open provider names.
- `cargo test --locked -p octacity-server --lib hosted_preview_applies --quiet`:
  Red returned no mismatch for a missing required label; Green returns
  `RequiredLabelMissing` without Work admission, Runs or Artifact-byte writes.
- `cargo test --locked -p octacity-server --lib source_worker_applies_retained --quiet`:
  Red created one Run for a nonmatching task instead of zero. Green filters it
  before selection/admission, then admits a matching subsequent input version.
- `cargo test --locked -p octacity-server-application --lib preview_presents_the_effective --quiet`:
  Red presented the installed unrestricted base instead of the custom predicate;
  Green preview/diagnostics use the effective immutable input predicate.
- `cargo test --locked -p octacity-server-api-rest --test v1_handlers repository_publication_returns_the_authoritative --quiet`:
  Red returned a missing digest; Green returns the independently specified literal
  canonical Repository digest. The documented response contract includes the field.
- `cargo test --locked -p octacity-server-api-rest --test v1_handlers input_schema_advertises --quiet`:
  Red lacked the optional predicate, editing advertisement and authoritative
  Repository digest schema; Green publishes provider-neutral state contracts and
  the shared label count/UTF-8 ceilings.
- `pnpm --dir ui exec vitest run src/features/factory/sourceApi.test.ts`:
  Red lacked the transport contract. Green uses generated types, scoped discovery,
  pure validation/preview and separate initial/replacement/group preconditions.
- `pnpm --dir ui exec vitest run src/features/factory/FactoryPipelineInputs.test.tsx`:
  Red lacked the input section and missing-setup guidance; Green reuses manual
  intake without integration discovery and links automatic setup to Integrations.
- `pnpm --dir ui exec vitest run src/features/factory/SourceInputEditor.test.tsx`:
  observed Reds included missing input/group controls; publication enabled after
  installed filter-editing support was withdrawn; stale refresh requesting the old
  connection version; two distinct installed profile identities selecting the first
  shared digest; raw JSON review; an unexplained unrestricted state; and raw
  machine mismatch names. Green covers reviewed input/group publication, exact
  frozen member references, reuse without credentials, retained filters, refreshed
  access/version preconditions, fresh validation, translated explanations and
  discarded transient drafts/previews. A further Red reproduced disappearing
  historical group members after a conflicting refresh; Green keeps the exact
  selected versions visible until the operator changes them. Nine rendered-UI
  tests passed in the initial qualification; post-review coverage is recorded below.
- `node --test --test-name-pattern 'guided pipeline inputs' ui/scripts/verify-architecture.test.mjs`:
  Red rejects the measured new output under the previous total budgets. Green
  admits the measured optional feature and still rejects an extra 2 KB payload.

Test-authoring import/type errors, an incorrect pre-existing manual Artifact
control label, and browser fixture route/translation mistakes were corrected;
they are not claimed as production Red/Green evidence. No zero-test filtered run
is counted. Additional integration/acceptance checks qualify the implemented
behavior rather than being represented as new implementation-driving Reds.

## Regression and repository checks

```sh
cargo test --locked -p octacity-server-factory -p octacity-server-store --lib --quiet
# 294 Factory + 81 store tests passed.
cargo test --locked -p octacity-server-application --lib source_configuration --quiet
# 20 passed.
cargo test --locked -p octacity-server-api-rest --tests --quiet
# 12 library + 17 administration + 25 public-error + 77 handler tests passed.
cargo test --locked -p octacity-server --lib work_source --quiet
# 62 passed; 9 existing opt-in tests ignored.
cargo test --locked -p octacity-server-store-postgres --test factory_discovery managed_source_configuration_publication -- --ignored --quiet
# 1 shared memory/PostgreSQL 18 contract passed, including retained custom predicates.
pnpm --dir ui api:generate
pnpm --dir ui test:unit
# 27 script checks + 434 Vitest tests across 69 files passed.
pnpm --dir ui test:browser
# 79 passed, including keyboard input review, English/Russian and narrow width.
pnpm --dir ui build
# Entry 423940 bytes; JS 910727; CSS 81069; static 992342; 42 files.
```

Total JS/static ceilings are now 911360/993280 bytes. Entry 460800, CSS 81920 and
42-file ceilings remain unchanged. The Factory module stays in the optional
Factory chunk; no dependency or product asset was added.

The full application suite also passed: `cargo test --locked -p
octacity-server-application --lib --quiet` — 468 tests. The following checks passed:

```sh
pnpm --dir ui lint
pnpm --dir ui format
cargo fmt --all -- --check
python3 tools/check_architecture.py
python3 tools/security_matrix.py validate
python3 tools/verify_local_stand_context.py
openspec validate add-github-work-source --strict
openspec validate add-factory-work-card --strict
git diff --check
```

All-target/all-feature Clippy passed without warnings:

```sh
cargo clippy --locked -p octacity-server-factory -p octacity-server-store -p octacity-server-application -p octacity-server-store-postgres -p octacity-server-api-rest -p octacity-server --all-targets --all-features -- -D warnings
```

## Resource and composition limits

Builds remained bounded to two Cargo jobs without incremental/debug output and
were run one at a time. About 16 GiB remained free; `target` was about 7 GiB.
Only the owned disposable `octacity-511-pg` container was started and then stopped;
its `--rm` lifecycle removes its anonymous data. User caches/containers were not
cleaned. No live provider/model request, deployment or authentication change was
performed.

Existing opt-in protected integration/Source services, trusted declarations and
runtime/preview composition are still required. The browser cannot install an
adapter or grant authority. Missing capabilities remain explicit, and unadvertised
filters are read-only. Internal cards, import/link refresh, manual card execution
and synchronization ownership/conflicts remain in `add-factory-work-card`;
guided status mapping/progress remain task 5.12. Documentation-only changes are
validated by OpenSpec and formatting/whitespace checks; executable TDD does not
apply to the prose itself.


## Post-review corrections

The three reported UI defects were corrected through separate public-boundary
Red/Green slices. Task 5.11 remains complete; task 5.12 and the card change were
not expanded. These corrections change only the two input/group editors, their
regression tests and English/Russian presentation. No commit was created.

### Retained Reporter scope

```sh
pnpm --dir ui exec vitest run src/features/factory/SourceInputEditor.test.tsx -t 'removes the old Reporter'
```

Red: all three cases (changed container, installation version/digest and reusable
connection) sent the previous incompatible Reporter. Green: the new draft omits
both the Reporter and its parent configuration, explains the effect and shows
`Reporting: none` before explicit confirmation. Original immutable inputs are
unchanged. A separate passing guard preserves both references when only a Source
mapping changes within the same scope. Immutable-reference comparison also serves
binding selection instead of duplicating comparison logic.

### First-publication conflict recovery

```sh
pnpm --dir ui exec vitest run src/features/factory/SourceInputEditor.test.tsx -t 'refreshes a new input'
pnpm --dir ui exec vitest run src/features/factory/SourceInputEditor.test.tsx -t 'refreshes a new group'
```

Red: after a precondition conflict, Refresh required an entity whose first
publication had not succeeded, permanently disabling validation. Green: an
unpublished draft can refresh dependencies without that entity. Input identity,
filters and `expected_version: null` survive. Fresh validation/review are required
and confirmation uses a new idempotency key. The corresponding new-group path
uses the same distinction, retains selected versions and lets the operator
explicitly choose renewed members. A missing current record still blocks refresh
of an already published version; this is not a generic error bypass.

### Selected group snapshots

```sh
pnpm --dir ui exec vitest run src/features/factory/SourceInputEditor.test.tsx -t 'keeps every selected group'
```

Red: adding C v1 to A/B and refreshing after C advanced to v2 removed C v1 from
the checkbox list while retaining it in the request. Green: all selected immutable
snapshots are held in one state value, including choices added after opening.
They remain visible and removable; review reads the same snapshots and the wire
intent projects only their id/version references. This removes the separate
initial-group snapshot and the fallible metadata lookup, without another cache,
state manager or generic editor abstraction.

### Final UI qualification

```sh
pnpm --dir ui exec vitest run src/features/factory/SourceInputEditor.test.tsx
# 15 passed.
pnpm --dir ui test:unit
# 27 script checks + 440 Vitest tests across 69 files passed.
pnpm --dir ui test:browser
# 79 passed, including manual intake without integration setup, keyboard review,
# English/Russian and narrow layouts.
pnpm --dir ui build
# Passed existing budgets: 911256 JS, 81069 CSS, 992871 static bytes, 42 files.
pnpm --dir ui lint
pnpm --dir ui format
python3 tools/check_architecture.py
python3 tools/security_matrix.py validate
openspec validate add-github-work-source --strict
openspec validate add-factory-work-card --strict
git diff --check
# Passed.
```

Entry/CSS/file-count and total bundle budgets, dependencies, deadlines and test
parallelism were not increased. The corrections introduce no provider vocabulary,
credentials, Issue creation/execution API, backend/runtime change or migration.
Existing server validation remains authoritative. The shared manual Artifact
path and future independent card ownership are preserved.

Only UI compilation was required for this correction. Free space remained about
16 GiB and the existing Rust target directory remained about 7 GiB. No Rust build,
new database container, cache cleanup, external mutation or paid model call was
needed. Documentation has no executable behavior; whitespace and OpenSpec checks
validate the prose; all final formatting, architecture, security and spec checks
listed above passed.
