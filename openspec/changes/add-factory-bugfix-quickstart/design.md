## Context

See `proposal.md` for motivation and `specs/server/factory-quickstart/spec.md` for the behavior contract. This is a separate follow-up to the in-progress `add-dark-factory-mode` change, whose generic runtime, production composition, and qualified execution path are prerequisites. Its unfinished release work is not transferred into this proposal.

Observed implementation boundaries:

- `FlowDefinitionTemplate::from_manifest` and `instantiate` already resolve bounded declarative configuration into immutable definitions. `config/factory/flows/base-flow.json` and its parts provide configured defect research, accepted contracts, protected tests, implementation, review, and verification; their names select no execution behavior. The examples default to deny-all and require actual installed authority/profile choices.
- `factory_configuration_cqrs.rs` and `factory_configuration_choices.rs` provide authoritative validation, normalized exact references, bounded visible choices, permission narrowing, immutable publication, and replay.
- `AdmitManualFactoryWorkCommand` and `factory_admission.rs` freeze exact Configuration/Repository versions and source revision and use scoped replay before mutable source resolution. Work currently contains `FactoryArtifactReference` values for task and acceptance.
- `FactoryArtifactReader` currently obtains input ownership from an ordinary producing Build. `FactoryExecutionInputStore` is for compiler documents attached to an existing fenced node intent. Neither is a valid manual-upload provenance boundary.
- `FactoryWorkForm.tsx` selects task and acceptance Artifacts from prior Builds. `FactoryPublication.tsx` ties a validation preview to an exact draft, and shared confirmed-intent handling preserves request bodies, idempotency, and version conflicts.
- `server/app/src/readiness/mod.rs` separates deployment and optional Factory dependency health. It does not expose a full exact-Configuration readiness report or establish Agent-local permission enforcement.

## Goals / Non-Goals

**Goals:**

- On a prepared deployment, configure one supported bug-fix journey using visible choices and start it from description plus acceptance text without graph/schema editing or prerequisite Builds.
- Add convenience at publication and admission boundaries while retaining one interpreter, one Build path, and ordinary Agent enforcement.
- Make unknown results, partial input publication, and stale readiness explicit and recoverable.
- Define a released headless and Console pilot with fault injection and an enforceable guarantee matrix.

**Non-Goals:**

- Installing infrastructure, provisioning Agents, acquiring credentials, broad document uploads, server-side editable drafts, or an alternate local executor.
- Delivery/PR creation, merge, deployment, Decision Signal calibration, additional presets, or an autonomous planning agent. An accepted candidate remains available through existing diagnostics and Artifact access.
- Changing shared JobSpec formats, treating a browser confirmation as a successful execution, or making quickstart a prerequisite for CI/CD.

## Decisions

### 1. Publish a preset through the existing immutable configuration path

Ship a `bugfix` preset manifest and its bounded parts under `config/factory/presets/`, inventoried with the release. Its identity includes name, version, content digest, and the complete dependency closure. Reuse the existing manifest resolver and template instantiation; add no preset-specific interpreter or permanent stage fields.

Expose parameter slots as bounded declarative data: permitted Repository selection, execution-profile and pool choices, Project policy selections, and explicit hard-budget/WIP settings. Use installed compatible choices; never infer a grant from an alias, node name, model suggestion, or task text. An absent required choice yields a blocker rather than a permissive default. The first release uses the concrete Codex capability that must be qualified by the underlying Factory change, with independently scoped author/implementer/reviewer profiles.

Resolution produces ordinary typed drafts and validation diagnostics without side effects. Publication uses existing commands and revalidates live policy and installed capabilities. Add optional typed preset-binding metadata to Configuration publication and retain it with the exact Configuration version. Recompute the canonical expected instantiated closure and allowed substitutions server-side; a caller-provided preset name or digest cannot certify an edited graph. Compatible Configuration discovery verifies that retained binding. Advanced custom definitions remain supported without being advertised as the supported bug-fix preset.

The preset's behavioral contract includes bounded defect reproduction, accepted requirements, protected regression authoring, trusted original-subject regression validation, implementation, exact capture, protected candidate validation, independent review, and required Project verification gates. Missing reproduction, an already-fixed defect, uncertainty, failed checks, and exhausted budgets use declared gates and human/terminal outcomes. The original regression must fail for the specified missing behavior; infrastructure failure is not Red. Hidden tests and their authoring Context never enter implementation. Writable work starts only after the baseline gate. The successful terminal retains the accepted candidate and reports that no external delivery was performed.

Alternative considered: create a special `bugfix` lifecycle or have an LLM author the initial graph. Both would introduce extra authority and maintenance cost; a verified immutable preset already fits the generic runtime.

### 2. Preserve Work references while introducing truthful manual-input provenance

Implementation handoff: reuse the [shared Project input service](shared-project-inputs.md) delivered by work-card 3.2–3.3. Its application, PostgreSQL, runtime-reader, protected-transfer and cleanup contracts replace a second quickstart publication implementation; the remaining preset/readiness/HTTP qualification and stricter ingress bounds are recorded there.

Keep Work's bounded task/acceptance reference shape and the existing signed input identity contracts. Add narrowly scoped managed Work-input records rather than synthetic Build outputs. Each record binds an Artifact-compatible logical identity and byte-store publication identity to its owning Project, text-submission identity, role (`description` or `acceptance`), exact UTF-8 digest/length, publisher kind `manual_work_input`, and retention. These roles describe documents, not Flow stages.

Extend the retained Factory input source at the application boundary to return a verified source projection for either an existing Build Artifact or a managed Work input. Preserve existing Build ownership and provenance checks for the former; verify Project/submission ownership and publication/retention for the latter. Never manufacture an `ArtifactRecord` with false Build/Job/Agent identities to satisfy the current reader return type. Use the same bounded byte reader/publisher ports and object backend. New input records do not qualify as deterministic evidence or Agent output.

Integrate these inputs with explicit Context selection, compiler input materialization, signed managed-input transfer, capability minting, and retention. The Agent receives only manifest-selected digest/size identities through the existing v3 protocol. Do not add an arbitrary download path or allow control edges to grant input access. Project visibility alone is insufficient to mint a Job transfer grant.

Description and acceptance are plain UTF-8, each nonblank and at most 16 KiB. Preserve submitted bytes after validation: no HTML interpretation, Unicode rewriting, truncation, shell interpolation, URL retrieval, or extraction of authority from prose. Reject unsupported control content according to the published closed text schema. Their aggregate must also fit existing Context and protected-input bounds. Title/external identity, source policy, classification, and priority retain current bounded contracts; conservative reviewed Project defaults do not silently classify a defect as low risk.

Alternative considered: run a dummy Build to publish documents or reuse node-scoped compiler-input rows before a node exists. Both fabricate causality; a narrow provenance-aware source is required.

### 3. Use one recoverable text-intake command over ordinary admission

Introduce operation-shaped text-intake ports in the existing store crate and parity implementations in memory and PostgreSQL. The submission journal is a technical publication operation, not a second Factory lifecycle. It has one immutable request fingerprint and states sufficient to distinguish reserved, input-published, admitted, and failed/expired preparation. Its hard deadline, attempt ceiling, byte/storage quota, and cleanup deadlines are deployment-policy bounds, separate from the eventual Run budget and never renewed by retry.

Process the command as follows:

1. Authorize the owning Project and every selected visible immutable dependency before handlers perform resolution, reservation, or byte I/O. Probe scoped completed/pending replay before resolving mutable catalogue or source state.
2. For a fresh intent, validate the complete request and current admission/readiness prerequisites. Reserve the immutable request, stable document/upload identities, recovery ownership, and cleanup metadata durably before writing bytes. Retain private submitted text only in protected authoritative intake storage required for recovery, subject to the same input retention; never log it.
3. Resolve the allowed repository selection through the existing VCS boundary and freeze one exact subject under the current reservation fence before publication. After the freeze, recovery uses that subject without resolving the branch again. Loss before the freeze creates no accepted base or Run; another bounded owner may retry resolution. Competing resolvers cannot overwrite the first committed freeze.
4. Publish both exact reserved documents through idempotent immutable byte writes, then independently verify lengths/digests and atomically mark the complete pair visible to the owning submission. Unknown write responses cause observe/verify-before-retry, not another logical input.
5. Invoke the shared ordinary Work admission authority using the frozen exact revision and verified input references. Do not call a fresh mutable-source selection internally or generate a different external Work identity. Atomically bind the ordinary admission outcome, input retention, submission receipt, audit facts, and scoped replay result; an admission that committed before receipt acknowledgement must be discovered and attached without a duplicate Run.

The admission seam can be factored to accept a previously resolved trusted source inside the application, while both public Artifact and text commands retain authorization, enabled-policy, deduplication, and immutable-bound checks. No client may claim its own source-resolution proof. Pending recovery revalidates current permission and enforcement availability while preserving frozen input; loss of a dependency waits/fails safely within bounds rather than substituting profiles or rights. Completed replay returns the recorded outcome even if current readiness changed.

Return the completed ordinary admission result when possible. If a bounded request window ends after reservation, return HTTP 202 with a safe stable submission receipt. A bounded read returns pending/admitted/failed state and, after admission, ordinary Work/Run identities; it exposes no raw text. Existing worker supervision owns fenced bounded recovery, shutdown, and cleanup. An explicit HTTP client replay with the same intent remains supported if the first response itself was lost.

Retention reserves cleanup metadata before external I/O. Pending inputs cannot mint execution capabilities. Abandoned reservations expire; admitted inputs are referenced by existing Factory retention and hold semantics. Cleanup must serialize or fence its final eligibility check against admission/hold binding, never delete a document between verification and the atomic admission link, and preserve replay receipts for the normal idempotency window without retaining task bodies indefinitely. Restart and backup/restore include the journal and input metadata.

Alternative considered: two visible commands to upload text and later admit Work. A single journaled command removes manual Artifact handling and makes partial completion explicit while retaining the existing admission owner.

### 4. Readiness reports reusable authoritative facts, not execution permission

Add an application query over an exact Configuration/Repository pair. Obtain required capabilities by traversing its frozen reachable Flow/profile closure, rather than enumerating business phases. Share the authoritative semantic-capability/policy matching boundary with placement/admission; never reproduce policy in the UI. Existing global health remains separate.

Use bounded read-only facts: configuration validation/enablement, source-selection policy, allowed pool policy, current worker/dependency health, signing/storage configuration health, fresh verified Agent inventories including pinned executable/plugin identity and qualified enforcement capability, and configured credential-profile references. This query resolves no credential material, executes no repository command, makes no model/forge request, and creates no publication, claim, budget reservation, or mutation-audit fact. Normal redacted request telemetry is still permitted. Metadata cannot prove a live provider credential works; expired or unavailable required health proof is reported `unknown` rather than replaced with a probe that transmits secrets or task content.

Each required check contains a stable code, `ready|blocked|unknown`, observed time/expiry, and an authorized remediation target. The overall report expires at the earliest required observation expiry. Busy compatible capacity is a queue-delay warning; absent, drained, unqualified, or stale required capacity is a blocker/unknown. Readiness covers all required reachable node profiles, not only the first node. No JEV, delivery, or deployment requirement is synthesized when the selected closure omits it.

The report is a snapshot: it cannot promise immediate placement or certify Agent-local grants at a future spawn. Fresh text intake repeats current required prerequisites; placement, signing, protected-input access, and Agent pre-source/pre-spawn checks remain unconditional. A required fact disappearing after admission can leave a Job pending or rejected, without Host fallback or permission widening. `ready` is never included as a trusted command field.

Alternative considered: expose only deployment `/ready` or run a live diagnostic Build. Neither provides the requested configuration diagnosis without extra authority or side effects.

### 5. Keep the additive API narrow and available headlessly

Proposed REST paths below `/api/v1`:

| Method | Path | Contract |
| --- | --- | --- |
| GET | `/projects/{project_id}/factory-presets` | Bounded visible preset metadata and typed parameter choices |
| POST | `/projects/{project_id}/factory-presets/resolve` | Read-only preset resolution and normalized ordinary drafts |
| POST | `/factory-configurations/{id}/versions/{version}/readiness` | Read-only report; body selects exact Repository version |
| POST | `/projects/{project_id}/factory-text-work` | Idempotent journaled text intake; completed result or pending receipt |
| GET | `/projects/{project_id}/factory-text-work/{submission_id}` | Visibility-safe bounded receipt observation |

Existing Flow/Configuration publication and Artifact-based `/factory-work` remain the execution/publication APIs. Optional typed preset-binding metadata is additive to Configuration publication; ordinary clients need not supply it. Read-only POST queries accept bounded bodies but allocate no idempotency mutation outcome. Publish all schema bounds and receipt states in OpenAPI, authorization inventory, and generated Console clients. Do not overload the existing Work DTO with an ambiguous text/Artifact union or require changes to existing clients.

Mutations use the existing management security scope, immutable confirmed request, idempotency key, version preconditions, and same-transaction audit patterns. Required actions for combined intake include permission to publish managed Work inputs and execute Work in the selected Project; publishing a Flow/Configuration retains its ordinary separate permissions. Remediation targets and Agent facts are visibility-filtered before projection. Trust-network mode does not imply a new login flow or public-network deployment.

### 6. Add progressive Console entry points over existing components

The first-time journey is **Project -> Fix a bug -> profile/pool/budget setup -> validate/review/publish -> description and acceptance -> readiness -> review/start -> Run**. Later tasks reuse a compatible published Configuration. Existing reference pickers, Configuration validation/publication, query-state notices, and confirmed-intent helpers provide the boundaries; add a preset-specific setup presentation without replacing advanced graph editing.

Text submission shows plain text and byte counters. Readiness is associated with exact selected versions and invalidated on subject change or expiry. The confirmation names Project, Configuration, Repository/source selection, budget, and accepted-candidate stopping point. A mutable source selector is labeled as resolved at admission; the returned exact revision is displayed only once frozen. UI defaults come from the resolved policy, not client-invented rights or risk values.

Pending intake shows preparation rather than execution and follows the same server receipt using bounded backoff while active. In-memory retries reuse the frozen body/key; changing text or selection requires a new reviewed intent. A lost response is not a safe reason to issue a new identity. Unconfirmed drafts are discarded on reload; do not persist text, payloads, keys, or receipts in browser storage. Receipt resource identities may be addressed through ordinary navigation without putting text in URLs. The existing Run view supplies all later diagnostics and preconditioned commands.

### 7. Validate the slice through public behavior and preserved guarantees

Every observable implementation, preset/configuration change, and migration follows repository-mandated test-first Red/Green slices. Existing behavior runs first before a shared-seam refactor, with characterization where coverage is missing. Environment failures do not establish Red.

| Guarantee | Required independent verification |
| --- | --- |
| Ordinary immutable definitions | Headless resolve/publish; preset replacement leaves an admitted Run's closure unchanged |
| Meaningful TDD and hidden tests | Exact original subject fails the protected regression for the specified defect; corrected candidate passes; implementation Context/grants exclude private tests and transcripts |
| Exact source and replay | Advance the branch after admission; replay still returns the original base, inputs, and Run |
| Truthful input provenance | Managed text reads work without a Build; text cannot satisfy an execution-evidence criterion |
| Scoped input authority | Cross-Project read, unsigned transfer, digest/size drift, and control-edge-only access are denied |
| Bounded publication and retention | Fault injection after reservation, either object write, input publication, and admission; stale-owner recovery and cleanup/admission races preserve one result and required bytes |
| Readiness remains an observation | Expire inventory or remove enforcement after ready; no unsafe admission/dispatch and no Host fallback |
| Failed evidence remains failed | Missing, stale, indeterminate, or failed required verification cannot be accepted through a preset or model claim |
| Operator concurrency | Existing stale Node/Run/candidate approval conflicts after quickstart admission |
| Privacy and visibility | Task text/credentials never appear in reports, receipts, URLs, logs, analytics, hidden-resource counts, or browser persistence |
| Independent product modes | Artifact-based Factory journey and ordinary CI/CD pass with quickstart absent, disabled, and enabled but unused |

Use existing owners' unit/service contracts, `server/app/tests/factory_rest_journey.rs`, `factory_configured_flow.rs`, retained-input tests, and Console component/browser tests. Add a released PostgreSQL/object-store/qualified-Agent pilot that starts with no task-producing Build; deterministic fake model ports prove orchestration, while real Git/input/backend checks establish execution-boundary evidence. A live model smoke test, where credentials are provided, supplements rather than replaces that evidence.

## Risks / Trade-offs

- **Prepared deployment dependency** -> Missing release-qualified Factory composition is a release blocker owned by `add-dark-factory-mode`; quickstart cannot mask it with a fake readiness pass.
- **Preset setup still needs installed choices** -> Provide typed compatible choices and actionable blockers; this slice does not install or invent authority.
- **New input provenance touches transfer and retention** -> Keep ordinary Build Artifact checks intact, introduce a narrow source projection, and qualify reads/transfers/cleanup before text intake is exposed.
- **Object writes cannot be atomic with PostgreSQL** -> Reserve before I/O, independently verify immutable writes, journal bounded fenced recovery, and atomically link complete inputs to ordinary admission.
- **Readiness may change or have incomplete proof** -> Publish freshness and unknown states, distinguish busy capacity, and revalidate at admission and actual execution.
- **Additional baseline regression gate can find unusable tests** -> Route infrastructure failures and invalid tests to bounded correction or human input; never classify these as verified defect failure.

## Migration Plan

1. Verify the underlying Factory dependency and release evidence before enabling the pilot. Add reentrant additive migrations for preset provenance, text-submission reservations/receipts, managed-input ownership and retention links; do not rewrite existing Run or Artifact rows.
2. Deploy application/store/readiness contracts with optional quickstart wiring disabled by default. Inventory the preset and publish new OpenAPI/generated clients while maintaining old request/response compatibility.
3. Enable one prepared pilot Project only after headless, Console, restart/fault, privacy, and qualified-backend checks pass. New publication creates ordinary versions; it does not migrate active Runs to a new preset.
4. To roll back the feature, disable new preset resolution and text intake while keeping accepted submissions, managed-input reads/transfers, recovery/cleanup, receipt observation, and active Runs supported. Do not drop provenance tables or downgrade below the managed-input-capable version while reservations or retained Runs still depend on them.
5. Sync these deltas only after implementation and their prerequisite specs are accepted. Keep this change additive; do not silently sync or archive `add-dark-factory-mode` during planning.
