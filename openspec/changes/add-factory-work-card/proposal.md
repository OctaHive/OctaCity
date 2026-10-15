## Why

Factory diagnostics expose execution, evidence, and commands in separate collections, so an operator must reconstruct a Work item's history and find each pending human decision. A unified Work card with operator discussion and a bounded decision queue should make collaboration accessible while preserving immutable Work, program-owned execution, and existing control contracts.

## What Changes

- Add a stable card for an admitted Work Envelope that presents its safe original subject and input references, exact Configuration and candidate, the existing Factory Run, nested Flow Runs, cycles, Node Attempts, linked Builds, and retained outcomes without introducing another lifecycle or permitting multiple Factory Runs for one Work.
- Add bounded append-only operator comments and replies, scoped to the owning Work and management security context. **Confirmed scope:** comments are for operators; clarification enters execution only through a separately reviewed typed response to an existing human gate. Comments never trigger agents, change immutable Work, enter model Context automatically, or qualify as evidence or approval.
- Present check results from authoritative candidate/evidence/assessment/Decision records, explicitly distinguishing required failure, missing or indeterminate evidence, pending branches, superseded candidates, technical execution completion, and accepted results.
- Add a server-classified actionable queue for visible active human gates, bound escalation/delivery/promotion requests, and eligible infrastructure retries. Queue items retain exact Work/Run/Flow/cycle/attempt/contract identities, candidate binding where applicable, allowed responses, deadlines, and current Run preconditions.
- Resolve items only through existing idempotent, strongly preconditioned Factory controls. Recorded operator intent remains distinct from owner-committed completion; stale, expired, superseded, or concurrently answered items cannot authorize another attempt or candidate.
- Provide bounded authorization-safe card/history/discussion/queue REST reads and comment commands, and a Console card plus shared Project/global decision view. Preserve existing Run URLs, advanced diagnostics, Artifact-based intake, and the notification center's current contract.
- Preserve signed execution, deny-by-default rights, qualified backends, explicit Contexts and hidden tests, immutable input/evidence lineage, fenced append-only attempts, bounded budgets/rework, audit redaction, replay, retention, and ordinary CI/CD independence.

## Capabilities

### New Capabilities

- `server/factory-work-collaboration`: Work-card projections, non-authoritative operator discussion, exact result presentation, and actionable decision-queue discovery over existing Factory records.

### Modified Capabilities

- `server/rest-control-plane`: Add bounded Work-card, history, discussion, and decision-queue operations with the existing authorization, cursor, replay, control, and OpenAPI contracts.
- `ui/operator-console`: Add one directly addressable Work card and an actionable queue with discussion, exact history/results, and existing guarded Factory actions.

## Impact

- Extends Factory application projections, discussion store ports and additive PostgreSQL migrations, REST/OpenAPI, `ui/src/features/factory/`, and retention/backup documentation. Uses the current Work-to-Run relation and existing control owner; no new executor, Flow primitive, Agent protocol, or provider dependency.
- Depends on `add-dark-factory-mode` contracts and required release qualification, without claiming its outstanding tasks are complete. Works for existing Artifact-admitted Work and optionally for `add-factory-bugfix-quickstart`; that planning change is not an implementation prerequisite and neither earlier change is rewritten here.
- Excludes mutable Issue-style Work editing, new Work revisions or resubmission lineage, automatic agent chat/delegation, comment-to-Context forwarding, authenticated user profiles, personal unread inboxes, mentions/notifications, attachments, rich-text rendering, and additional merge/deploy authority. Implementation requires repository-mandated behavioral Red/Green slices.
