# Task 2.1 verification

## Scope and decisions

Extend the existing accepted classification action, authorized Issue command and
PostgreSQL transaction. No second classification writer or scalar-value store.

The operator clarified the built-in model during implementation:

- Priority is a Project choice ID, initially `blocker`, `high`, `medium`, `low`.
- Component is a built-in attribute with each Project's own choices.
- Type is built in, with Project-specific choices initially `defect`, `feature_request`.
- Size is built in, with fixed `small`, `medium`, `large` values.
- These defaults supply choices, never automatically populated Issue values.
- Arbitrary custom properties remain separate. Old component/kind/size bindings
  and numeric priorities retain their historical meaning and encoding.
- Project settings will provide separate priority/component/type-list editors in
  task 6.6. General classification controls and new Flow authoring are still the
  later tasks in sections 5 and 6, not completed by this backend slice.

Priority remains in one canonical `content.priority` location, with a closed
legacy-number/Project-choice representation. There is no implicit conversion of
old values, accepted results or Work scheduling. The later pinned admission policy
owns mapping new Issue choices to numeric Work priority.

## Observed Red and Green

All Rust commands used `CARGO_INCREMENTAL=0`. PostgreSQL commands used
`OCTACITY_POSTGRES_URL` pointing to a disposable local PostgreSQL 18 instance.

```sh
cargo test --locked -p octacity-server-store \
  builtin_component_uses_project_choices_independently_of_custom_properties -- --nocapture
```

Red: the public Project catalog rejected `components`. After adding built-in
component, Green. Extending the same reviewed planning case for type/size gave
Red on unsupported planning fields, then Green. It verifies accepted own-Project
choices, separate arbitrary boolean properties, rejection of an undeclared
component, immutable history and scoped replay without inventing a producer.

```sh
cargo test --locked -p octacity-server-store \
  issue_priority_uses_project_choices_and_preserves_legacy_numeric_history -- --nocapture
```

Red: `high` failed decoding because priority required `u8`. Green: default `high`
is accepted, `urgent` is rejected before Project publication and accepted after
explicit publication, and original numeric history/replay remain unchanged.

```sh
cargo test --locked -p octacity-server --test factory_configured_flow \
  classification_action_atomically_applies_builtin_and_project_fields_with_provenance \
  -- --ignored --nocapture
```

Red: the real Flow owner rejected the newly selected classification fields.
Green: the accepted producer, trusted action adapter, authorized command and real
PostgreSQL owner publish exactly one resulting Issue revision with priority,
component, type, size and unresolved reproduction plus five field attributions.
Changing the assessment to enum priority `high` produced another behavioral Red
until the new action consumed Project choices instead of numbers. Attribution
pins the original Issue/applicability versions, Work/Run and accepted record;
unrelated boolean properties, labels and the original admitted Work remain intact.

```sh
cargo test --locked -p octacity-server-store-postgres --test issue_execution \
  pipeline_priority_choice_preserves_unselected_planning_and_records_its_catalog \
  -- --ignored --nocapture
```

Red: applying only priority also changed the unselected planning catalog version.
Green: only priority and its exact-catalog attribution change; planning stays intact.

```sh
cargo test --locked -p octacity-server-api-rest --test v1_handlers \
  issue_http_priority_uses_project_choices_instead_of_work_scores -- --nocapture
```

Red: HTTP creation rejected `high` with 400 instead of 201. Green: the real handler
accepts the Project choice and built-in type/size, rejects an undeclared priority,
and the resource matches OpenAPI. A subsequent metadata Red showed the omitted
catalog's default choices were not advertised; OpenAPI now derives those defaults
from the same domain catalog while retaining original legacy publication bytes.

```sh
cd ui
node --test scripts/issue-constraints.test.mjs
pnpm exec vitest run src/features/issues/IssuePriority.test.tsx
```

Red: API generation could not derive legacy bounds from the new priority union.
Green: the generator reads the explicit legacy integer branch without treating
Project choices as scores. Red: the existing editor treated `urgent` as a numeric
legacy value. Green: an unrelated edit preserves the exact existing choice ID,
including when the operator reselects it, rather than coercing it to `NaN`.

## Regression checks

- Store core: `cargo test --locked -p octacity-server-store --lib` — 118 passed.
- Application Issue contracts: `cargo test --locked -p octacity-server-application
  --all-features --lib issues` — 169 passed.
- REST Issue contracts: `cargo test --locked -p octacity-server-api-rest --test
  v1_handlers issues` — 81 passed, including generated-schema validation.
- PostgreSQL: `cargo test --locked -p octacity-server-store-postgres --test issues
  --test issue_execution -- --ignored --test-threads=2` — 81 Issue and 23 execution
  tests passed, including numeric classification receipt upgrades/replay; the
  additional priority-only regression passed separately.
- Console: `pnpm exec vitest run src/features/issues` — 209 passed;
  `node --test scripts/issue-constraints.test.mjs` — 4 passed.
- API generation, Console type checking/lint, Rust formatting, six-package
  all-target/all-feature Clippy, architecture and strict OpenSpec validation pass.

Documentation/planning changes have no executable behavior and use OpenSpec and
whitespace validation. Strict validation retains the previously documented
informational predecessor-spec synchronization reminder; no archive was attempted.
No running personal stand, provider integration or VM release journey was used.
