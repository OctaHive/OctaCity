# Task 5.4 verification: explicit Project access

## Qualified boundary

Project access is a separate, versioned connection/resource/purpose subset. Publishing
connection metadata or configuring a token grants no Project use. Empty replacement
resources explicitly revoke access. Publication requires connection and Project
administration, plus Project visibility; enumeration requires Project/connection visibility and explicit connection use
and returns only granted resource capabilities. Administration and use are checked
separately through the existing management authorization dispatchers.

The common purpose vocabulary keeps task read, task update, repository read and
Delivery independent. Shared application authorization verifies the exact metadata
version, current Project grant, enabled/active administration state and configured
purpose credential. It reads no secret and invokes no provider. The request contains
no Factory input, Work, Run or internal Issue requirement, allowing a future task
card to use the same authority without creating an execution request.

Source reads/authorization and every Reporter inspect/apply dispatch can explicitly
adopt this guard. Adoption preserves original installed namespace, native resource,
capability, configuration and executable pins. The guard is outside the immutable
Reporter draft, so adoption does not rewrite its binding or accepted effects.
Denied managed use returns before provider dispatch; it cannot fall back to an
operator-installed profile. Existing explicitly installed authority remains supported
when no managed adoption was configured. There is no automatic adoption by name.

PostgreSQL migration 0066 adds access heads and immutable revisions, foreign keys to
Projects and original metadata, optimistic versions, and attributed atomic mutation
replay/audit/outbox publication. Concurrent exact retries apply once; competing
replacements cannot both win. Historical replay returns the original receipt and
cannot restore a newer revoked grant. Metadata is read through the publication's
existing transaction, so publication works with one pool connection.

The protected REST surface adds POST/GET
`/api/v1/integrations/{integration_id}/projects/{project_id}/access` and GET
`/api/v1/integrations/{integration_id}/projects/{project_id}/resources`. Mutations
use the existing idempotency/precondition headers. The routes retain verified
protected ingress, typed authorization registration and `Cache-Control: no-store`.
Resource choices omit credential references as well as secret values.

## TDD evidence

Each implementation slice followed the repository public seams: store ports,
authorized application handlers, verified runtime processes and HTTP. Missing API
compilation failures below refer to the newly required public API, not environment
failures. Follow-up assertions also qualify the four-purpose matrix and durable
replay, competing publication and event rollback.

| Test command | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server-store project_grants_are_explicit_resource_and_purpose_subsets --lib` | Missing Project access publication API | Passed |
| `cargo test -p octacity-server-store project_grants_reject_uninstalled --lib` | Uninstalled task-update purpose was accepted | Passed after subset/duplicate validation |
| `cargo test -p octacity-server-application runtime_use_requires_live_project --lib` | Missing current resource authorization API | Passed; independent purpose checks |
| `cargo test -p octacity-server-store-postgres --test integration_project_access -- --ignored` | PostgreSQL did not implement the access store contract | Passed against disposable PostgreSQL |
| `cargo test -p octacity-server-application grant_publication_needs_both --lib` | Missing management publication/enumeration API | Passed with separate administration/use and visibility |
| `cargo test -p octacity-server managed_source_rechecks_project --lib` | Missing managed runtime guard/adoption API | Passed: revoked read caused no further provider request or Run |
| `cargo test -p octacity-server managed_reporter_revocation --lib` | Missing Reporter guard adoption API | Passed in `cargo test -p octacity-server managed_ --lib`: retained retry made no further provider call; Work/Run/Flow unchanged |
| `cargo test -p octacity-server-api-rest --test integration_administration project_access_rest` | Missing typed Project access endpoints | Passed: verified ingress, preconditions, safe resources and OpenAPI |
| `cargo test -p octacity-server-application grant_publication_needs_both --lib` (visibility follow-up) | A use grant enumerated connection resources without a separate View grant | Passed after checking both before store reads |
| `cargo test -p octacity-server-store project_grants_require_an_existing_project --lib` | Memory publication accepted an absent Project | Passed; valid fixtures explicitly seed Projects |
| Single-connection variant of the PostgreSQL access contract above | A temporary five-second test watchdog exposed a nested pool acquisition deadlock | Passed after using the existing transaction; final test awaits the ordinary store contract without an extra timing assertion or increased timeout |

Later Cargo verification uses one build at a time with:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
export OCTACITY_POSTGRES_URL=postgres://postgres@127.0.0.1:32786/postgres
```

The explicitly owned disposable PostgreSQL 18.6 container is separate from the local
stand. Every test creates its own database. No live provider credentials, paid model
calls, external Issue mutations, input activation or local-stand database changes
are part of this qualification.

## Scope

This slice supplies management/runtime contracts and explicit trusted composition
hooks. Input selection/publication and its composition remain task 5.6; provider
readiness checks remain 5.5 and Console access editing remains 5.10. Existing VCS
and Delivery protocols/credential classes are unchanged; their purpose cannot
authorize task operations. Native GitLab/Jira adapters and persistent internal
Issues are not implemented by the common grant vocabulary.

## Regression and checks

Commands use the bounded build environment above after the target cleanup.

- `cargo test -p octacity-server-store -p octacity-server-application -p octacity-server -p octacity-server-api-rest --lib`: 675 passed, 8 PostgreSQL runtime tests ignored pending the separate service run. This full baseline preceded the final visibility check; the affected application contracts then passed again (`cargo test -p octacity-server-application integration_project_access_tests --lib`, 2 passed).
- `cargo test -p octacity-server-store-postgres --test integration_project_access --test integration_connections --test integration_administration --test migrations -- --ignored`: 17 passed against the disposable service, including grant concurrency/event rollback and forward-failure/previous-schema snapshot rollback.
- `cargo test -p octacity-server --lib runtime::work_source_tests::reporting::ordering::postgres -- --ignored`: all 8 separately selected PostgreSQL runtime tests passed, including slow admission/confirmation and retained-intent ordering/recovery.
- `cargo test -p octacity-server-api-rest --tests`: 109 passed. The first full run detected three missing requests in the authorization-inventory fixture (97 requests for 100 operations). The fixture now exercises all three new protected routes; both complete authorization-inventory tests and OpenAPI drift checks passed.
- `python3 tools/check_architecture.py`: passed (63 workspace packages, 253 dependency edges).
- `python3 -m unittest tools.tests.test_check_architecture tools.tests.test_security_matrix`: 37 passed.
- `python3 tools/security_matrix.py validate`: passed (inventory validation, not the full cross-platform security matrix).
- `openspec validate add-github-work-source --strict` and `openspec validate add-factory-work-card --strict`: passed.
- `node scripts/verify-architecture.mjs` in `ui`: passed.

- `cargo clippy -p octacity-server-store -p octacity-server-store-postgres -p octacity-server-application -p octacity-server-api-rest -p octacity-server --all-targets --all-features -- -D warnings`: passed. A test-support re-export was moved ahead of the existing test module to satisfy `items_after_test_module`; this changes no executable behavior.
- `pnpm --dir ui api:generate` and `pnpm --dir ui typecheck`: passed; the generator exports the locked Rust OpenAPI contract. No Console flow was changed in this slice.
- `cargo fmt --all -- --check` and `git diff --check`: passed after formatting the added fixtures/re-export placement.

All 22 previously completed task entries remain unchanged. The pre-existing
`add-factory-work-card` artifacts match their session baseline hashes. Only 5.4 is
newly complete (23/37 tasks); no later task is claimed complete.

The temporary test container and its anonymous volume were removed; the four local
stand containers remain healthy. After the interrupted build was stopped and
2.3 GiB of its artifacts removed, the remaining verification used the bounded build
profile above. Final `target` size is about 3.2 GiB with 20 GiB free; no Cargo profile
or production setting was changed. Work remains uncommitted on `main`.
