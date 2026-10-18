# Task 5.6 verification: managed automatic input configuration

## Delivered boundary

Provider-neutral application queries validate input/group intents without discovery
or mutation. The publication command resolves exact Factory Configuration, root
Flow and independently selected code Repository versions, then commits immutable
input/group versions, scoped idempotency, audit and outbox facts together. It
reuses the existing Source journal and management mutation machinery; there is no
new table, migration, dependency, provider enum or second admission identity.

A managed input separately pins connection metadata and the reviewed Project grant
revision. Its Source/optional Reporter settings are exact installed profiles,
independent of the reusable connection profile. Trusted composition declares any
parent relationship; clients cannot manufacture it. Several inputs can reuse one
connection with different installed selectors/mappings. Safe choices include
literal state/label descriptions and exclude ungranted write purposes and private
credential references. Code Repository choices reuse the existing independently
authorized Repository query.

Validation and commit both check current metadata, explicit resource/purpose
grants, active connection state and configured role credentials. PostgreSQL locks
connections in stable order before the owning Project, matching grant publication.
Exact replay returns the original immutable receipt after replacement or grant
revocation, without republishing; application visibility and connection-use rights
are still required. Invalid selected input fields have normalized diagnostics
without credential values or hidden catalogue identities.

Bounded inspection/listing includes disabled/current definitions and exact retained
versions. Every frozen group member's target, Repository and managed connection
must be visible. Memory/PostgreSQL filter before ordering, page limits and cursors;
a hidden row or mixed-visibility group cannot affect a visible page. The REST
cursor is bounded and Project-bound, separate from production polling progress.

The protected opt-in REST surface adds seven operations:

- POST `projects/{project_id}/factory-source-inputs/{input_id}/versions`
- POST `projects/{project_id}/factory-source-groups/{group_id}/versions`
- POST `projects/{project_id}/factory-sources/validate`
- GET `projects/{project_id}/factory-source-choices`
- GET `projects/{project_id}/factory-sources`
- GET `projects/{project_id}/factory-source-inputs/{input_id}/versions/{version}`
- GET `projects/{project_id}/factory-source-groups/{group_id}/versions/{version}`

Paths are relative to `/api/v1/`. Publication requires an Idempotency-Key and
creation/replacement preconditions. The group route can publish member versions
and their exact group atomically; each input retains its own version precondition.
Verified protected ingress, application authorization, closed request bodies and
`Cache-Control: no-store` apply. OpenAPI carries typed authorization/visibility,
parameters and credential-free schemas. Connection setup and native profile
installation remain separate capabilities, not token/path fields in input forms.

Source/Reporter runtime composition resolves each managed input's own connection
and independently rechecks current read/write grants before dispatch. A managed
input with missing resolver fails closed rather than borrowing a legacy profile's
authority. Original action/native target, finite Reporter retry and absolute
deadline remain unchanged. Legacy installed inputs without the optional connection
field retain their serialization and execution behavior.

## Test-first evidence

Rust commands use `--locked` and one build at a time with:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

Observed pre-refactor baselines:

- `cargo test --locked -p octacity-server-store --lib source_`: 5 passed.
- `cargo test --locked -p octacity-server-application --lib factory_admission_tests`: 60 passed.
- `cargo test --locked -p octacity-server-store-postgres --test factory_discovery source_publication_is_atomic_owned -- --ignored`: 1 passed against the disposable PostgreSQL peer, before transaction extraction.

Each row records Red before its implementation and subsequent Green. A missing
required public API is distinguished from fixture/compiler mistakes; those
intermediate mistakes are not claimed as Red evidence.

| Behavioral slice and command filter | Observed Red | Observed Green |
| --- | --- | --- |
| Application `source_configuration_validates_one_connection` | Required typed query/input connection API absent | One connection reused for distinct inputs; independent code Repository, exact root, no mutation |
| Application `source_configuration_validation_freezes` | Validation returned no group | Exact exclusive group members frozen; invalid member version rejected |
| Application `source_configuration_publication_replays` | Publication command/use-case absent | Original receipt after replacement, conflicting key and stale publication rejected |
| Application `source_configuration_publication_rechecks` | Revoked grant still published | Store commit rejects revocation after validation, no input/evidence retained |
| Application `source_configuration_replay_still` | Replay revealed a now-hidden dependency | Current dependency visibility and connection use remain required |
| Application `source_configuration_reuses_one_connection` | Trusted parent-profile API absent | Distinct installed selector settings share one connection profile |
| Application `source_configuration_choices_are` | Safe choice query API absent | Literal arbitrary labels/state, only currently granted installed profiles, no writer/credentials |
| Application `source_configuration_discovery_filters` | Exact get/list APIs absent | Hidden rows/group excluded before limit-one pagination and cursor; historical get authorized |
| Application `source_configuration_validation_identifies` | Missing write grant returned a generic Store conflict | Reporter/repository/source errors identify the exact input field without private values |
| Server `managed_input_publication_uses` | Managed runtime resolver API absent | Own connection resolved, no legacy fallback, revoked read grant blocks later dispatch |
| Server `managed_reporter_input_cannot` | Legacy authority delivered a managed effect | Missing connection resolver retains a bounded retry without a provider call |
| PostgreSQL `managed_source_configuration_publication` | Required management store port absent | Same memory/PostgreSQL atomic concurrent replay and revocation contract passes |
| PostgreSQL read extension, same filter | Default read port returned Unavailable | Same visible page/cursor behavior in both stores, including frozen group members |
| REST `source_configuration_` | Required opt-in REST builder absent | Protected typed routes, exact root/group pins, headers, closed/redacted requests and OpenAPI pass |
| REST `source_configuration_rest_and_schema_allow` | Wire validation accepted an omitted Reporter but OpenAPI incorrectly required it | Schema and REST both preserve the optional write integration |
| REST `source_configuration_rest_bounds` | Invalid limit 0 reached the query peer | Limits 0/65 return 400 before dispatch; the store, REST and OpenAPI share the page ceiling |

Application filters use:

```sh
cargo test --locked -p octacity-server-application --lib source_configuration_
```

The accumulated nine application tests pass. Runtime slices use:

```sh
cargo test --locked -p octacity-server --lib managed_input_publication_uses
cargo test --locked -p octacity-server --lib managed_reporter_input_
```

The Source test and two Reporter tests pass. The second Reporter test qualifies
successful delivery and independently revoked writer authority. Provider-neutral
contracts use controlled peers named GitHub, GitLab, Jira and custom tracker; this
is not a claim that native GitLab/Jira adapters were implemented.

The PostgreSQL command is:

```sh
OCTACITY_POSTGRES_URL=postgres://postgres@127.0.0.1:32787/postgres \
  cargo test --locked -p octacity-server-store-postgres --test factory_discovery \
  managed_source_configuration_publication -- --ignored
```

It passes with a single-connection pool and a separate migrated disposable database
per test. Concurrent identical publication returns Applied/Replayed. A review-valid
replacement becomes unauthorized before commit and leaves no next version. The
same receipt remains historical after grant revocation. The owned PostgreSQL 18.6
container uses bounded tmpfs storage, no persistent volume or user stand.

REST tests use:

```sh
cargo test --locked -p octacity-server-api-rest --test integration_administration source_configuration_
```

All five pass. Unknown token/credential/Issue fields fail before dispatch without
reflecting their contents. Protected ingress and publication preconditions fail
before the authorized command executes.

## Scope and compatibility

This configures **automatic execution inputs**, not importing/refreshing task cards.
An internal Issue is neither required nor accepted as an automatic replay key.
Input identity, native task correlation and explicit future card execution requests
remain separate. Work/Run/Flow history and original source/reporting pins are not
rewritten. Manual Artifact-based Factory admission requires no integration setup.
Persistent cards, synchronization ownership/conflicts and manual Issue creation
remain in `add-factory-work-card`.

Selectors/mappings remain operator-installed, adapter-validated immutable profiles.
This task exposes their safe typed choices and publication; it adds no executable
upload, arbitrary provider JSON, browser credential storage or private-file editor.
Read-only candidate preview is task 5.7; Console input/effect editing is subsequent
UI work. No live account, production deployment, paid model call or real provider
mutation is used for this qualification.

## Regression qualification

The full relevant core/store/application/REST command passed **1030 tests** across
21 suites, including all 453 application tests and the existing manual-admission
and lifecycle journeys:

```sh
cargo test --locked -p octacity-server-factory -p octacity-server-store \
  -p octacity-server-application -p octacity-server-api-rest --lib --tests
```

The server library regression passed **150 tests**, with 8 existing PostgreSQL-only
journeys ignored by that default command:

```sh
cargo test --locked -p octacity-server --lib
```

Its Source/Reporter process, recovery, ordering, manual-outage and original absolute
deadline regressions pass under normal parallel test execution. No timeout or test
serialization was introduced.

A final REST-only rerun after the pagination boundary correction passed **115 tests**:

```sh
cargo test --locked -p octacity-server-api-rest --lib --tests
```

The common route-inventory fixture was extended to recognize these protected
Project routes; no production authorization was weakened to satisfy it. The
configuration record's large input variant was boxed after Clippy identified its
stack size; existing serialization/behavioral tests remain unchanged and pass.

PostgreSQL final qualification passed **14 tests**: the new managed publication/read
contract, the original Source atomicity/recovery contract, manual-admission
migration/replay, and all existing connection, administration and Project grant
contracts:

```sh
OCTACITY_POSTGRES_URL=postgres://postgres@127.0.0.1:32787/postgres \
  cargo test --locked -p octacity-server-store-postgres \
  --test factory_discovery source_ -- --ignored
OCTACITY_POSTGRES_URL=postgres://postgres@127.0.0.1:32787/postgres \
  cargo test --locked -p octacity-server-store-postgres \
  --test integration_connections --test integration_project_access \
  --test integration_administration -- --ignored
```

The owned disposable container was identity-checked and removed after these tests;
its tmpfs databases and any earlier failed-test database were discarded. No user
service, data volume or local stand was modified.

Final non-behavioral checks also passed:

- `cargo clippy --locked -p octacity-server-factory -p octacity-server-store -p octacity-server-application -p octacity-server-api-rest -p octacity-server-store-postgres -p octacity-server --all-targets --all-features -- -D warnings`.
- `pnpm --dir ui api:generate` and `pnpm --dir ui typecheck`.
- `cargo fmt --all -- --check` and `git diff --check`.
- `python3 tools/check_architecture.py`: 63 packages, 253 dependency edges.
- `python3 -m unittest tools.tests.test_check_architecture tools.tests.test_security_matrix`: 37 passed.
- `python3 tools/security_matrix.py validate` and `node ui/scripts/verify-architecture.mjs`.

Documentation/status updates have no executable behavior; OpenSpec strict
validation and whitespace checks qualify those edits. This is targeted source,
integration and API qualification, not the full cross-platform security/release
matrix or a live account qualification.

## Resource use

Disk checks throughout the builds observed 17–18 GiB free. The shared `target`
grew from about 5.4 to 6.1 GiB. Builds used the bounded nonincremental/no-debug
settings above; test execution remained normally parallel. No target wipe,
package/dependency installation or user-data cleanup was performed. The temporary
PostgreSQL container was removed. Work remains in `main`, without a commit.

Both `openspec validate add-github-work-source --strict` and
`openspec validate add-factory-work-card --strict` pass. Task 5.6 is checked;
OpenSpec reports 25/37 complete and task 5.7 next. Completed earlier task entries
and the task-card change remain unchanged.
