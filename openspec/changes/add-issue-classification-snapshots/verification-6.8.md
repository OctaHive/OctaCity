# Task 6.8 verification

## Scope and public boundary

`PATCH /api/v1/projects/{project_id}/issues/{issue_id}/materials` exposes the
existing `EditIssueMaterialsCommand` through an optional
`IssueManagementApplication::with_materials` capability. Runtime composition
uses the existing Issue/discussion and document stores, policy and server clock.
There is no new source reader, byte store, migration, permission or execution
path. Current and exact historical Issue detail and mutation receipts now expose
the optional bounded `materials` metadata from the existing immutable revision.
Legacy empty selections retain the omitted wire encoding.

Tests run through the HTTP router with real Issue, discussion, document and
material handlers over existing public in-memory persistence/byte ports. The
public mutation-evidence probe checks atomic rejection; document bytes and
application commands are not stubbed. The document and material suites share a
small collaboration fixture. Closed common reference/selection schemas have one
owner instead of being split between classification and snapshot schemas.

## Red → Green

From the repository root:

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-api-rest --test v1_handlers issue_materials_http_select -- --nocapture
```

Red: the selection request returned **404 instead of 200**, before the material
mutation route existed. Green: the thin adapter delegates to the existing
application owner, assigns Project/Issue from the route and version from the
strong `If-Match`, and returns the original Issue receipt with body-free material metadata,
ETag and no-store. The test verifies selection, explicit removal, current/exact
historical detail, unchanged requirements/applicability, original receipt replay
after a newer revision and retained independent source content.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-api-rest --test v1_handlers issue_materials_http_ -- --nocapture
```

All **6 focused tests** passed. Additional cases qualify existing owner behavior
through HTTP: both document owner kinds, exact immutable pins, server-derived
selection attribution and unchanged-entry attribution after replacement; new
publications do not retarget selections. Forged bodies/owners/actor/time,
digest/size mismatches, cross-Issue/foreign references, duplicate identities and
33 distinct entries are rejected without mutation evidence; **32 actual retained
comments are accepted**. Wrong Project/Issue routes cannot disclose or mutate
selection. Missing/malformed/stale preconditions, absent keys, changed-intent and
cross-Issue/security-scope replay are rejected. Current Issue Update/visibility
and independent source rights are required for a new selection. Safe metadata
and original receipt replay do not grant source-body access; removal needs no
source-body permission, and replay restores neither a removed selection nor an
expired body.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-api-rest --test v1_handlers every_management_operation -- --nocapture
```

The exhaustive authorization inventory detected the missing new operation
(**235 requests vs 236 operations**). Adding the typed recording handler/request
and optional test composition restored both allow and deny paths in the full
suite; no bypass or inventory exemption was added.

## Regression and generated contracts

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-api-rest --lib --test v1_handlers --test v1_contract
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-application --lib issue_materials_
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-application --lib snapshots::materials
CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true pnpm --dir ui api:generate
pnpm --dir ui typecheck
CARGO_INCREMENTAL=0 cargo check --offline --locked -p octacity-server
cargo fmt --all --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

The REST suites passed **13 library + 287 handler + 33 contract = 333 tests**,
including all task 6.7 document tests. Existing application suites passed
**5 selection + 17 snapshot-material tests**: independent owner-aware readers,
count/aggregate UTF-8 limits, required blockers, optional omissions, exact
provenance, admission/replay and legacy compatibility remain qualified. OpenAPI
and client generation, TypeScript checking, runtime compilation, Rust formatting,
architecture (64 packages, 263 edges), strict OpenSpec and whitespace checks
passed. Generated client output remains ignored/reproducible under the existing
workflow; its request union distinguishes same-Issue sources from shared Project
sources, excludes caller attribution/bodies and retains the 32-entry bound.

Documentation-only edits have no executable behavior requiring TDD. The new
reference section and this report were formatted without reformatting the
pre-existing reference. Native PostgreSQL/object storage, Agent/executor,
provider effects and packaged release execution were not run or claimed here.
No commit was requested. Prior task 6.7 work and the untouched untracked
`github.token` remain in place. Console selection controls remain tasks 6.9–6.10.
