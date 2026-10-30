## Why

The triage pipeline stops with an accepted assessment. OctaCity needs the next usable Dark Factory pipeline: take a triaged ticket, resolve reproduction and specification needs, then independently prepare tests, implement, review, validate and publish the resulting pull request for manual merge.

## What Changes

- Add a versioned `ticket-delivery` preset over the existing generic Factory Flow, Role/Profile, Build, protected-test and Delivery boundaries. Enable a separate downstream intake that consumes an exact accepted triage result; triage completion alone does not start development.
- At the triage stage, project `need reproduce` for defects whose reproduction remains unresolved, independently of whether downstream delivery is enabled. The downstream pipeline consumes that triage outcome and runs one deeper reproduction stage; it does not add `need reproduce`. If completed reproduction checks still cannot reproduce the symptom, project `can't reproduce`, remove the managed `need reproduce` label and leave the ticket open without implementation.
- Route triage size `large` through OpenSpec proposal/specification/design/tasks authoring; `small` and `medium` proceed using the ticket's explicit expected behavior. Do not rerun sizing or triage. Missing requirements pause the affected ticket.
- Run acceptance-test authoring in an isolated agent invocation before implementation. Keep its immutable protected test bundle and transcript inaccessible to the implementer; retain developer-visible TDD tests in the implementation candidate.
- Bind AI-agent roles by stage: `Tester` for defect reproduction, test authoring and test execution; `Coder` for implementation and independent review; `DevOps` for Delivery; `Analytic` for requirements/specification authoring. Each stage selects its own exact Profile and isolated invocation, including stages sharing a role.
- Configure each stage's input data separately: repository source visibility, selected predecessor results, requirements, test artifacts and additional configuration files. Reuse explicit Context projections and scoped workspace/tool grants; a role or control dependency never implies access to all inputs.
- Deliver and verify permission isolation per stage through existing runtime enforcement: workspace and Artifact access, read/write paths, tools/commands, network and credentials. Context omission and prompt instructions alone do not enforce isolation; extend the shared runtime where necessary to provide the required restrictions.
- Allow configured human acceptance of prepared requirements/specifications for combinations of triage component, size and priority. Other stages proceed automatically; approval applies only to the exact requirements presented.
- Execute implementation or defect fix, one independent code review, deterministic validation using the previously prepared tests, and trusted PR publication. Findings may return to implementation within one configured rework budget.
- End Delivery with `delivered_for_review` and the PR reference. DevOps prepares and observes publication through scoped tools and existing trusted Delivery operations. A human merges the PR in the forge, including into production branches. Merge operations, automatic merge and a merge-approval stage are outside this change; the existing publication-only Delivery protocol remains unchanged.
- Reuse preset setup and Run diagnostics to show the chosen path, selected AI agents and stage inputs, human requirements acceptance, test/review results and PR delivery outcome. Do not add reviewer quorums, model-scored gates or repeated readiness checks between stages.

## Capabilities

### New Capabilities

- `server/factory-ticket-delivery-preset`: Post-triage handoff and label policy, stage roles and configurable inputs, conditional reproduction/OpenSpec/human acceptance, isolated tests, implementation/review/validation, delivery composition and operator visibility.

### Modified Capabilities

- `server/dark-factory`: Make specification bypass bounds and a separate requirements-review invocation explicit Flow policy so this preset can use triage size and one later code review.

## Impact

Reuse `config/factory/flows/base-flow/`, `server/application/src/factory_presets*`, requirements/protected-test/review handlers, Issue execution and Source/Reporter contracts, Delivery protocol/runtime/store, `octacity-github`, REST/OpenAPI and Console preset/run views. Extend these owners only where the supported journey lacks behavior; do not add a second engine or lifecycle store.

Depends on the accepted-result and reporting contracts of `add-factory-triage-pipeline`; this change owns the downstream handoff and `need reproduce` projection extension without editing the sibling change. `add-factory-bugfix-quickstart` remains independent. Merge automation, deployment, feature research, new providers and production promotion are out of scope. Implementation must follow repository behavioral Red/Green TDD; this change creates planning artifacts only.
