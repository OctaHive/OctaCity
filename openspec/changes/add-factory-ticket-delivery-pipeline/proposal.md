## Why

The triage pipeline stops with an accepted assessment. OctaCity needs the next usable Dark Factory pipeline: take a triaged ticket, resolve reproduction and specification needs, then independently prepare tests, implement, review, validate and publish the resulting pull request for manual merge.

## What Changes

- Make ticket delivery buildable from an empty graph in **Add pipeline**, using ordinary Factory steps, Role/Profile bindings, Builds, protected tests and Delivery. Supply a versioned `ticket-delivery` example through the existing preset resolver as an optional starting point for the same editable graph. An explicit Issue execution or separately enabled downstream intake binds an exact accepted triage result.
- When the operator configures Reporter for the accepted triage outcome, project `need reproduce` for defects whose reproduction remains unresolved, independently of whether downstream delivery is enabled. The downstream pipeline consumes that triage outcome and runs one deeper reproduction stage; it does not add `need reproduce`. If completed reproduction checks still cannot reproduce the symptom, retain that outcome and, when configured, project `can't reproduce`, remove the managed `need reproduce` label and leave the ticket open without implementation.
- Route triage size `large` through OpenSpec proposal/specification/design/tasks authoring; `small` and `medium` proceed using the ticket's explicit expected behavior. Do not rerun sizing or triage. Missing requirements pause the affected ticket.
- Run acceptance-test authoring in an isolated agent invocation before implementation. Keep its immutable protected test bundle and transcript inaccessible to the implementer; retain developer-visible TDD tests in the implementation candidate.
- Let operators supplement Tester-authored tests from the test step in Console. Select uploaded files or a directory, or files from an additional Project repository. Set the Octafile tasks or shell entrypoints and optional Tester instructions. Retain all selected inputs and execution settings with the generated tests.
- Bind AI-agent roles by stage. Tester handles defect reproduction, test authoring and optional orchestration of test execution. A Flow may instead invoke the same protected runner directly through an ordinary Build. Coder handles implementation and independent review, DevOps handles Delivery and Analytic handles requirements/specification authoring. Each agent stage selects its own exact Profile and isolated invocation.
- Keep each assignment and expected result in its Flow step. Select the original Work task, exact Project documents, accepted predecessor fields and code scope independently per step. Reuse Context projections and scoped workspace/tool grants. Project launch policies and access profiles govern the published Configuration.
- Deliver and verify permission isolation per stage through existing runtime enforcement: workspace and Artifact access, read/write paths, tools/commands, network and credentials. Context omission and prompt instructions alone do not enforce isolation; extend the shared runtime where necessary to provide the required restrictions.
- Allow configured human acceptance of prepared requirements/specifications for combinations of triage component, size and priority. Other stages proceed automatically; approval applies only to the exact requirements presented.
- Execute implementation or defect fix, one independent code review, deterministic validation using the previously prepared tests, and trusted PR publication. Findings may return to implementation within one configured rework budget.
- End Delivery with `delivered_for_review` and the PR reference. DevOps prepares the public PR metadata in one isolated invocation. The scoped trusted Delivery action publishes, observes and retains the receipt. A human merges the PR in the forge, including into production branches. Merge operations, automatic merge and a merge-approval stage are outside this change; the existing publication-only Delivery protocol remains unchanged.
- Use the common Flow editor and Configuration publication, then qualify a manual Issue execution before testing automatic intake. Use Run diagnostics for execution details and opt-in Issue outcome cards for concise specification, review and confirmed PR results. Keep external label actions separate from card display and execution routing.

## Capabilities

### New Capabilities

- `server/factory-ticket-delivery-preset`: Post-triage handoff and label policy, stage roles and configurable inputs, conditional reproduction/OpenSpec/human acceptance, isolated tests, implementation/review/validation, delivery composition and operator visibility.

### Modified Capabilities

- `server/dark-factory`: Make specification bypass bounds and a separate requirements-review invocation explicit Flow policy so ticket delivery can use triage size and one later code review.

## Impact

Reuse `config/factory/flows/base-flow/`, `server/application/src/factory/authoring/factory_presets*`, requirements/protected-test/review handlers, Issue execution and Source/Reporter contracts, Delivery protocol/runtime/store, `octacity-github`, REST/OpenAPI, the common visual Flow editor, Project policies and existing Run/Issue outcome views. Extend these owners only where the supported journey lacks behavior; do not add a second engine or lifecycle store.

Builds on the archived triage, manual pipeline authoring, Project Factory policies and Issue outcomes changes. This change owns the explicit downstream handoff and any missing generic authoring or runtime bindings needed for this journey. `add-factory-bugfix-quickstart` remains independent. Merge automation, deployment, feature research, new providers and production promotion are out of scope. Implementation must follow repository behavioral Red/Green TDD; this change creates planning artifacts only.
