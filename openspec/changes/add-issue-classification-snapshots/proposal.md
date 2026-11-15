## Why

Independently authored pipelines must cooperate through canonical Issue fields and immutable reviewed inputs. Tasks 1–9 implement that path, but fixed classification roles constrain authoring. The correction lets any agent step fill selected Issue fields using their Project definitions, with no manual result mapping or duplicate schema setup.

## What Changes

- Keep Project definitions as the source of supported field types, choices, bounds and ordering. Priority, component, type and size remain built-in; supported custom text/choice/boolean/integer/date fields require no semantic-role registration. Defaults are configuration, not pipeline code.
- Add **Fill Issue fields** to an ordinary agent step. The operator selects fields once; their stable identities and exact catalog definitions generate the agent's typed field form and the server's permitted write set. Required/optional flags use existing controls. No source-path picker, separate result schema, target mapping or manually authored apply step is required.
- After a successful accepted field-form result, reuse the common trusted Issue writer for atomic values, per-field provenance, applicability/conflict checks and scoped receipts. The model supplies values, not the Issue identity, permissions or arbitrary write targets. Omitted optional fields stay unchanged; invalid or missing required values reject the selected application.
- Leave arbitrary result-to-Issue import/transformation, nested-source mapping, multi-source merge and clear-operation editing outside this change. Existing Flow input projections, admission value conversions and explicit pipeline-to-pipeline accepted-result inputs remain current capabilities, not a new Issue mapping feature.
- Keep consumers explicit: select direct Issue fields, local aliases, existing admission/evidence checks and ordinary gate conditions. Familiar names or values confer no authority. Freeze selected values, definitions, materials and policy decisions once at admission.
- Keep triage and Delivery as optional declarative templates over the same Flow mechanisms. Reuse existing typed parameters, gates, trusted observations and input/transaction owners; build no new workflow engine, policy framework or mapping language.
- Support CI/CD report -> agent analysis -> selected Issue fields through existing accepted inputs, including configured failed-test routes. Existing provider-neutral Decision Signal nodes can choose a branch before/after analysis through installed JEV/OpenAI adapters; add no provider-specific field-write behavior, new adapter or live-provider experiment.
- **BREAKING:** There are no released old publications requiring support. Replace affected fixed-slot formats, obsolete examples and fallback encodings; update development fixtures without legacy adapters or dual readers. New-model history, immutable snapshots and exact replay remain correctness requirements.
- Qualify through the common UI with an unrelated CI-analysis pipeline using arbitrary Project fields, then adapt the existing triage/Delivery journey. Preserve isolated tests, independent review and trusted publication with proportionate focused checks.

## Capabilities

### New Capabilities

None. Extend existing owners and editors.

### Modified Capabilities

- `server/factory-work-collaboration`: canonical selected-field completion, guarded application, attribution/applicability and owner-scoped materials.
- `server/dark-factory`: agent field forms, ordinary accepted inputs, Issue snapshots, configurable conditions and Decision Signal composition.
- `server/factory-triage-preset`: optional triage template selecting Issue fields rather than fixed roles/result mappings.
- `server/factory-ticket-delivery-preset`: configurable Delivery template over snapshots, existing gates and trusted validation/publication.
- `server/rest-control-plane`: one closed selected-field/form-result contract and authoritative Project capability discovery.
- `ui/operator-console`: one field selector and automatic typed form, direct consumer settings and ordinary template authoring without JSON or duplicate schemas.

## Impact

Update current Issue field/provenance and PostgreSQL contracts, agent result acceptance and trusted save integration, template/stand configuration, REST/generated clients and existing Console forms. Reuse one value store and authoritative validation path. Manual result-to-Issue mapping APIs/editors, a new write-capable agent tool, arbitrary nested Issue JSON, automatic merge/deployment and new provider integrations are outside scope.

The predecessor delivery change is archived. Completed tasks 1–9/reports retain their historical scope; section 10 is pending. There is no backward-compatibility requirement for development formats. Use focused behavioral Red/Green at each changed boundary and broader affected regressions at integration completion, not repeated full-workspace checks for every slice.
