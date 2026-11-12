# Ticket delivery Flow branches

## Current v2 Issue snapshot example

`v2.json` is the Issue-snapshot `ticket-delivery` release. Resolve its exact
reference from the Project preset catalog through the resolver API. Select the
Project's published field-definition version and an explicit admission policy:

```json
{
  "issue_snapshot": {
    "definition_version": 1,
    "admission": {
      "priority": {"field": "priority", "values": {"blocker": 100, "high": 80, "medium": 50, "low": 20}},
      "risk": {"field": "risk", "values": {"low": "low", "medium": "medium", "high": "high", "critical": "critical"}},
      "allowed": {"work_kind": ["defect", "feature_request"]}
    }
  }
}
```

Use the actual active choice IDs in that Project. These mappings are reviewed
scheduling/risk policy, not another agent assessment or a per-step vocabulary.
The explicit type allowlist must be a nonempty subset of the release's supported
`defect`/`feature_request` routes; adding a Project type does not assign a route.

The example selects built-in priority, component, type (`work_kind`) and size,
existing dependency references, and the Project's severity, risk, recommended
route and reproducibility semantic bindings. Publish those definitions first.
The resolver derives the shared result/input types from that exact catalog;
operators do not copy its enums into each step. No `accepted_predecessor`,
producing Flow/terminal/schema or triage Run is required. Passing a predecessor
alongside this mode is rejected. An eligible manually prepared Issue uses the
same contract as an Issue classified by any authorized triage implementation.

The frozen Issue supplies the reproduction decision and requirements summary.
Requirements approval uses Project priority IDs: the released example requests
human acceptance for component `api`, size `medium`/`large`, and priority
`blocker`/`high`. Review these literal policy choices in the resulting editable
Flow. Admission/scheduling scores remain separate from the canonical choice.
Requirements, protected tests, candidate review, verification and trusted PR
publication keep their ordinary owners and independently selected executors.

Resolution is read-only. It can return an editable Flow alone (`flow_only`) or a
disabled Configuration draft; publish and select exact versions explicitly.
Source intake and Reporter effects remain separate opt-ins. The default snapshot
selects no comment or document text. Add any required ordinary fields, explicit
materials/evidence and per-step projections through the generic Flow contract;
prose grants no evidence or approval authority. Visual Project-field selection
is not yet available; use API authoring for this preset's Issue input. Console
explains this required-input limitation and disables opening `v2`; choose `v1`
there to use the existing producer-result controls.

## Retained v1 producer-result example

`v1.json` and its parts retain their original bytes and exact accepted-triage
contract. Discovery offers both `v1` and `v2`, and exact references for both
remain resolvable. Follow the catalog's opaque `next_cursor` to visit every
version; older identity cursors still skip all versions of that identity.
Existing published Flows,
Configurations and Runs are not upgraded in place. Their Incoming pipeline
completion controls and exact earlier-Run selections remain supported.

Both versions compose the branches below and disable the optional test-authoring
queue. Review stage Profiles, installed command reports, the publication target,
step data, code selections, access requests and limits before ordinary Flow and
Configuration publication. The example selects no credential, activates no intake
and grants no access.

The reproduction command is optional. Leave it unselected for feature tickets:
the ordinary entry gate routes features and already reproduced defects onward.
Without this command, a defect that still needs reproduction goes to the existing
operator review step; it cannot proceed directly to test authoring. Selecting an
installed reproduction command keeps the full observation branch below.

For specification authoring, allow Coder to write the selected specification
directories as well as the implementation directory. On the local stand, pass
each explicit scope using `--delivery-write-directory` (for example,
`openspec/changes`, `openspec/specs` and the intended implementation directory).
Attach those sources to the appropriate steps in the editor. A source added to
the Flow does not grant a step access to it.

API clients discover the immutable release in
`GET /api/v1/projects/{project_id}/factory-presets` and review it via the
catalog's `/resolve` operation. Set `flow_only: true` to get only the editable
Flow, or supply ordinary Configuration parameters to also review a disabled
Configuration. The resolver identifies missing and incompatible dependencies
by path; publication always uses the ordinary management commands. Source
intake and Reporter effects are explicit. A confirmed pull request is ready
for review and manual merge.

The installed publisher supplies the exact target and successful receipt
contract. Select the bundle and manifest independently in the publication
step's Context. A publication target appears in the editor when the
installation supplies a compatible receipt schema and its plugin and logical
credential profile are selectable in the Project. The draft requests the
target's configured adapter, hosts and credential profile. Review these
requests against the Project access policy before publishing. The example
binds both review and validation expiry. The earlier expiry closes new forge
writes while the original attempt can still observe an existing PR.

## Reproduction

`reproduction-v1.json` is an editable Flow manifest for the reproduction branch.
Resolve its parts relative to `config/factory` through the ordinary manifest
loader. The branch accepts only `work_kind` and `reproducibility` from the exact
accepted triage result. Select those fields explicitly in the parent call.
When placing the nodes directly in a root Flow, use the same projection from
the pinned incoming triage schema instead of the caller input.

Bind `reproduction` to the exact installed Tester command and review its
permissions and budget. Add its canonical `observation_contract.report_schema()`
under the `reproduction_report` schema choice and select that same report as
the command's primary outcome. The observation schema is reserved for
commands with an installed observation criterion. The command can read the original
Work task at `/octacity/protected/work-task`. Select any required code access
for this step. The installed command pins the symptom criterion and environment
and records bounded checks against the admitted subject.

By default an indeterminate report or a triage assessment that requires more
input leaves the Flow waiting for an operator at `review-reproduction`. Dismissing that step ends
the branch as `unresolved` without a Reporter effect. If the installed command
sets `stop_on_indeterminate`, the Flow stops at the command instead and retains
the report under the Build. An explicit retry uses the ordinary technical
retry contract.

Continue to size routing from `bypass` or `reproduced`. Only the latter carries
a new reproduction report. The existing triage predecessor binding excludes
review and missing-input outcomes. `cannot_reproduce` ends the branch before
any implementation work.

For optional external effects, use `reproduction-effects-v1.json` as the
`outcomes` selection of the ordinary result reporting composer for
`reproduction-result`. Choose an existing writer and replace the label
strings with the installation's existing managed labels when needed. Include
the exact reporting child and schemas in normal publication. The negative
finding requests adding the mapped `can't reproduce` label, removing the
managed `need reproduce` label and keeping the ticket open. The positive
finding only requests removing the managed label. Reporter preserves other
labels, and the accepted Flow result remains separate from delivery receipts.

## Requirements policy

`requirements-v1.json` loads an editable branch after the reviewed
reproduction decision. The caller binds `summary` and the retained
`priority_assessment` from the accepted triage handoff (`/assessment/priority_assessment`
in the triage preset). In a root Flow, replace the entry's caller binding with
the equivalent projection from the pinned incoming schema, or select those
fields from an explicit predecessor.
The task and implementation acceptance come from the admitted Work. Issue
admission and automatic intake wait for explicit ticket acceptance when the
triage Work only contains process-completion criteria.

`requirements_needed` prepares the accepted summary for `small` and
`medium`. `large` invokes the selected `specification_author` profile.
Configure its assignment to author an OpenSpec proposal, delta specifications,
design and tasks, and for a defect to record regression criteria. Qualify the installed independent verifier to check those files and
their coherence before it attests to the captured requirements. The shared
`specification_gate` checks the immutable capture and verifier evidence.
`specification_accepted` prepares the summary with the exact manifest and
bundle. Its Issue card links to those two artifacts while requirements are
still eligible for human review. The accepted code-review card links to the
exact Build report selected by the review profile. Both cards can be turned
off or edited through **Show on Issue** on the corresponding result. Link
visibility follows the ordinary Artifact and Build access checks. Invalid
evidence has a separate terminal.

The Run's step completions show the verified report digest, verification time
and freshness limit for the review and protected validation steps. Use the
digest to correlate each report with its exact execution.

The example policy requests human review for component `api`, sizes
`medium` and `large`, and triage priority at least 70. Edit the ordinary gate
rules for the Project. An empty rule list sends every valid ticket through
`automatic`. The queue priority remains independent. The selected rule and
retained requirements are pinned to the admitted Flow and the pending
`requirements_review` attempt.

Use the Run's operator controls to resume or abort the pending review.
The example allows 30 minutes for the operator response. Set the
HumanGate time budget to fit the Project's review process.
Resume freezes an Accepted Work Contract at `requirements_approved` using
the review attempt's original input. Unmatched tickets freeze at
`requirements_review_policy.automatic`. Abort selects `needs_correction`
and stops this branch before Tester. For updated ticket acceptance, admit
the updated Work through the requirements route again. A Flow composing
its own correction path can route refusal to its declared requirements
entry in a new workflow cycle. A changed specification or Work requires
a fresh review when the policy matches.

The compact branch uses the same acceptance policy after the deterministic capture check.
A Flow that needs an independent specification reviewer can compose the
existing review, review gate and candidate publication path from the base
Flow. When connecting protected test authoring, place the requirements nodes
in the same Flow and explicitly select the accepting record on each route.
The standalone `ready` terminal supports qualifying the requirements branch
before that downstream composition.

## Protected test preparation

Compose `protected-preparation-v1.json` after the requirements acceptance
routes. Include the base Flow's `protected_tests`, `implementation_entry`
and `implementation` nodes with their referenced schemas and policy.
The base `protected_tests` node selects the `development.ready` phase pool.
Configure a pool with that exact key before admission. Set its budget and
concurrency within the Flow admission limits, and map the queue fields
`severity` and `dependencies` from `/accepted/severity` and
`/accepted/dependencies` in the frozen step input. If this Flow should run
test authoring without a phase pool, remove the node's `pool` setting
explicitly before publication.
Connect each successful requirements route to `protected_tests` in the
same Flow. It reads the frozen Accepted Work Contract and publishes the
private inventory and a separate public coverage report. The test author reads
the Work task at `/octacity/protected/work-task` and the exact selected acceptance
at `/octacity/protected/work-acceptance`.

Install a qualified command in `protected_validation` for
`tests_on_base`. Set `observation_contract.requires_protected_tests`
to true and leave `stop_on_indeterminate` off so the graph can retain the
blocked outcome. Supply the exact author Profile, policy and inventory
contract under `inputs`. Select the canonical observation report as
`report` and as the command's sole output. Base observations omit
`capture_tool` and `facts`. Publish `baseline_report` using the
installed observation contract's `report_schema()`.

The signed baseline Job checks out the admitted Work revision, including
for Work with a later specification candidate. Its only private test inputs
are the selected inventory and the exact files it names. The command should
establish a working harness and complete applicable test coverage before
reporting whether the expected behavioral failure was observed. Conclusive
reports must include `test_classes` in the pinned policy order, with one
`class` and `outcome` per required class. `matched` requires a failed class
with no indeterminate classes. `not_matched` requires all classes to pass. Emit a
bounded indeterminate report for setup or coverage failures when the runner
can still report them. A technical Job failure stops the command through
ordinary execution handling.

An observed expected failure proceeds to `implementation`. A complete
passing baseline reaches `already_satisfied`. Inconclusive observations
reach `test_preparation_blocked`. These terminal routes end preparation
before implementation and publication. The Coder input includes requirements
and the public coverage classes. Only the trusted command receives the
selected test files. Install the exact referenced author in the same Project.
Set its policy's `transcript` to the name of the command's private transcript
Artifact and grant that exact output.
The original Build retains the bytes, and the immutable inventory pins its
reference separately from test files. Neither the baseline nor validation
mounts the transcript, and the public coverage report contains only class names
and the accepted contract identity.

## Candidate review and validation

Add `development-v1.json` and `agent-result-gates-v1.json` after protected
preparation. Include the base Flow's `implementation_rework_entry`, `review`,
`protected_validation`, `protected_validation_gate` and
`validation_classification` with their referenced schemas. Include `policies/public-deliverables.json` for the
shared capture and validation contract. Include `flows/base-flow/gate-programs.json`
for the common review and validation-classification decision rules. Each
agent-result gate binds those rules to its own evidence precondition, which
checks the freshness and candidate identity of the qualified Build report.
For Codex, include `implementation-capture-gate-v1.json`. Select
`codex-run-result` as the primary report and declare the trusted capture
observation as `candidate_facts` with the `candidate_capture_facts` schema.
The gate verifies that the report and the exact capture are fresh for the same
execution. It forwards the frozen accepted summary and pauses for requirements
review when the agent reports a possible defect in the requirements.
Set the writable profile's `candidate_capture_tool` to the exact trusted
producer accepted for incoming candidates. This selection contributes to the
immutable Codex task identity. The profile's independently
installed capture and signed write paths apply to both initial implementation
and rework.

An installation using `observations` and independent `execution_facts` can
instead include the base Flow's `implementation_result_gate` and its schema.

Configure a separate read-only review Profile. Review gets the accepted
requirements and the current candidate. Validation starts after the review
gate accepts and uses that same candidate with the inventory selected from
the original test author. Include applicable repository checks as required
classes in the installed test policy. Have the installed validation
command run the selected repository checks against the candidate and report
a result for each class. The gate requires a complete passing report.

Review changes and confirmed implementation defects converge on the same
rework edge. Its `max_repeats` defaults to one and counts both reasons
together. The replacement gets the original accepted requirements, public
coverage scope, exact prior candidate and a bounded list of remediation items.
For a review failure it contains the review findings, or a single fallback
when a violated criterion has no written finding. For a validation failure it
contains the classifier's remediation based on public class outcomes.
Each replacement repeats review and
validation with the original protected inventory.

An indeterminate review or validation and exhausted rework wait in an
operator gate with the exact findings or diagnostic input. An operator can
close the review with a reason. The gate cannot authorize another automated
rework or publication. Requirements and test defects end the automatic
branch for an explicit correction cycle. `verified_candidate` is the
successful completion of this branch.

## Pull request publication

To continue from validation to publication, add `pull-request-v1.json`.
Remove the previous
`protected_validation_gate.passed` transition to `verified_candidate`
and its unused terminal in the composed draft before publishing the Flow.
The branch prepares metadata only after validation passes.

Bind `prepare_pull_request` to a reviewed DevOps Profile. The step
gets the original task, accepted summary, review and validated revision.
Its result contains a title and public summary. Give this profile only
the access needed to produce that result. The title must contain text;
the summary may contain multiple paragraphs. Delivery validates both
before contacting the forge.

In `publish_pull_request`, select the exact installed trusted action
and target policy in place of the example references. Include both the
trusted action identity from the step's binding and the provider plugin
identity from the selected policy in the step, Flow and admission access
requests. The same reference suffices if both identities match. Allow
the policy's credential profile and network hosts in the enclosing
ceilings and request them on the trusted action, with no grant on the
DevOps step. The action gets the accepted revision, its original bundle
and manifest, the review and validation deadlines and the prepared metadata.
Before a new forge write, it verifies that the retained review passed and
every class in the protected validation report passed for the same candidate.
The installed target policy controls the repository, target branch and draft mode.

Install the Delivery action with the `published` outcome from this
branch and its exact `pull_request_receipt` schema. Set `review_links`
to `github` on the installed Delivery policy entry, alongside `draft`
and `input_directory`. Use a pinned GitHub repository with its
canonical HTTPS clone URL.
GitHub.com uses `api.github.com` with `github.com`; an Enterprise
installation uses the same HTTPS origin for the API and clone URL.
Installation verifies and pins the browser route for that repository.
The publisher resolves the numeric identity confirmed by the adapter
against that route. A policy change requires a new pinned reference in
the Flow.

The `delivered_for_review` terminal retains the receipt, exact published
head and review URL. A provider conflict, permanent rejection, invalid
review metadata, or an expired validation window without an observed
PR ends at `publication_blocked` with the corresponding reason.
Ticket status changes require a separate
Reporter action. Merge takes place manually in the forge under its
branch policy, including for a production target.

If a publication response is lost, Delivery looks up the original
operation before making another write. If that lookup is unavailable,
the next reconciliation resumes the same recorded attempt. Review
expiry blocks new forge writes. Within the attempt's original deadline
and the current claim, a read-only lookup can still recover the
already-created PR.
