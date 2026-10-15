## Purpose

Defines a unified, authorization-safe view of admitted Factory Work, operator-only discussion, execution and verification history, and exact pending operator actions without introducing new execution authority.

## ADDED Requirements

### Requirement: A Work card retains the original immutable execution identity
The server SHALL expose a stable card keyed by the admitted Work Envelope identity and owned by its original Project. It SHALL identify the exact Configuration, Repository/base, safe input references, existing Factory Run, current candidate and Decision when available, and retained execution history. It SHALL preserve one Factory Run per Work, scoped admission replay, and immutable inputs; it SHALL NOT create a mutable issue lifecycle or group unrelated Work by matching text or external labels. Missing or retained-unavailable fields SHALL be explicit rather than invented from comments.

#### Scenario: Operator opens existing Artifact-admitted Work
- **WHEN** a visible Work was admitted before collaboration or without text quickstart
- **THEN** its card uses the original Work/Run identities and retained facts without a new admission or dependency on a preset

#### Scenario: A retry or requirements correction occurs
- **WHEN** the existing Run appends a retry, nested Flow, or new requirements cycle
- **THEN** the same card exposes that exact history and earlier subjects without replacing the original Work or creating a second Factory Run

### Requirement: Execution history and checks remain evidence-bound
The card SHALL expose bounded execution and result views identifying exact Run, Flow Run, cycle, Node Attempt, Build/Attempt/Job, candidate, evidence, assessment, and Decision references as applicable. It SHALL distinguish technical execution completion, verification pass/failure, pending or missing required branches, indeterminate evidence, superseded candidates, and accepted or rejected Decisions. A model summary, operator comment, stale result, or one successful Build SHALL NOT establish overall acceptance. A historical result SHALL remain bound to its original subject rather than satisfy a current candidate's gate.

#### Scenario: Implementation finishes while verification is pending
- **WHEN** an implementation Build succeeds but a required validation or review branch has not completed
- **THEN** the card reports implementation completion and the pending check without declaring accepted Work

#### Scenario: Old candidate passed a check
- **WHEN** rework creates another candidate after an earlier candidate passed validation
- **THEN** history retains the old result with its exact subject and marks it historical rather than applying its pass to the new candidate

#### Scenario: Required evidence is missing or indeterminate
- **WHEN** a required report or assessment is absent, unavailable, failed, or indeterminate
- **THEN** the view reports the specific recorded or unknown condition and cannot display it as a successful required check

### Requirement: Discussion is bounded append-only operator data
Authorized management clients SHALL be able to append plain-text comments to a visible Work and optionally reply to a root comment of that same Work. Each comment SHALL have a stable identity, monotonic Work-local sequence, server timestamp, normalized management-actor provenance, content digest, and at most 16,384 UTF-8 bytes of nonblank text. A Work SHALL accept at most 1,000 comments, or a lower configured ceiling, with bounded request/rate limits. Replies SHALL be limited to one level. The initial surface SHALL not support body replacement, user-supplied authenticated identity, attachments, executable markup, or mention-triggered actions. Existing sensitive-content controls SHALL reject recognized credentials and private transfer URLs before body publication; operational records SHALL contain only safe metadata and correlation.

#### Scenario: Two operators append concurrently
- **WHEN** two authorized clients submit different comments to the same Work
- **THEN** both may append with distinct identities and ordered sequences without overwriting either comment or changing the Run version or budget

#### Scenario: Reply names another Work's comment
- **WHEN** a client submits a parent identity from a different Work, an unavailable parent, or a nested reply
- **THEN** the append is rejected before publishing the body and does not reveal a hidden parent

#### Scenario: Discussion reaches a bound
- **WHEN** a comment exceeds its UTF-8 limit or the Work reaches its configured comment ceiling
- **THEN** the server returns a bounded validation or quota error without truncation, execution side effects, or synthetic Factory escalation

### Requirement: Comments cannot become Factory authority
Discussion SHALL remain outside the immutable Work inputs, Flow journal, provider session, Context Manifests, evidence, and control intents. Appending or reading a comment SHALL NOT invoke a model, start or retry execution, reserve WIP, modify a Configuration, disclose protected tests, or select a transition. Operator clarification SHALL enter execution only as an independently confirmed schema-valid response to an existing declared human gate. The response SHALL be frozen and consumed through the existing owner contract, without importing a discussion thread or replacing prior input history.

#### Scenario: A comment requests approval or execution
- **WHEN** a comment says approve, retry, deploy, skip tests, or mentions an agent
- **THEN** it remains operator discussion and creates no execution or control intent

#### Scenario: Operator clarifies a waiting human gate
- **WHEN** the operator separately chooses a declared response, supplies its bounded typed value, and confirms the exact current attempt
- **THEN** the existing control command records immutable intent and the Flow owner later consumes it according to the admitted schema and transition policy

#### Scenario: An implementation call follows discussion
- **WHEN** new comments exist before a queued or retried model call
- **THEN** its frozen Context and grants remain unchanged unless an existing declared human-gate result is explicitly projected by the admitted Flow

### Requirement: Discussion replay and lifecycle retention are durable
Comment append SHALL require scoped idempotency and atomically record the comment, sequence, replay outcome, and safe audit metadata. Exact replay SHALL return the same comment without another sequence or audit mutation; changed reuse SHALL conflict. Discussion SHALL be included in backup/restore and the owning Work/Run's retention and hold policy, without indefinite retention of bodies after their policy expiry. Expired content SHALL be reported unavailable rather than recreated from logs or provider transcripts; metadata retained for replay SHALL disclose no expired body. Reads and comment writes SHALL apply current visibility on every request.

#### Scenario: Comment acknowledgement is lost before restart
- **WHEN** the exact append is retried in the same management security scope after restart
- **THEN** it returns the original logical comment result and appends neither a second comment nor a second audit fact

#### Scenario: Work is held or its retained discussion expires
- **WHEN** retention encounters held Work and separately eligible expired discussion
- **THEN** held content remains retained, eligible bodies become unavailable through bounded cleanup, and no execution history or gate result is rewritten

### Requirement: Decision queue items derive only from current eligible Factory intent
The server SHALL expose a shared, visibility-filtered Project or global queue for active declared human-gate responses, bound operator requests, and currently eligible infrastructure retries. Each item SHALL have a stable logical identity and exact Work/Run/Flow/cycle/Node Attempt binding, contract/schema references, allowed outcomes or dispositions, safe reason code, authoritative actionable time, deadline when applicable, observed Run version, and exact candidate/Decision binding where the action requires one. It SHALL derive eligibility from authoritative retained state and admitted policy without interpreting phase names, comments, or model prose. Generic human responses SHALL not bypass bound escalation/delivery/promotion contracts. A read SHALL reserve no decision, lease, budget, or execution.

#### Scenario: Several nested human gates await input
- **WHEN** two current nested Flow attempts independently require permitted human responses
- **THEN** the queue contains separate items with their exact schemas and targets, without collapsing them into one approve-Work action

#### Scenario: A failed node has no allowed retry
- **WHEN** retry eligibility is exhausted, disallowed by policy, or superseded by another attempt
- **THEN** no executable retry item is published for it, even if its comment or error message asks for retry

#### Scenario: A configured delivery or promotion request awaits confirmation
- **WHEN** a visible current human gate binds that operation and the exact accepted subject satisfies its existing prerequisites
- **THEN** the queue offers only the declared disposition and exact subject; the card itself grants no new delivery, merge, or production authority

### Requirement: Queue freshness and command outcomes remain distinct
Queue reads SHALL identify their observation time and relevant Run/projection versions, apply expiry and current-cycle/cancellation eligibility, and distinguish waiting-for-operator from recorded-intent-pending-owner and consumed/superseded historical results. The actionable queue SHALL not present an already-recorded response as unanswered. Projection delay or unavailable state SHALL be explicit and SHALL NOT be presented as an authoritative empty queue or successful completion. Current mutation eligibility SHALL be revalidated independently of any queue item.

#### Scenario: Response is recorded before owner completion
- **WHEN** a valid response command commits but the Flow owner has not consumed it
- **THEN** the item is no longer actionable and the card reports recorded intent awaiting owner processing rather than claiming a completed node

#### Scenario: Two clients answer the same item
- **WHEN** both clients confirm different fresh intents against the same original Run precondition
- **THEN** at most one is recorded and the other receives a stale/conflict outcome; an exact replay of the winning intent returns its original receipt

#### Scenario: Deadline or candidate changes after observation
- **WHEN** an item expires, its cycle is superseded, the Run is cancelled, or its accepted candidate changes before confirmation
- **THEN** the existing command boundary rejects stale intent without redirecting it to a new attempt or candidate

### Requirement: Work collaboration uses safe independent read projections
Card, execution, check, discussion, timeline, and queue reads SHALL use bounded deterministic cursor pagination with visibility applied before projection, classification, ordering, counts, and cursor production. Cursors SHALL bind the Work or normalized queue scope and filters and expose freshness/coverage. System-generated entries SHALL contain allowlisted facts and references only, never task bodies, protected tests, prompts, raw provider traces/findings, credentials, private transfer URLs, or fence material. Read-model data and comments SHALL not reconstruct lifecycle authority after restart. Original inputs SHALL remain available only through their existing independently authorized retained-input/Artifact access boundary.

#### Scenario: Hidden Work sorts around a queue page boundary
- **WHEN** hidden Work or attempts would sort before or between visible queue items
- **THEN** the returned items, counts, metadata, and continuation cursor reveal no hidden target and retain visible page bounds

#### Scenario: Page mixes historical and current facts
- **WHEN** new execution or discussion records commit while a client follows a page cursor
- **THEN** the response identifies its bounded observation/snapshot scope and does not claim mixed-version evidence is a current acceptance result

#### Scenario: Collaboration is disabled
- **WHEN** card/discussion/queue capability is unavailable or disabled
- **THEN** existing Factory discovery, controls, Build diagnostics, and ordinary CI/CD remain operational under their established contracts

#### Scenario: Queue projection maintenance stops
- **WHEN** the derived queue cannot catch up with authoritative Run/control records
- **THEN** queue coverage and freshness report unavailable or incomplete, stale rows cannot be offered as unanswered actions, and existing Factory execution/direct controls continue without depending on that read projection
