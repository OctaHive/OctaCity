# Task 2.5 — Manual Issue application and memory contracts

Implemented on 2026-10-19. Scope: provider-neutral manual create/read/edit,
immutable requirements revisions, workflow/status pins and explicit guarded
organizational moves through the authorized public application boundary.
PostgreSQL, REST composition and the Console card remain tasks 2.6–2.8.

## Behavior and reuse

- `IssueHandlers` compose the existing authorized management dispatch and audited
  mutation context over one `IssueStore` port. No execution, admission, adapter,
  credential, network or provider port is supplied to these handlers.
- `IssueId` and positive `IssueVersion` are domain values. An atomically allocated
  positive Project-local number produces a stable `ISS-1` display key. Different
  Projects may each have `ISS-1`; the key never authorizes or identifies a native task.
- Manual drafts need a nonblank title, permit empty description and incomplete
  acceptance, and use the exact selected default/initial status at creation. Text
  revisions preserve identity, key, workflow/status and original creation time.
  Current and retained reads stay within the owning Project.
- Creation checks Project visibility before replay. Dedicated content/history
  reads and edits/moves check current Issue visibility before lookup/replay.
  Creation uses `Create` on Project-owned `Issue`; content editing uses `Update`
  on `Issue`, organizational movement uses separate `Publish`, and reads use
  `View`. None implies workflow `Administer`, execution or Factory-control rights.
- Exact replay is partitioned by operation and accepted management security scope.
  Original receipts survive later edits/moves; changed intent and stale reviewed
  Issue/workflow/status conflict. Generic input documents reject reserved setters.
- One transition command checks a direct edge in the Issue's pinned policy,
  permitted manual origin, source ownership and required evidence. Manual commands
  cannot supply trusted execution evidence; guarded edges fail closed. No name,
  terminal flag, model prose or multi-edge path substitutes for an accepted action.
  Accepted automated producers and their provenance are later action/source tasks,
  which reuse this boundary rather than add another transition implementation.
- Creation retains initial placement; moves append exact before/after/version,
  edge, reason, actor and time history. Text edits add no status event. History
  requests are bounded to 1–200 entries with exclusive revision continuation.
- A retained trusted `IssueFieldPolicy` blocks changed source-owned fields while
  allowing unrelated local edits. Approved push fields and an immutable mapping
  reference produce a typed `IssuePushIntent` for the original revision. Replay
  and mapping replacement do not retarget or duplicate that intent. The fixture
  policy initializer is test-support only, modeling future authorized source-link
  state; it is not an operator API or an adapter/permission implementation.
- The memory adapter shares Project/workflow authority and one transactional
  revision-publication helper. Revision, initial/status history, replay, safe audit,
  outbox and any selected push intent publish together. It reuses the existing
  failure points; failure consumes no number or partial authoritative evidence.
  Reporter dispatch and real persistence are not claimed by this memory slice.
- Dedicated content remains available to authorized reads, while generic Debug,
  audit and outbox omit requirements and organizational reason text. Display keys,
  status vocabulary and provider wire names carry no execution authority.

## Red/Green evidence

Each slice was tested before its implementation. A missing required contract is
reported as a compile-time Red; it is not confused with an environment failure.
Test-fixture constructor mistakes encountered during development were corrected
and are not counted as behavior evidence.

| Public application test | Observed Red | Observed Green |
| --- | --- | --- |
| `issue_manual_draft_uses_default_without_execution_or_provider` | Required Issue create/read contracts were absent | Draft, stable key/default pin, read-only lookup and original creation replay |
| `issue_edit_retains_revisions_and_original_replay_after_later_edits` | Required edit contract was absent | Retained text versions, stale/changed-intent conflict and original receipt after later edit |
| `issue_explicit_moves_share_exact_pinned_edges_and_replay_original_history` | Required transition/history contracts were absent | Direct organizational moves, bounded history and original replay after later terminal move |
| `issue_hidden_reads_and_replays_are_rejected_before_content_or_mutation` | Hidden current content was returned | Hidden current/exact/history and edit/move replay fail before mutation or disclosure |
| `issue_source_owned_fields_reject_local_replacement_but_allow_unrelated_edits` | Required retained ownership contract/fixture was absent | Local description/status replacement conflicts; unrelated priority remains editable |
| `issue_move_atomically_retains_exact_push_intent_without_provider_dispatch` | Required retained projection evidence contract was absent | One exact revision/mapping/field intent; mapping replacement/replay retain the original |
| `issue_mutations_rollback_revision_history_replay_audit_and_push_at_each_failure_boundary` | Required Issue failure-injection contract was absent | Create/edit/move rollback at all five existing failure boundaries and exact retry/replay |
| `issue_debug_does_not_disclose_requirements_or_organizational_reason` | Organizational reason appeared in generic Debug | Requirements and reason omitted from command/revision/history diagnostics |
| `issue_creation_requires_visible_project_even_for_original_replay` | Invisible Project creation replay returned its saved Issue | Original/new intent denied without mutation; explicit visible Project scope remains usable |
| `issue_outbox_identity_cannot_collide_when_scope_and_key_contain_separators` | Two distinct scope/key pairs produced one outbox identity | Existing structured management evidence identity retains both operations without delimiter collisions |

Additional regression scenarios cover separate edit/move/create rights, actual
verified service attribution, independent security-scoped keys and Project-local
numbers, retained workflow policy after default replacement, arbitrary names and
order, disallowed origins/evidence/path inference, byte bounds, closed reserved
setters and cross-Project reads/moves. Existing workflow and native-source Issue
correlation regressions remain passing. No larger timeouts or serialized test
harness were introduced.

## Commands and qualification

All Rust commands reuse the existing target with:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

Focused Red/Green commands (each named filter was run separately):

```sh
cargo test --locked -p octacity-server-application --lib issue_manual_draft
cargo test --locked -p octacity-server-application --lib issue_edit_retains
cargo test --locked -p octacity-server-application --lib issue_explicit_moves
cargo test --locked -p octacity-server-application --lib issue_hidden_reads
cargo test --locked -p octacity-server-application --lib issue_source_owned
cargo test --locked -p octacity-server-application --lib issue_move_atomically
cargo test --locked -p octacity-server-application --lib issue_mutations_rollback
cargo test --locked -p octacity-server-application --lib issue_debug
cargo test --locked -p octacity-server-application --lib issue_creation_requires
cargo test --locked -p octacity-server-application --lib issue_outbox_identity
cargo test --locked -p octacity-server-application --lib issue_
```

The final focused selection passes **26 tests**, including **14 new Issue tests**.
Broader check results and their scope are recorded below. Documentation and
formatting have no executable behavior; their checks are separate from TDD.

Free space was checked before and throughout scoped builds: approximately 15 GiB
at entry and 14 GiB after focused qualification. No target/cache reset, Docker
image rebuild, live token access, external mutation or commit was performed.

## Final checks

| Boundary | Result |
| --- | --- |
| Full library regression (`domain`, `store`, `application`) | 501 application tests and 81 store tests pass; domain has no standalone library tests |
| Final Issue/workflow selection after the outbox identity correction | 26 pass, including all 14 new Issue tests |
| Clippy for all targets of the three affected packages | Pass with `-D warnings` |
| Downstream REST, PostgreSQL and server compilation | Pass |
| Rust formatting and whitespace | Pass |
| Architecture | Pass: 63 packages, 255 edges, no dependency changes |
| OpenSpec strict validation | Pass |

The full application regression includes the existing long requirements/release
journeys and ran before the final test-only outbox-evidence identity correction.
That correction changes only the Issue memory publisher's evidence encoding;
the complete Issue/workflow selection and Clippy were rerun afterward. Existing
Factory execution code was unchanged. No full-suite timeout or serialization was
introduced to shorten that regression.

```sh
cargo test --locked -p octacity-server-domain -p octacity-server-store -p octacity-server-application --lib
cargo clippy --locked -p octacity-server-domain -p octacity-server-store -p octacity-server-application --all-targets -- -D warnings
cargo check --locked -p octacity-server-api-rest -p octacity-server-store-postgres -p octacity-server
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
git diff --check
```

Final free space: approximately 13 GiB. No dependencies, migration, OpenAPI,
generated Console contract or UI behavior changed, so no unrelated database,
provider or browser stand was built. Only task 2.5 is marked complete; tasks
2.6 onward and the other active changes retain their prior state. See
[Issue contracts](../../../docs/reference/issues.md) for the current interface
and rollout boundary.
