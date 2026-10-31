## Why

Operators cannot yet assemble their first complete Factory pipeline from ordinary steps: the UI has a dedicated triage wizard, while the general editor borrows execution bindings from existing Flows. The screen review also exposed read-only process criteria, misplaced Project document editing and a pipeline-wide repository requirement that obscures each step's actual inputs.

## What Changes

- Make **Add pipeline** open an empty visual Flow editor for adding, configuring and connecting steps and conditional transitions. A new Project with no Flows, Factory Configurations or installed business preset must be able to author, publish and run the two-agent assessment/classification example manually. Steps are nodes in this graph; nested sub-Flows are outside this change.
- Discover exact installed execution, policy and reporting capabilities independently of existing Flows. Configure step roles/profiles, supported models/tools, bounded result schemas, deterministic checks, transitions, budgets and permissions through shared typed controls.
- Keep agents reusable: Role/Profile describes capabilities, general working instructions, skills and execution settings; each Flow step separately owns its editable task and expected result. Project-fit and duplicate assessment belong to the step, not a hidden triage duty in the Project agent or installed runtime. The same agent/profile can serve different steps without edits.
- Give each agent step a distinct Executor section with clearly labeled Role and Profile selection, visible selected versions, a readable profile summary and a link to view that exact profile. Keep Documents/input and Tools and access separate. Show MCP bindings and understandable scope in Tools and access, with technical references/identifiers under Details. Step names do not select a role and browsing roles does not assign a profile or start execution.
- **BREAKING (authoring UX/API):** replace the dedicated triage entry point and triage-shaped draft assembly with common authoring. A template or built-in triage preset is not a prerequisite or a deliverable of this change. Preserve published definitions, accepted inputs, history and ordinary CI/CD; explicitly document any retired authoring contract.
- Make pipeline goals and process-completion criteria editable and reviewable, distinguish them from Project goals, step result schemas and source-authored task acceptance, and explicitly select which steps receive them.
- Manage shared documents under Project settings. Each step has one Documents list with bounded multiple selection from authorized Project documents, exact-version preview and removal, instead of separate Goals, Rules and Additional documents fields. Each selected document explicitly serves as agent instructions or reference material. Opening a document for editing never implicitly attaches it to a step.
- Define Project components and priority meanings in selected Project documents, without mandatory duplicate vocabulary lists/enums in the constructor. Result schemas validate bounded structure/types; they do not certify semantic compliance with prose. Configure Reporter conditions and allowed label/state effects separately from those documents and the agent task, using accepted results and explicit unmatched-case behavior.
- Declare available code sources at pipeline level using authorized Project repositories, then explicitly attach a selected source/revision/access scope to each step that needs it. Separate Work's subject/correlation from these resources. Support repository-free steps and repository-free workflows, plus different sources across steps. Pipeline source availability and a shared subject grant no automatic checkout or context access.
- Organize pipeline-wide policy into Start and queue, Execution limits, and Pipeline access. Show authoritative effective limits/access, their scope and origin instead of unexplained duplicate Flow/Configuration fields; preserve distinct per-run, configuration-wide and per-step enforcement semantics.
- Preserve isolated agent invocations, internal authoritative states/results, accepted-only projected handoffs, independent model/MCP/Reporter credentials and existing recovery/permission enforcement.

## Capabilities

### New Capabilities

- `server/factory-pipeline-authoring`: capability-backed construction of the first ordinary Flow without pre-existing Flows, business presets or a business-specific interpreter.

### Modified Capabilities

- `server/dark-factory`: repository-independent Work subjects, exact per-step code resources and editable process-completion inputs over ordinary execution and retention owners.
- `server/project-runs`: explicit source-free ordinary Build execution for qualified steps, preserving signed intent and existing repository-backed CI/CD semantics.
- `server/rest-control-plane`: first-use typed authoring choices/validation, criteria and step resource contracts, with headless parity and compatibility for retained inputs.
- `ui/operator-console`: empty general pipeline authoring, Project-owned document management, explicit step inputs/resources and removal of the special triage setup journey.

## Impact

Affected areas include Factory subjects/input envelopes and Build materialization, application choices/resolution and Project input publication, REST/OpenAPI and generated clients, PostgreSQL compatibility, the Factory and Project editors, and stand/package verification. Reuse existing Flow/Configuration, Role/Profile, Artifact, admission, Node Attempt, Context, Build and Reporter owners; introduce no second executor, credential registry or state journal.

The acceptance fixture must prove the complete manual path and actual isolated execution, not merely JSON submission or copying an existing Flow. Technical runtime/Agent installations remain explicit prerequisites. `tools/factory-stand` may keep its explicitly requested development example through the same ordinary contracts; it cannot be required by the empty-system journey. Coordinate shared contracts with `add-factory-bugfix-quickstart` and `add-factory-ticket-delivery-pipeline` without implementing or completing those changes. This proposal covers findings collected so far; further screen review can extend it before implementation.
