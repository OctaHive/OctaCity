# Task 3.1 verification

## Scope

Flow drafts and immutable Flow/Configuration publications can now retain an
optional, versioned `issue_snapshot` declaration. Earlier documents omit the
field and keep their original encoding. The declaration has no producer Flow,
terminal, result schema or Run selector.

The contract selects named required/optional values from built-in title,
description, acceptance, priority, component, type and size fields, ordinary
Project properties, explicit classification bindings and the existing bounded
Work/Run dependency collection. It pins the exact Project catalog publication.
The publication compiler reuses canonical types, bounds and active choices;
consumer schemas can accept strings without copying Project choice enums.
Unmapped semantics, unknown/retired properties and incompatible consumer types
produce field-specific diagnostics. An optional built-in with an empty choice
list can describe absence; a required one blocks publication. This does not
invent a value or permit a value outside the pinned catalog.

Materials are explicit exact comment/Issue-document/Project-document selectors
or an opt-in to approved Issue selections. Their declared aggregate ceilings
are at most 32 entries and 1 MiB UTF-8, with stricter individual selections.
Duplicate exact identities cannot evade validation by changing a document's
claimed owner kind. Material fields project ordered text arrays; actual source
ownership, immutable identities and digests belong in the later retained
execution snapshot. Safe evidence selections name a selected field and expose
bounded record-digest arrays, never report bodies or caller-written verification
flags. Both families have explicit required/optional handling.

Selections use the existing `Incoming` channel and mapping grammar. Each step's
actual source paths and target types are qualified against the selected data,
including `IfPresent` guards for optional values. Unselected paths and unsafe
optional dereferences fail with node/definition diagnostics. Ordinary `Record`
sources remain usable alongside snapshots, including whole alias-map projections.
The mapping walker and structural compatibility logic are shared with existing
contracts; no second interpreter or synthetic schema publication is introduced.

The new mode rejects competing accepted-predecessor inputs, unsupported versions,
empty/oversized declarations, alias collisions and incompatible Incoming schemas.
The same Project-authorized catalog validation runs for Flow and Configuration
publication. Configuration failures reserve no mutation evidence and exact
publication retry retains its original contract.

This slice publishes declarations, not live Issue input bytes. Snapshot
preparation/admission remains tasks 3.2–3.9; REST/editor exposure remains section
6. Existing ingress paths reject required missing Incoming data, so this change
does not silently admit unprepared snapshot targets. It adds no scheduler,
document publication, discussion ingestion, mutable classification copy or SQL
migration.

## Observed Red and Green

All Cargo commands used `CARGO_INCREMENTAL=0`. Each command below was run before
its corresponding implementation and then passed after it:

```sh
cargo test --locked -p octacity-server-application --lib \
  flow_publication_pins_an_issue_snapshot_without_a_producer
cargo test --locked -p octacity-server-application --lib \
  flow_publication_rejects_missing_or_incompatible_issue_field_bindings
cargo test --locked -p octacity-server-application --lib \
  flow_publication_rejects_unbounded_or_competing_issue_snapshot_sources
cargo test --locked -p octacity-server-application --lib \
  flow_publication_validates_explicit_material_and_evidence_selections
cargo test --locked -p octacity-server-application --lib \
  flow_publication_checks_snapshot_step_paths_types_and_optional_presence
cargo test --locked -p octacity-server-application --lib \
  flow_publication_reuses_builtin_project_and_dependency_contracts_without_copied_enums
cargo test --locked -p octacity-server-factory --lib \
  issue_snapshot_projection_preserves_a_whole_alias_map
cargo test --locked -p octacity-server-application --lib \
  flow_publication_allows_an_optional_builtin_with_an_empty_project_choice_list
```

1. Red: authoring rejected the unknown `issue_snapshot` field. Green: ordinary
   publication retains the declaration, round-trips it and preserves sibling
   step projections without a producer.
2. Red: a nonexistent Project property was accepted. Green: unknown properties,
   missing semantic mappings and incompatible field types block publication.
3. Red: contract version 2 was accepted. Green: version, count, empty-selection
   and competing-source checks reject invalid declarations.
4. Red: an empty exact-material selection was accepted. Green covers exact and
   approved selections, both document owner kinds, required/optional inputs,
   duplicate references, per-selection/aggregate bounds and evidence selectors.
5. Red: an unselected step input path was accepted. Green rejects unknown paths,
   wrong target types and unguarded optional reads while preserving guarded
   mappings and ordinary record inputs.
6. Red: the dependency selector was absent. Green retains the existing typed
   dependency shape and qualifies all supported built-in/custom scalar families
   and a mapped classification choice without copied vocabulary enums.
7. Red: a whole alias map containing a snapshot and ordinary record was rejected.
   Green preserves that existing mapping form while checking the known snapshot
   value's type.
8. Red: optional component selection failed against the empty default catalog.
   Green allows explicit absence without a new value and still rejects the
   required version of that declaration.

Before refactoring the shared mapping checker, its existing `flow_data` public
contract tests passed (24 tests). The added Configuration publication/replay
characterization passed against the shared validation. Fixture import/error-type,
sorted-node-index and profile-identity setup mistakes were corrected and are not
counted as behavioral Red.

## Final qualification

```sh
cargo test --locked -p octacity-server-factory --lib
cargo test --locked -p octacity-server-application --lib factory::authoring
cargo test --locked -p octacity-server-application --lib issues
cargo clippy --locked -p octacity-server-factory -p octacity-server-application \
  -p octacity-server-store-postgres -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

**604 tests passed**: 351 Factory core, 78 authoring and 175 Issue application
tests. The authoring suite includes eight snapshot cases, new and existing
explicit-producer publication and ordinary intra-Flow authoring regressions.
Clippy passed with warnings denied for all targets/features of the four selected
packages, including native server test compilation and REST dependencies.
Formatting, architecture (64 packages, 263 edges), strict OpenSpec and diff checks
passed. This slice changed no PostgreSQL writer, REST schema definition or UI;
their separately planned native/client/browser qualifications were not claimed.
No commit was requested for task 3.1.
