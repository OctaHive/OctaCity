# Project Issue workflow contracts

An Issue workflow describes business statuses and permitted organizational moves.
It is independent of an execution Pipeline, Factory Flow, provider, repository or
model. A workflow terminal selection or a status named `Done` does not establish
execution acceptance or authorize delivery.

## Definitions and identity

`IssueWorkflowDefinition` contains a name, an ordered ID/name-only status list,
one required `initial_status_id`, a `terminal_status_ids` set, and explicit direct
transitions. Initial/terminal selection belongs to the workflow, not status flags.
Status and transition UUIDs remain stable when display names or order change.
A retained definition is identified by Project, workflow ID and positive workflow
version; references must keep all three.

- A definition has 1–64 statuses and at most 256 transitions.
- `initial_status_id` references exactly one status in this definition, independent
  of display order. `terminal_status_ids` contains 0–64 unique references to its
  statuses. Unknown references, duplicates and missing settings are invalid.
  Empty terminal selection is valid, and the initial status may also be terminal.
  Selection organizes tasks only; an explicit reopening edge is allowed.
- Status entries reject `initial`/`terminal` flags, avoiding a second source of
  truth. Workflow settings are retained in the same immutable definition version;
  changing them uses ordinary publication/replay and does not migrate old Issues.
- Workflow, status and transition names use the shared 128-byte UTF-8 resource
  name ceiling, with no empty, padded or control-containing values.
- IDs cannot repeat. Edge endpoints must exist and differ. Each edge requires
  at least one allowed origin; a `(from, to, origin)` cannot be declared twice.
- Allowed origins are `manual`, `pipeline` and `source`. They describe permitted
  command provenance, not granted caller rights or automatic execution.
- Evidence requirements are the closed values `accepted_execution` and
  `passed_required_checks`, without duplicates. All listed requirements must
  hold for the exact applicable execution subject when movement is implemented.
  Unknown values and script/guard fields are rejected. Workflow validation
  validates these declarations; it does not evaluate execution evidence.

## Default selection and publication

Every existing Project has a declarative initial default even before an
administrator publishes custom policy. Reading it makes no authoritative write
and needs no administrative permission, Factory configuration or integration.
The implicit workflow's typed ID uses the Project UUID, scoped by that Project.
Its publication time is the Project creation time.

The retained [v1 template](../../server/core/octacity-server-store/src/issue_workflow_default.v1.json)
is ordinary editable policy data. Its names are not domain enum values. Never
overwrite the meaning of that version when introducing future templates;
existing exact references must remain resolvable.

`PublishIssueWorkflowCommand` creates or replaces an immutable definition under
an exact current-version precondition. It does not change the Project default.
`SelectDefaultIssueWorkflowCommand` selects an exact same-Project workflow
version under its own selection-version precondition. A workflow version and a
selection version are different types and cannot substitute for each other.
Publication/default selection require `Administer` on Project-owned
`IssueWorkflow`; reading and draft validation use `View`. Neither command
creates an Issue, migrates a pinned workflow, admits Work or contacts a provider.

Both mutations partition idempotency by operation and accepted management
security scope. Exact replay returns the original receipt after subsequent
changes. Different intent under the same key conflicts; publication time is not
part of caller intent and is not renewed by replay. Publication, safe audit,
outbox evidence and replay commit together. Invalid publication or default
selection leaves authoritative state unchanged.

## Implementation boundary

The public application handlers and narrow `IssueWorkflowStore` port have memory
behavior over the same Project authority used by manual tasks. PostgreSQL
persistence, REST discovery and Console settings are separate work-card tasks
2.2, 2.3 and 2.4. Issue creation and the shared guarded movement command follow
in 2.5; publishing this vocabulary alone enables no transitions or execution.
