## Context

See [proposal.md](proposal.md). Tasks 1–9 provide Project fields, guarded classification writes, Issue materials, snapshot admission and UI triage/Delivery. The remaining hardcode is the fixed classification-role list and its required result names. A manual source-to-target mapping editor would add unnecessary configuration when the agent is simply filling known Issue fields.

Existing foundations are sufficient: typed Project catalogs, accepted results/projections, trusted Issue action/transaction/replay owners, ordinary Flow gates, CI/CD report inputs and a provider-neutral Decision Signal port. Reuse these. There are no released old publications to support; replace the affected development formats/fixtures directly. New-model history, frozen inputs and idempotent receipts still survive normal edits/restart.

## Goals / Non-Goals

**Goals:** Any agent step can fill a selected supported set of Issue fields from Project definitions, independently of pipeline/role names. Select fields once, derive one typed contract and save through the existing owner. Keep consumers configurable and safety/provenance intact.

**Non-Goals:** Manual result-to-Issue mapping, arbitrary JSON/nested result import, multi-source field merging, casts/converters or clear-operation editing; a new workflow/policy/template engine, global semantic-role registry, write-capable agent tool, value store, provider adapter or qualification framework; unrelated cleanup, automatic merge/deployment or legacy format support. Existing Flow data/admission bindings remain unchanged in purpose.

## Decisions

### 1. Project field identity is the shared contract

Keep core priority/component/type/size and supported custom text/choice/boolean/bounded-integer/date fields. Project definitions supply stable IDs, human labels/descriptions, types, bounds, choices/order and retirement. Defaults seed choices, not Issue values. A field called risk or approved remains data; there is no global business-role registry.

A step stores a bounded selected-field set and exact definition references, with required/optional handling. The server derives one field-form schema and readable descriptors from that selection. Generated value keys represent stable field identities, not arbitrary agent aliases. The same selection determines allowed writes; operators do not define an output schema or duplicate its enums/targets. Display/step/outcome renames do not change field identity. Catalog edits require explicit reconciliation rather than silently rewriting a published form.

Use one canonical catalog/provenance/snapshot representation and update affected fixtures directly. Remove old fixed-slot/numeric-priority/fixed-three-size and omitted-default compatibility branches where superseded; retain selected choice definitions uniformly, including configured defaults.

**Alternative rejected:** A larger semantic-role enum or a mandatory mapping table. Neither is needed to fill fields whose identities and types are already known.

### 2. Agent proposes field values; the existing trusted writer applies them

Enable **Fill Issue fields** on a declared successful result of an ordinary agent step. Selection generates the structured result internally and authorizes only that selected write set. The agent receives its task, ordinary selected inputs and field descriptors and returns values in that generated form. Free text asking to fill fields grants no write capability by itself.

After normal result acceptance, automatically configure/invoke the existing trusted Issue action path for those same field identities. Reuse its record/replay mechanics; do not add a direct mutable MCP write tool, special triage executor, new scheduling loop or user-authored apply node. Any technical internal projection/action binding is generated from the field set, not a public source-path mapping API or a second operator schema.

The controller supplies the current Issue association, invocation baseline and exact form/catalog references. Capture the baseline before analysis; the model cannot choose the Issue, widen the field set or replace that baseline. Require the accepted form result for that invocation, reject unknown/unselected fields and retain actual Run/node/attempt/accepted-result provenance. A random accepted object of another schema is not imported into Issue fields.

All selected fields are required by default; existing required/optional controls can make individual fields optional. Missing optional fields preserve their values. Invalid values or missing required fields reject application atomically. No implicit defaults, casts or clearing from missing/null input; explicit clear/import/merge operations are outside this scope.

The accepted analysis result and application receipt remain distinct facts. Report fields as saved only after the guarded transaction succeeds; conflicts/pending/denied writes are visible even when analysis succeeded. Retrying a lost save acknowledgement resumes the original accepted result and operation receipt, not another model call or new Issue revision.

**Alternative rejected:** Infer fields from prose, let the model write arbitrary Issues, require manual producer/path/destination rows, or ask the operator to build a separate apply step for the standard path.

### 3. One owner validates and commits

Generalize the existing Issue mutation path, not a second writer. Reuse one domain validation implementation for writable destinations, Project types/bounds, current authority/source ownership, exact definitions, requirements/selected-field baselines and accepted provenance across stores. Commit values, field origin/applicability, history and scoped receipt atomically. Preserve unselected fields and reject conflicts without partial mutation. System identity, workflow/execution/authority state, credentials and evidence records retain their separate typed commands.

Generate immutable field/type structure at publication. At invocation/application recheck only facts that can change: authorization, field availability/compatibility, Issue baselines, required observations and accepted source identity. Callers reuse the common result; UI checks remain advisory. No repeated independent field-semantic validators in several layers; reads do not mutate or renew assessments.

**Alternative rejected:** Keep the fixed-slot writer as a legacy adapter or duplicate validation/store logic. There is one current contract.

### 4. Inputs and decisions remain composable

The agent can analyze ordinary accepted inputs before filling its form. In particular, CI/CD -> retained report -> agent analysis -> Issue fields uses existing Build/result/Artifact input paths. Configure routing for passed and failed test reports explicitly. Missing reports/infrastructure failures are distinct from a test verdict and cannot be fabricated into passing data. No new CI integration is required.

Existing Decision Signal nodes can consume declared report/context projections and select a configured route before analysis or after field completion. JEV and OpenAI remain adapters behind the existing neutral port, with their installed capabilities, rollout/fallback and evidence/authority limits. Finite-choice evaluation is not a general text/date/integer field filler. This change adds no provider adapter, live call or provider-specific write path; integration tests use the existing finite provider fixture.

Consumers select direct Issue fields, local input aliases, required/optional values, applicability/evidence and explicit materials. Existing scheduling/risk conversions and gate predicates remain configured execution mechanisms. Field filling does not add a result-to-Issue mapping facility or turn a scalar into proof. Reuse trusted observation readers/subject/criterion/environment checks for declared proof-dependent paths; protected validation/publication retain independent qualified records.

Finite-choice thresholds use the selected field and reviewed order to compile existing concrete gate rules. No shared code recognizes requirements_needed, a special size string or a triage/Delivery name. Human approval remains a separate configured gate. Freeze exact definitions, field values, materials, evidence and policy decisions once at admission; preserve per-step access and existing material bounds/owners. Later edits do not replace admitted snapshots. Explicit pipeline-to-pipeline accepted-result input remains a supported current mode.

**Alternative rejected:** Couple the fill-fields operation to CI report keys, one decision provider or a new rules framework. Existing inputs and decision steps already provide composition.

### 5. Templates and UI use the same field selector

Triage and Delivery are optional ordinary Flow configurations. Template data selects field sets, ordinary inputs, conditions/thresholds, agents and existing outcome display bindings. Extend current typed parameter metadata only as needed; shared discovery/forms/resolution do not switch on template or node names. The stand supplies the common trusted field-save capability and configured permissions without activating any business effect.

In an agent step the operator enables **Fill Issue fields** and selects Project fields once. Show labels, supported types/choices, required/optional controls and the exact save scope at publication. The generated result contract and automatic save association are technical details, not separately editable schema/mapping forms. No source-path picker, transform controls or manual apply node is required. Existing free-form result authoring remains available for other purposes without implicitly writing an Issue.

Project settings have no fixed classification roles. Reopen, step/field display renames and catalog conflicts retain selected identities and the draft. Displaying outcomes and Reporter effects remain separate opt-ins. An accepted analysis with an unsuccessful save is not shown as applied. No new presentation framework or second endpoint family is introduced. Replace superseded formats/fixtures rather than keeping legacy modes.

**Alternative rejected:** Expose the generated transport binding as mandatory user work, or retain two business-specific editors.

### 6. Proportionate qualification

At each changed public boundary run the relevant baseline and one meaningful Red/Green cycle, then directly affected regressions. Use PostgreSQL for persistence changes, schema/client checks for API changes and component/affected browser cases for UI. Do not duplicate an invariant at every layer or rerun unrelated/full workspace suites after every edit; run broader affected integration once when ready, and repeat only after a change/failure warrants it.

Use the existing harness for one unrelated CI-analysis pipeline: create supported custom scalar fields without roles, feed a retained successful/failed CI report to the agent, fill the selected fields and start an independent snapshot consumer. Rename/reopen and verify stable identities/provenance. One finite Decision Signal fixture chooses an analysis/review route without provider-specific field writes. The UI scenario configures no source/target mapping, duplicate schema or apply node; test-side APIs do not seed business definitions.

Adapt the existing ordinary-stand triage/Delivery journey for final protected test/review/publication checks without another qualification framework or repeated native PR runs per slice. Record actual results and unavailable checks honestly.

**Alternative rejected:** Prove universality only with the two templates, or add a broad verification project instead of the focused behavioral cases above.

## Risks / Trade-offs

- **Implicit save scope becomes too broad** -> Explicit opt-in and selected identities generate both the form and permitted write set; the trusted owner rechecks authority/baselines and commits atomically.
- **Generated field contract diverges from storage** -> One Project-derived contract/selection is reused for generation, accepted result and application; reject stale/incompatible forms instead of silently remapping.
- **Analysis or Decision Signal is mistaken for a trusted CI verdict** -> Keep accepted assessment, saved data and independently qualified observations distinct and reuse current proof-consuming checks.
- **Old development definitions use the replaced format** -> Update fixtures/templates and explicitly recreate/re-publish development definitions as necessary; no adapter or automatic destructive stand reset.

## Implementation sequence

1. Introduce the common selected-field/form contract and reuse Issue persistence/validation.
2. Connect ordinary agent result acceptance to automatic guarded save, then current inputs/conditions/templates.
3. Publish one REST/client contract and wire the common field selector and stand capability configuration.
4. Exercise CI analysis/Decision Signal composition and the existing triage/Delivery journey, then update docs for the actual controls.

Manual result-to-Issue mapping and old-format compatibility are not deferred implementation tasks in this change. Completed tasks/reports remain historical; section 10 tracks the pending correction only.
