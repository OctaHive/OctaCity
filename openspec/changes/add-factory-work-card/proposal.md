## Why

Operators need one persistent task that connects requirements, external tickets, Factory executions, current progress, and results. A card tied to one immutable Work cannot represent manual drafts, later changes, or assessment and implementation performed by different pipelines.

## What Changes

- Introduce a Project-owned **Issue** with a stable internal identity, human-readable key, editable versioned requirements and metadata, optional external source link, and history of related executions. Manual creation and editing work without a provider, credentials, repository, or configured Factory pipeline; execution has its own prerequisites.
- Add versioned Project business workflows with stable status/transition identities, editable names, one initial status and explicit permitted moves. Supply an editable default so first manual creation requires no workflow setup. Manual commands, accepted pipeline actions and permitted source pulls share one guarded transition boundary; generic metadata editing cannot bypass it.
- Configure explicit Issue-status actions on selected declared pipeline outcomes. Freeze workflow/transition and exact execution provenance, reject stale or competing updates, and retain idempotent history. Run completion, stage names and model prose never infer a task status or execution acceptance.
- Replace the proposed one-Work card with a persistent task card. One Issue can have many immutable Work Envelopes, each retaining its existing single Factory Run. Explicit execution requests freeze the selected Issue revision and exact Factory target; retry remains within its original Run. Existing Work/Run identities and URLs stay valid.
- Import and refresh tasks through installed provider-neutral Source capabilities, and synchronize explicitly managed fields through versioned capability-supported mappings and existing Reporter recovery. Initially allow zero or one primary external source per Issue; preserve native identity, field ownership, conflicts, freshness, and independently scoped read/write grants. GitHub is the first concrete adapter; GitLab and Jira must fit the same contract without becoming dependencies or promised implementations.
- Keep external status mapping separate from the Project workflow and pipeline actions. Map stable internal status IDs to supported native values/effects, or explicitly owned native observations to permitted internal transitions; declare one authority/direction per field, expose unmapped/ambiguous/forbidden moves and prevent echoes or competing write producers.
- Show task business status, execution state, current AI profile when available, enrolled execution worker, and synchronization state separately. Present exact run/stage/Build results, current and historical checks, safe activity, and links to authorized logs without deriving acceptance from agent prose.
- Retain bounded operator-only discussion and an exact shared operator-action queue. Comments do not invoke agents or become execution Context; clarification and approval still use declared guarded controls.
- Add REST and a task-oriented Console journey: create a task, edit requirements, select a pipeline, inspect the frozen request, start execution, follow progress, and revisit the same task for another execution. Integrations are selected by authorized connection/resource references; credentials never enter task or pipeline forms.

## Capabilities

### New Capabilities

- `server/factory-work-collaboration`: Persistent Issues, configurable Project status workflows, guarded manual/pipeline/source transitions, immutable revision-to-Work lineage, separate external status mappings, operator discussion, exact execution/result projections, and actionable decision discovery.

### Modified Capabilities

- `server/rest-control-plane`: Add versioned Project workflow management, transition choices/history/commands, typed Issue-status action publication, Issue creation/editing, execution requests, source association/synchronization, bounded history/discussion/queue reads, and generated contracts while retaining existing Factory controls.
- `ui/operator-console`: Add Project status/transition settings, explicit pipeline Issue-status action authoring and separate external mapping review alongside task lists/cards, manual creation, run history, current execution, synchronization feedback, and guarded operator actions.

## Impact

- Extends Factory application/store boundaries, additive PostgreSQL migrations, existing Source/Reporter composition, retained Project inputs, REST/OpenAPI, Console navigation, and retention/backup guidance. No new executor, Agent protocol, provider SDK in the domain, or second Factory lifecycle owner.
- Reuses the connection/resource grants, immutable intake and reporting contracts from the archived `add-github-work-source`; its live GitHub qualification remains unqualified and does not become a prerequisite for manual tasks. Issue-owned workflow/actions and field synchronization stay in this change. Reuses the recoverable text-publication seam planned by `add-factory-bugfix-quickstart`; deliver the shared seam here if needed first, without creating a parallel intake implementation. AI role profiles remain optional integration with `add-factory-role-profiles`.
- Excludes a generic workflow scripting engine, automatic migration of existing Issue workflows, automatic agent chat/delegation, comment/attachment synchronization, automatic creation of external tickets, multi-provider mirroring of one Issue, implicit execution on text/status edits, a new pipeline canvas, personal authenticated inboxes, and additional merge/deploy authority. Implementation uses mandatory behavioral Red/Green slices; this update changes planning artifacts only.
