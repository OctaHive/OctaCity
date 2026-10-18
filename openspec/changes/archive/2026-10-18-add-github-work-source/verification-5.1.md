# Task 5.1 verification: reusable immutable connection metadata

## Boundary and behavior

`IntegrationConnectionHandlers` publishes and reads metadata through the existing
management authorization decorators and the provider-neutral `IntegrationConnectionStore`.
Creation/replacement requires connection administration; reading requires connection
visibility. These permissions do not grant Project use or activate Factory inputs.
The trusted composition supplies verified installation declarations. Client drafts
select exact installed references, approved canonical HTTPS endpoints, supported
authentication mechanisms, native resources and purposes. They cannot supply
provider identities, executable pins, credentials or paths. Frozen versions retain
the open provider identity, adapter pin, native resource identity and original
capability/configuration/logical-credential references.

The existing `IntegrationId`, `IntegrationVersion`, `IntegrationName`, Factory
references and logical-secret types are reused. Native resources remain independent
of OctaCity Projects and code Repositories. GitHub/GitLab/Jira/custom fixture names
exercise the common contract; this does not advertise new native provider adapters.

Publication atomically commits the version, current pointer, security-scoped replay
receipt, safe audit and outbox using existing management mutation infrastructure.
Old versions are retained. Current-version conflicts and changed intent fail without
altering the current version. Exact replay returns the original receipt before
current installation resolution, after current management authorization. It retains
the original timestamp even after a newer version exists or an installation is
unavailable. Audit/outbox contain identity/version facts, not endpoints or credential
references. No provider or credential I/O occurs at this boundary.

Migration 0064 adds two tables only. It does not import existing installations,
create grants, change VCS/Delivery bindings or enable inputs. Explicit adoption can
reuse an existing VCS IntegrationId and preserve the original WorkSourceBinding.
Protected credential writes, Project-use grants, management REST composition and UI
remain in their subsequent section-5 tasks.

## TDD evidence

| Slice | Observed Red before implementation | Observed Green |
| --- | --- | --- |
| Metadata selection | Public connection metadata API absent | Open provider identity, exact adapter/native/resource pins and original source binding retained |
| Endpoint/installed ceiling | `http://tracker.invalid/` was incorrectly accepted | Unsafe/noncanonical URLs, uninstalled endpoints/authentication/resources/purposes and duplicate selections rejected |
| Atomic publication | Public connection publication store API absent | Shared memory/PostgreSQL contract covers immutable versions, replay, changed intent, invalid update and stale current-version rejection |
| Authorized application | Connection command/query/handler API absent | Denied publication/replay has no side effects; authorized exact replay survives empty installation catalog |
| PostgreSQL | `PostgresStore: IntegrationConnectionStore` absent | Persistent publication/replay, one winning concurrent replacement and transactional outbox-failure rollback |
| Durable integrity | A substituted stored endpoint was returned despite the original definition digest | Durable reads recompute the common content digest and reject inconsistent metadata |

Characterization checks additionally cover client credential/executable injection,
unselected write purposes, concurrent identical retry, explicit legacy adoption and
manual admission replay across migration/restart. The initial adoption assertion
compared the fixture timestamp (1) with the older SQL seed timestamp (0); the test
was corrected to compare the actual persisted Repository before/after adoption.
No production behavior was changed for that fixture discrepancy.

## Commands

Baseline before implementation:

```sh
cargo test --locked -p octacity-server-application --lib factory_admission_tests
cargo test --locked -p octacity-github -p octacity-server-vcs -p octacity-delivery-protocol
```

Observed baseline: 60 admission tests and 92 GitHub/VCS/Delivery tests passed.

Red/Green slice commands (the initial API commands failed before their implementation):

```sh
cargo test --locked -p octacity-server-store --lib connection_metadata_resolves
cargo test --locked -p octacity-server-store --lib connection_metadata_rejects
cargo test --locked -p octacity-server-store --lib connection_publication_is
cargo test --locked -p octacity-server-application --lib connection_application
cargo test --locked -p octacity-server-store-postgres --test integration_connections -- --ignored
```

Final regression/check results are recorded below. PostgreSQL commands use
`OCTACITY_POSTGRES_URL` pointing at a task-owned disposable PostgreSQL 18.6 container
with independently generated databases, without accessing the user's local stand.
`CARGO_INCREMENTAL=0` was used for final Rust checks to bound generated cache growth.
The store now directly declares the workspace's existing `url` dependency; the
lockfile changes only that package dependency edge, with no package/version update.

Final checks:

```sh
cargo test --locked -p octacity-server-store --lib
cargo test --locked -p octacity-server-application --lib connection_application
cargo test --locked -p octacity-server-store-postgres --test integration_connections -- --ignored
cargo test --locked -p octacity-server-store-postgres --test migrations -- --ignored
cargo test --locked -p octacity-server-store-postgres --test factory_discovery -- --ignored
cargo test --locked -p octacity-github -p octacity-server-vcs -p octacity-delivery-protocol
cargo clippy --locked -p octacity-server-store -p octacity-server-application -p octacity-server-store-postgres --all-targets --all-features -- -D warnings
cargo check --locked -p octacity-server-api-rest
cargo fmt --all -- --check
openspec validate add-github-work-source --strict
git diff --check
```

Observed: 75 store tests, the application connection contract (four providers),
4 connection PostgreSQL tests, 6 migration tests, 23 Factory discovery/Source
PostgreSQL tests and 92 provider/VCS/Delivery tests passed. Clippy, REST compilation,
formatting and change validation passed. The migration inventory now lists the two
new global integration tables and the two previously omitted Source projection
tables; schema-46 snapshot rollback explicitly checks that connection tables are
absent. The initial inventory failures required no change to older migrations.

A first broad PostgreSQL run, concurrent with the full application unit suite,
passed 22/23 discovery tests and failed the existing Source contract with
`Unavailable`; a repeat during that load failed while opening its 250 ms pool
with `PoolTimedOut`. The final identical default-parallel discovery command passed
all 23 tests in 30.73 seconds after that load subsided. No timeouts, serialization,
Source production code or Source timing assertions were changed for task 5.1.
This is evidence of a remaining environmental sensitivity in that older fixture,
not a qualification of unlimited-load reliability.

The additional full `cargo test --locked -p octacity-server-store
-p octacity-server-application --lib` regression completed successfully: all
438 application tests passed (564.77 seconds), including manual Factory admission
and complete requirements/release journeys. Its initial store build passed 74
cases; the final store-only rerun includes the additional selection characterization
and passed 75. The final application connection test was also rerun successfully
with denied-query coverage after the integrity change.
