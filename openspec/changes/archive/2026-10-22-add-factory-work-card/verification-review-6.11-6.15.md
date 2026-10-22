# Review corrections for tasks 6.11–6.15

The three reported defects are corrected through existing public readers and
Console contracts. This follow-up creates no new execution owner, provider
client, store, HTTP endpoint, migration, dependency or browser persistence.

## 1. Both trusted candidate-publication forms are selectable

The common Factory diagnostic reader has an additive `candidates` collection.
One typed `CandidatePublication` family selects `Candidate` and `ChangeSet`
records through the same memory/PostgreSQL diagnostic projection, ordering,
bounds and cursors. The selected discovery categories are declared once and
shared by both store adapters. SQL reuses the existing query with a bound array;
there is no second query/pagination implementation. The legacy `changesets`
collection retains its ChangeSet-only meaning.

Issue Results reuses that collection under its existing visibility, exact
Work/Run and Run-version fence. The Console selects an original publication
record from this page and calls the existing Checks contract. Existing generated
OpenAPI types expose the additive collection; no arbitrary candidate digest or
provider fallback is introduced.

Red command:

```sh
cargo test --locked -p octacity-server-application --lib issue_candidate_choices
```

`issue_candidate_choices_include_capture_and_changeset_publications_without_changing_legacy_reads`
failed because `candidates` was an unknown collection. After implementation it
passes for both retained publication forms, opens Checks for the exact record,
preserves legacy reads and leaves the original Issue and Run unchanged.

The PostgreSQL parity assertions were written before implementation in the
existing capture and ChangeSet discovery/restart cases. They verify original
fact/subject identity, the one-item page and its exhausted continuation after
reconstructing the store.

```sh
OCTACITY_POSTGRES_URL=<owned-disposable-database> cargo test --locked \
  -p octacity-server-store-postgres --test factory_discovery configured_ -- --ignored
```

Both cases passed against the task-owned PostgreSQL 18.6 instance. The Console
Red command was:

```sh
pnpm --dir ui exec vitest run src/features/issues/IssueResultPanels.test.tsx -t 'selects a retained'
```

Both Candidate and ChangeSet cases failed to show a choice when only the actual
`candidates` collection returned a publication; both pass after using that
collection. Four generated-client browser journeys now choose actual publication
forms rather than bypassing selection with an already known `check_record`.

## 2. Producer navigation retains the original Attempt and Job

The shared Checks/Artifacts producer link uses the existing Build route's
`attempt_id` and `job_id` parameters. No new navigation/parser is added.

```sh
pnpm --dir ui exec vitest run src/features/issues/IssueResultPanels.test.tsx -t 'exposes exact Build'
```

Red: the rendered link used unsupported `job` and omitted the original Attempt.
Green: the link retains both exact identities. The combined Results and existing
Build-view suite passed 34 cases, including an older Attempt/Job while the Build
has a newer current retry:

```sh
pnpm --dir ui exec vitest run src/features/issues/IssueResultPanels.test.tsx src/features/builds/BuildView.test.tsx
```

## 3. Indefinite retention is explicit

An available metadata record with `expires_at: null` now displays No expiry /
Без срока удаления. Unavailable metadata retains its separate unknown state;
expiry and byte-read policy are unchanged.

```sh
pnpm --dir ui exec vitest run src/features/issues/IssueResultPanels.test.tsx -t 'known indefinite'
```

Red: both languages displayed unknown instead of the specified retention label.
Green: both labels render correctly. The full targeted Results suite passes 16
cases, with the existing expiration/revocation and executable-preview regressions.

## Regression and resource checks

Builds use a task-owned temporary Cargo target, two jobs, disabled incremental
compilation and zero dev/test debug info. The disposable PostgreSQL container uses
an existing pinned local image with `--pull=never`, tmpfs and automatic removal;
no named or host data volume is created. Shared caches and pre-existing changes
are preserved. No commit or push was requested.

- PostgreSQL discovery/restart parity: 2 passed.
- `pnpm --dir ui test:unit`: 33 Node checks and 647 Vitest cases passed.
- `pnpm --dir ui exec playwright test tests/browser/issue-result-panels.spec.ts`:
  4 passed (English/Russian, 390/1280 px), including candidate selection, explicit
  safe preview/download and the known indefinite-retention label.
- OpenAPI was exported from the current REST crate with
  `cargo run --quiet --locked --package octacity-server-api-rest --example export_openapi`,
  then consumed with `OCTACITY_OPENAPI_SCHEMA_FILE=<export> pnpm --dir ui api:generate`.
- `cargo test --locked -p octacity-server-application --lib issue_`: 163 passed;
  `--lib factory_discovery`: 2 passed.
- `cargo test --locked -p octacity-server-api-rest --test v1_handlers`: 159 passed,
  including safe typed Candidate facts through the common HTTP collection,
  every-operation allow/deny checks and OpenAPI drift validation.
- `pnpm --dir ui lint`, `pnpm --dir ui format`, `pnpm --dir ui build`: passed.
  The current bundle has 53 files, 1,152,255 JavaScript bytes, 86,531 CSS bytes
  and 1,239,408 static bytes; all existing budgets remain unchanged and pass.
- `python3 tools/check_architecture.py`: passed (63 packages, 256 edges).
- `openspec validate add-factory-work-card --strict` and `git diff --check`: passed.
- `cargo clippy --locked -p octacity-server-application -p octacity-server-api-rest
  -p octacity-server-store -p octacity-server-store-postgres --all-targets -- -D warnings`:
  passed without warnings.
- `cargo fmt --all -- --check`: passed.

After all checks, the task-owned 1.9 GiB Cargo target was removed and the
`octacity-review-fixes-pg-20261021` container was stopped and automatically removed.
Free space was 47 GiB initially, about 45 GiB during checks and 46 GiB after
cleanup. Only owned temporary resources were removed. OpenSpec remains 47/56
complete; these corrections do not mark later tasks complete.

Documentation-only updates describe these existing contract meanings and corrected
navigation. They introduce no executable behavior; TDD does not apply to prose.
