## ADDED Requirements

### Requirement: Unified URL-addressable Work card
The Console SHALL provide one directly addressable card for an admitted Work with safe overview, operator discussion, execution history, checks/results, and required operator actions. It SHALL preserve exact Work/Run/Flow/cycle/attempt/candidate identities and link to existing advanced Factory and Build views. Work, selected tab, bounded filters, and diagnostic target SHALL round-trip through URLs; discussion bodies, typed response values, prompts, and private transfer URLs SHALL not. Existing Run deep links SHALL remain valid, and original task input access SHALL keep its independent authorization/download boundary.

#### Scenario: Operator follows Work from a Run or Project
- **WHEN** a visible Work is selected from existing discovery
- **THEN** the card exposes its original immutable subject and existing Run with directly addressable history/check/action sections without a new admission

#### Scenario: Operator refreshes a historical attempt
- **WHEN** a card URL selects an earlier attempt or candidate
- **THEN** the selected historical record remains visible with its original identity and is not mislabeled as current evidence or an actionable current request

### Requirement: Discussion and execution input are visibly separate
The Console SHALL present plain-text operator comments/replies with server attribution and timestamps, an explicit operator-only purpose, UTF-8 limits, and independent append feedback. Posting a comment SHALL not be presented as answering a human gate, approving a candidate, or sending a message to an agent. A clarification action SHALL open a separate typed form for the server-provided declared gate/schema and require existing target-specific confirmation. Unsubmitted comments and response values SHALL remain only in component memory and be discarded on leave/reload; no discussion payload or replay key SHALL be stored in browser persistence, analytics, URLs, or logs.

#### Scenario: Operator posts an approval-like comment
- **WHEN** the comment says that a result is approved or contains an agent mention
- **THEN** the Console records it as discussion only and shows no successful Factory control outcome

#### Scenario: Operator answers a clarification
- **WHEN** a current queue item exposes a declared human response contract
- **THEN** the Console renders its bounded typed inputs, confirms the exact target/version, and submits an ordinary Factory control command separately from comment append

#### Scenario: Comment submission loses its response
- **WHEN** a confirmed append's transport response is lost while the intent is active
- **THEN** retry uses the same frozen text/parent and idempotency key, preserves input while pending, and does not silently append again with a new key

### Requirement: Check and history presentation preserves result authority
The card SHALL render server-published exact execution and evidence state with non-color labels and candidate/attempt correlation. It SHALL distinguish completed execution, pending required checks, failed/indeterminate/missing evidence, superseded results, and accepted Decisions. Incomplete pages, unavailable records, and stale observations SHALL remain explicit. The UI SHALL not infer acceptance, promotion, retry eligibility, or queue completion from comments, model text, local state, or probability.

#### Scenario: Model reports success before verification
- **WHEN** a model-backed Build finishes and verification still has required pending or failed branches
- **THEN** the card shows separate execution and verification outcomes and does not mark the Work accepted

#### Scenario: Background refresh fails
- **WHEN** an execution, checks, discussion, or actions refresh fails after showing data
- **THEN** previous data remains visibly stale, unsafe confirmation is disabled until refreshed, and empty/successful state is not fabricated

### Requirement: Shared actionable decision queue uses guarded controls
The Console SHALL offer bounded Project/global decision views and the same Work-scoped items in the card. It SHALL show server-classified reason, exact target/contract, deadline, candidate where required, and permitted choices; opening an item SHALL navigate to its canonical Work context. Submission SHALL use existing immutable confirmed-intent handling, one key per reviewed intent, current strong Run precondition, duplicate prevention, and targeted refresh. Recorded intent, owner processing, completed response, expiry, and supersession SHALL remain distinct. The queue SHALL be shared operational work rather than a personal unread inbox, and SHALL not change the existing header notification-center categories.

#### Scenario: Operator opens an actionable item
- **WHEN** the operator selects a visible waiting human gate or eligible retry
- **THEN** the card identifies its exact Run/Flow/cycle/attempt and renders only the current server-declared command and response contract

#### Scenario: Another operator already answered
- **WHEN** confirmation receives a stale/precondition conflict because another intent was recorded
- **THEN** the Console preserves review context, refreshes the authoritative item, and requires a new explicit review rather than resubmitting against a newer attempt

#### Scenario: Control intent is accepted but owner has not progressed
- **WHEN** the existing command returns a recorded-intent receipt
- **THEN** the actionable item disappears or is marked no longer actionable and the card displays awaiting owner processing without claiming a successful node or delivery

#### Scenario: Collaboration is absent or keyboard navigation is used
- **WHEN** the feature is unavailable or the operator uses supported narrow/keyboard-only layouts
- **THEN** existing Factory/CI views remain usable and available card/queue functionality retains localization, focus, semantic labels, and bounded query/error behavior
