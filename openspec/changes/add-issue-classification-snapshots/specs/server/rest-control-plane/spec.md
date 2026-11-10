## MODIFIED Requirements

### Requirement: REST Issue execution requests preserve ordinary admission
The API SHALL expose bounded Issue execution history, explicit reviewed execution requests and request-status reads. Commands SHALL bind the observed Issue revision, exact visible target/repository and input mapping; validate ordinary admission authority and prerequisites; and expose pending publication/admission versus a durably linked Work/Run. Replay SHALL resume the original request and return its exact outcome after interrupted publication, admission, association or acknowledgement. Reads SHALL link to existing Factory Run/Build routes. Later intentional execution SHALL require a distinct request, while existing retry controls SHALL remain unchanged.

For a published Issue-snapshot target, the request and review SHALL select the exact Issue version, bounded field/material selections and input contract without requiring predecessor_run. The retained response and execution detail SHALL identify the original snapshot/definition versions, selected field/evidence coverage, exact comment/document identities/digests, explicit optional omissions and admission-policy decisions. Missing, stale, incompatible, oversized or inaccessible required data SHALL have bounded field/material-specific diagnostics. Client-supplied values, bodies or provenance SHALL NOT substitute for server-resolved reviewed sources. Existing predecessor-based requests SHALL retain their original typed contract and replay behavior, and newly authored exact-producer inputs SHALL remain supported.

#### Scenario: Client starts another pipeline for an Issue
- **WHEN** it confirms a new request against an exact revision and authorized target
- **THEN** the returned execution is retained under the same Issue with its own immutable Work/Run and prior executions remain addressable

#### Scenario: Client retries a partially admitted request
- **WHEN** the same request is submitted after restart with a lost admission acknowledgement
- **THEN** the API recovers its original Work/Run association without a second execution

#### Scenario: Issue changed before execution confirmation
- **WHEN** the submitted reviewed Issue version is no longer current
- **THEN** the API returns a conflict without silently selecting new content or invoking ordinary admission

#### Scenario: Headless client starts delivery from current Issue data
- **WHEN** it submits a reviewed current Issue revision to a compatible Issue-snapshot Configuration without a predecessor Run
- **THEN** ordinary admission freezes its requirements and selected classification with one recoverable Work/Run association

#### Scenario: Client supplies an arbitrary accepted flag
- **WHEN** a request attempts to use a metadata field or forged producer/evidence reference as permission to bypass snapshot admission checks
- **THEN** the API rejects the unsupported or unauthorized claim without publishing an execution

#### Scenario: A client reviews selected supplementary text
- **WHEN** an authorized request selects a retained Issue comment and exact Issue-owned or explicitly linked Project document under the target's input contract
- **THEN** review and admitted detail identify the selected sources, owners, publications and digests while body access remains independently authorized and later publications cannot substitute input

### Requirement: REST operator-only Issue discussion retains scoped replay
The API SHALL expose bounded dedicated comment body reads and idempotent plain-text append commands for a visible Issue, with optional one-level same-Issue root replies. Actual actor provenance SHALL come from management context. Exact replay SHALL preserve the original logical outcome and changed reuse SHALL conflict. Appending or reading comments SHALL not execute commands, update Context or authorize approval. A separate authorized Issue material selection and reviewed/configured snapshot request SHALL be required to consume selected comment text as data in a new execution. Audit, list/card summaries and queue payloads SHALL exclude bodies; dedicated body reads SHALL apply no-store. Discussion rights SHALL grant neither Issue execution nor Factory control rights.

#### Scenario: Comment append is denied
- **WHEN** a client lacks Issue discussion permission or cannot observe its parent
- **THEN** denial precedes comment storage, sequence/replay allocation or mutation audit and discloses no hidden target

#### Scenario: Another security scope reuses a key
- **WHEN** a different management scope submits an already-used append key
- **THEN** it cannot obtain the first scope's comment or replay outcome

#### Scenario: A discussion-only client attempts snapshot consumption
- **WHEN** a caller can append a comment but lacks Issue material mutation or execution authority
- **THEN** those separate operations are denied without widening its grants or exposing another selected body

### Requirement: Project document management remains separate from attachment
The API SHALL reuse Project input document discovery, exact authorized content reads and immutable publication for shared Project documents in Project settings, explicit link-existing-Project-document actions and step selectors. Project document collections/selectors SHALL exclude Issue-owned documents and Project document content endpoints SHALL NOT return them, even when the caller can access both owners. Metadata collections SHALL remain bounded and content-free; body reads SHALL enforce current Project/content authorization, retention and no-store behavior. Publication SHALL not attach the result to an Issue or step, replace process criteria, enable intake or mutate accepted Runs. New criteria content SHALL be frozen through existing managed-input publication with stable replay before admission.

Common authoring SHALL accept a bounded collection of exact Project document references with explicit instruction/reference roles for each step and normalize them to existing typed Context bindings. It SHALL require no fixed goals/rules/supporting-document slots. Document names or content SHALL not infer instruction authority. Selected instructions SHALL retain existing controlling-policy semantics; reference material SHALL remain data, and neither role SHALL grant execution permissions. Explicit Profile requirements and independently required Project policy SHALL remain enforced. Explicitly linking a Project document to an Issue SHALL retain its Project owner and exact existing publication through the Issue mutation contract; consuming it SHALL still require the target's explicit input/step selection.

#### Scenario: Client submits a unified step document selection
- **WHEN** an authorized client validates multiple exact same-Project document selections with instruction/reference roles for one step
- **THEN** normalization retains those references and roles only for that step using existing Context contracts, without publishing new documents, inferring authority from their names or changing sibling inputs

#### Scenario: Document-guided assessment has no vocabulary enum
- **WHEN** a client attaches exact component/priority guidance documents and a bounded structural result schema without copied classification enums
- **THEN** authoring accepts that representation subject to ordinary compatibility/policy checks, performs no implicit prose-to-enum conversion and requires Reporter conditions/effects to be explicitly configured separately

#### Scenario: Project document is edited
- **WHEN** an operator publishes changed content through Project document management
- **THEN** the API returns an immutable exact reference and existing Issue/pipeline/Run selections remain unchanged until explicit selection and any required versioned publication

#### Scenario: A document read is denied
- **WHEN** the caller lacks current access or the selected exact content is unavailable
- **THEN** the API reports the safe reason without returning another document or treating the missing body as empty content

#### Scenario: Task documents exist in the same Project
- **WHEN** a client lists shared Project documents, opens a step document selector or uses a Project content endpoint after an Issue document is published
- **THEN** the Issue document is neither listed nor returned through those Project contracts and existing shared documents remain available under their own permissions


## ADDED Requirements

### Requirement: Issue document management preserves explicit owner scope
The API SHALL expose bounded Issue-scoped document metadata/history, independently authorized exact content reads and idempotent text creation/immutable-version publication. Each publication SHALL retain its exact Project/Issue owner, document/publication and Artifact identities, digest, encoded size and server-derived actor attribution. Clients SHALL NOT override the owner through request fields or obtain another Issue's document through guessed IDs, replay keys or Project document endpoints. Issue visibility/update rules SHALL govern the corresponding read/write operations, independently of shared Project document administration, and content authorization/retention checks SHALL precede body access. Body reads SHALL apply no-store; generic metadata, audit and receipt summaries SHALL exclude bodies.

Creation/publication SHALL make the document available in the owning Issue's Documents view without creating a Project catalog entry or transferring ownership. It SHALL NOT select snapshot inputs or start execution. Existing Project publications SHALL retain their owners and contracts; linking one to an Issue SHALL be a separate preconditioned operation. Closed generated contracts SHALL distinguish Issue-owned publications from linked Project sources and retain immutable versions under scoped replay.

#### Scenario: Client creates a document for one Issue
- **WHEN** an authorized client publishes bounded text through an Issue-scoped document command
- **THEN** its receipt identifies that Issue owner and exact immutable publication, the document is discoverable only through that Issue's document contract, and no Project document or execution is created

#### Scenario: A publication acknowledgement is lost
- **WHEN** the same authorized Issue-scoped publication is retried after a newer document version exists
- **THEN** the API returns the original receipt/publication without another version, owner change or disclosure across replay scopes

#### Scenario: Client substitutes the Issue owner
- **WHEN** a client reads, versions or selects an Issue document using another Issue's route or a forged owner/publication reference
- **THEN** the request is rejected before hidden metadata/content disclosure or a partial publication/material mutation

### Requirement: Issue classification management exposes typed values and applicability
The management API SHALL expose authorized current and historical effective Issue classification, selected Project definitions, safe field provenance and applicability/blocker information through bounded generated contracts. Manual classification edits and configured Factory application SHALL use existing Issue mutation authorization, source ownership, exact preconditions, scoped replay and audit rules. Field changes and their provenance SHALL commit atomically. The response SHALL distinguish accepted producer output from successful application to Issue fields; a conflict SHALL expose no private producer content.

The API SHALL advertise the bounded fields, supported values and semantic input bindings reusable by Issue editors and Flow authoring. It SHALL NOT require arbitrary nested JSON in generic planning properties, duplicate component/priority lists per step, or a write-capable external provider for local classification. Read-only detail and readiness operations SHALL start no tools, executions or provider writes.

Existing Project property publication SHALL remain the shared contract for supported custom property types, choices, bounds and optional classification semantics. Definition administration SHALL retain Project administration rights separately from Issue value editing; clients SHALL be able to discover the authoritative catalog and exact retained definitions without creating another classification schema.

#### Scenario: Client edits classification and loses the response
- **WHEN** an authorized preconditioned edit commits and its exact request is retried after a later Issue change
- **THEN** the original edit receipt is returned without another revision or replacing the later values

#### Scenario: Caller can read the Issue but not a private triage transcript
- **WHEN** it reads applied classification and reviews an Issue-snapshot execution
- **THEN** authorized field values and safe provenance remain available without exposing or requiring that transcript

### Requirement: Issue material management exposes exact authorized source selections
The API SHALL expose bounded current/historical Issue material metadata and preconditioned idempotent selection/replacement/removal through existing Issue authorization and history. Closed generated contracts SHALL distinguish exact same-Issue comment references, same-Issue document publications and explicitly linked same-Project document publications, retain exact source/owner/publication identities and digests, reject forged source/owner/body/actor data and enforce the shared 32-entry bound. Cross-Issue document references SHALL be rejected even within the same Project. Empty/absent selections SHALL preserve legacy decoding. Independent content readers SHALL enforce current source access, retention and no-store; Issue visibility alone SHALL not grant body access.

Snapshot review SHALL resolve permitted selected text on the server, enforce at most 1 MiB combined UTF-8 bytes and stricter consumer limits, distinguish required blockers from explicit optional omissions, and preserve selected inputs on replay. Issue/Project document publication and discussion endpoints SHALL remain usable independently of selection or execution. Selecting/removing a material SHALL neither delete source content, change its owner nor create a provider effect or Work.

#### Scenario: A material edit response is lost
- **WHEN** an exact authorized selection command is retried after another Issue edit
- **THEN** it returns its original receipt without another revision, body restoration or replacement of newer selections

#### Scenario: Source access is lost during preparation
- **WHEN** a required selected body becomes inaccessible or expires before admission commits
- **THEN** the request reports an actionable source-specific blocker without returning hidden content, using a newer document or admitting empty replacement text
