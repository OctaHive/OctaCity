# Task 1.2 — Shared prerequisite inspection and hand-off

Inspected on 2026-10-18 at `2b7a464200f590e59f7f8339033e567b62a09ba5`.
Scope: inspect and characterize existing boundaries; specify the shared hand-off
before implementation. This task does not implement Issues, workflows, text
intake, role profiles or a native provider adapter.
Status: task 1.2 complete; 2 of 42 work-card tasks complete.

## Concrete implementation inventory

| Boundary | Present implementation and limits |
| --- | --- |
| Ordinary admission | `FactoryAdmissionHandlers` and `FactorySourceAdmissionHandlers` share admission/replay authority. Work is immutable and owns one Run. Source identity is input-scoped; it cannot serve as the canonical Issue key across pipelines. |
| Source input publication | `FactoryWorkSourcePreparer`, `WorkSourceArtifactStore` and `SourceArtifactQueryStore` reserve exact objects before writes, retain original snapshots/mapping, publish the complete pair and recover ordinary admission intent. This requires an installed input/scan and target; it is not a provider-free manual text service. |
| Retained input reading | `FactoryArtifactReader::read_input` returns verified private bytes and provenance for Build outputs or explicitly installed Source input reads. `read` remains a produced-output/evidence boundary. Source inputs do not fabricate Build/Job/Agent identities. |
| Runtime composition | `FactoryProfileRuntimeInstaller` in `factory_profiles.rs` constructs its input reader without `with_source_inputs`; the Codex reader in `factory.rs` does likewise. The Source reader extension is exercised at the application boundary but is not wired into these runtime constructors. Do not treat those tests as qualification of Source text reaching an installed model pipeline. |
| Compiler input publication | `FactoryExecutionInputPublisher` reserves and publishes trusted compiler documents under an existing Flow Build intent and owner fence. It cannot publish the original task before a Run exists. |
| Signed input transfer | PostgreSQL `ArtifactRecordStore::published_execution_input` already selects private compiler inputs, Source inputs or ordinary Artifacts. Existing manifest/lease/Project/digest checks remain controlling; adding another input provenance must preserve this boundary. |
| Integration authority | `authorize_integration_resource`, `ManagedIntegrationConnections` and the Source/Reporter runtime recheck enabled connections, exact versions/resources and separate purpose grants. Credentials/check success alone grants neither Project access nor read/write/delivery authority. |
| Typed actions | `FactoryFlowActionPlugin` / `FactoryFlowActionAdapter` validate exact installed bindings, declared results, projected input, permissions, original deadline and owner fence. Existing source-update authoring and Reporter effects use this path. A local Issue-status action is not installed yet and must not be confused with the existing external status projection. |
| Profiles | `FactoryCodexBuildProfile`, `FlowBuildBinding`, runtime profile files and `matches_intent` supply exact installed technical execution choices. Editable Project roles/profiles, templates and dynamic profile-selection records remain planned in `add-factory-role-profiles`; enrolled workers are a separate concept. |

Relevant implementation evidence:

- [Source preparation](../../../../server/application/src/factory_source_preparation.rs),
  [retained input reader](../../../../server/application/src/factory_artifact_reader.rs),
  [input selection](../../../../server/application/src/factory_artifact_flow_inputs.rs),
  [compiler publication](../../../../server/application/src/factory_execution_input_publisher.rs).
- [Runtime input composition](../../../../server/app/src/runtime/factory_profiles.rs),
  [Codex composition](../../../../server/app/src/runtime/factory.rs),
  [signed input metadata](../../../../server/infrastructure/octacity-server-store-postgres/src/store/artifact_cache.rs).
- [Integration grants](../../../../server/application/src/integration_project_access.rs),
  [typed actions](../../../../server/application/src/factory_flow_action.rs),
  [source effect authorization](../../../../server/application/src/factory_source_actions.rs),
  [technical profile matching](../../../../server/application/src/factory_codex_materializer.rs).

## One shared Project-text publication and admission seam

The provider-free Project-text service is absent. The existing Source preparer
and node-scoped compiler publisher are useful behavioral references, not APIs
to call with fabricated Source bindings or Build provenance. Implement the
missing operation in work-card tasks **3.2–3.3**, with signed transfer, retained
reads, cleanup and crash recovery qualified together.

The common boundary must:

1. Accept a bounded immutable Project/security-scoped intent with exact task and
   acceptance bytes, input roles, digests, target/repository versions and source
   selection. Issue execution supplies its reviewed revision; quickstart supplies
   its explicit submission. Neither is required to invent the other's identity.
2. Authorize visible dependencies and probe original scoped replay before mutable
   resolution. Reserve request/document/upload identities, recovery ownership,
   hard preparation bounds and cleanup metadata before external effects.
3. Freeze one exact trusted repository subject under that operation's fence
   before byte publication. Reuse the frozen subject for ordinary admission;
   retries do not re-read Issue text, select the latest profile or resolve a
   mutable branch again after the freeze. A client-supplied assertion is not a
   trusted resolution proof.
4. Publish and verify the complete immutable input set through existing byte
   ports. Retain Project-owned provenance, not synthetic `ArtifactRecord` Build
   identities. Keep input reads distinct from produced evidence and authorize
   only manifest-selected signed transfer under the ordinary execution lease.
5. Reuse ordinary admission and recover its original Work/Run before issuing
   any new admission. Issue execution adds durable association before reporting
   admitted; an unknown response never authorizes compensation by deleting Work.
6. Use one supervised publication/admission operation and its bounded recovery,
   retention/holds and replay receipts. Issue association adds its narrow state;
   it must not duplicate the intake journal, byte writer, lifecycle owner or
   scheduler. No text/status edit implicitly starts another execution.

The [design](design.md) now makes the source-freeze order explicit, matching
[quickstart design decisions 2–3](../../add-factory-bugfix-quickstart/design.md).
Before quickstart implementation, reconcile its tasks **3.1–3.3 and 4.1–4.4**
with the delivered shared service, and retain its separate preset/readiness UX.
Those tasks remain unchecked; this inspection does not deliver that service or
complete either change's end-to-end pilot. Preserve existing Source journals
and Work deduplication semantics while adding canonical Issue association.

## Manual tasks, transitions and provider compatibility

Manual Issue/workflow creation and editing require only their visible Project,
bounded content and specific rights. They must not resolve an integration,
Repository, model, installed Factory profile or execution target. Saving a draft
with incomplete acceptance creates no Work/Run. These are requirements of work-card
section 2, not capabilities claimed by the current Artifact-admission tests.

Manual moves, accepted pipeline actions and authorized source pulls must call
one Issue transition boundary (tasks 2.5 and 3.6–3.7). Neither Run completion nor
model prose selects a status. External mappings and independently granted
Reporter effects remain separate (sections 4–5), reusing the existing ordered
outbox instead of a second producer. Pure local moves need no Reporter.

Existing native installation/resource/item identity, read/write grant checks and
Reporter recovery from archived `add-github-work-source` remain the baseline.
The [live gate](../2026-10-18-add-github-work-source/verification-6.2.md)
remains unqualified; local connection setup is not live Source/Reporter execution
qualification. No credential or live provider operation was used here. GitLab,
Jira and custom names in fake conformance tests prove an open shared vocabulary,
not shipped native adapters or prerequisites for manual tasks.

Role profiles remain optional. Retain real installed profile facts where present;
do not infer a role from a stage/worker name or require the role-profile change
before manual Issue creation. Its template/package/selection implementation and
qualification remain owned by that change. No neighboring tasks were completed
or planning files modified.

## Behavioral characterization and verification

Before editing tests, the existing public admission suite passed **87 tests**.
The unavailable-Source test in `factory_source_admission_tests.rs` was then
strengthened: failed Source admission creates no replay/audit or VCS resolution;
manual admission with the same submitted identity succeeds; Source recovery
cannot obtain that manual receipt; manual replay still returns its original
Work/Run, with only one admission/resolution/audit.

The characterization passed against unchanged production code. This task adds
no executable product behavior and performs no production refactor, so there is
no missing-behavior Red/Green cycle to claim. A missing test import was corrected;
that compiler failure is not behavioral Red. New behavior in later tasks still
requires an observed Red before its implementation.

Commands, using Rust/Cargo 1.99.0, the existing `target`, ordinary test parallelism
and unchanged timeouts:

```sh
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2

cargo test --locked -p octacity-server-application --lib factory_admission_tests
# Before and after characterization: 87 passed each.
cargo test --locked -p octacity-server-application --lib an_uninstalled_source_is_denied_before_replay_resolution_or_publication
# 1 passed; also included in the final 87-test suite.
cargo test --locked -p octacity-server-application --lib factory_flow_action_tests
# 20 passed.
cargo test --locked -p octacity-server-application --lib integration_project_access_tests
# 2 passed.
cargo test --locked -p octacity-server-application --lib factory_codex_tests
# 21 passed.
cargo test --locked -p octacity-server-application --lib factory_command_materializer_tests
# 3 passed.
```

The final focused selection is **133 passing tests**. It includes retained Source
publication/uncertain-write recovery, private input provenance, source/manual
identity separation, current independent grants, generic action replay/outbox
isolation and static execution-profile/compiler compatibility. PostgreSQL,
REST and Console baselines remain recorded in [1.1](verification-1.1.md); no SQL,
HTTP or UI behavior changed here and those larger suites were not repeated.

```sh
cargo clippy --locked -p octacity-server-application --lib --tests -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
git diff --check
```

All final checks exit 0; architecture reports 63 packages and 255 dependency
edges. Report links resolve. Disk was checked before and during compilation:
approximately 12 GiB free initially and at completion, with a temporary reading
of 11 GiB during the focused rebuild. Only the existing `target` was used; no
container/image build, cache deletion or stand mutation was needed. No commit,
archive, spec synchronization or neighboring change update was performed.
