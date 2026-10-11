# Configurable promotion

`../promotion-flow.json` is an editable nested template assembled through the
common manifest loader and immutable publication contracts. Its node names,
strategy, environment aliases, allocation, window, predicates and transitions
are operator configuration. The engine has no strategy or business-node switch.
The caller supplies the exact accepted candidate and retained bundle, manifest
and public-contract references under the closed `release_input` schema.

## Plan and authority

A Build-backed `trusted_action` selects one installed plugin identity and an exact
Build profile whose tool matches that capability. `action.parameters.plan`
contains the bounded `PromotionPolicy`. `PromotionPlan::freeze` derives the exact
candidate from trusted Context, binds the admitted configuration/closure and
limits, complete frozen input (including Flow, cycle and generation), action
budget and actual deterministic/human node named by `criteria`. Restoration compares
these original identities; changing current configuration cannot replace them.

`required_gates` names deterministic or human nodes in the owning definition.
Each must also be selected by an exact Record input binding with its declared
finite outcome. The normal owner/store input-preparation checks recompute those
selections from committed current-cycle records. Do not use a reasoning report
as a gate or copy another Flow's context implicitly. When gates are in the parent
Flow, its declared call transition controls entry; the child explicitly selects
the frozen approved caller input. The full owner contract exercises this layout
with separate independent verification, delivery and human approval before the
promotion call.

`criteria` refers to a configured deterministic or human node, rather than copying
another set of rules into action parameters. Its complete admitted declaration,
finite outcomes, bindings and, for a deterministic gate, actual program are frozen
in the plan. Human disposition uses the same existing owner/control contracts:
the Flow waits for an authorized response tied to the exact input and attempt.
No extra evaluator is required merely to construct a human plan. Configure an
actual health/freshness gate before making human continuation eligible, with a
failed or stale observation routed directly to rollback or escalation. Continuation
must also recheck freshness if a human wait can outlive the proof.
These checks use configured deterministic nodes and authoritative owner time.
An optional Decision Signal may advise a declared decision through a bounded
projection/request. Its eligible routes must remain inside the configured
authorization result: a failed observation cannot become eligible for continuation.
Reasoning and signals receive no deployment or forge-write authority. Human gates
use the existing management authorization, exact input and append-only completion.

## Strategy choices

Strategy strings are passed to the installed deployment Pipeline; they do not
select Factory runtime branches. Operators can publish these choices or additional
strategies supported by another installed capability:

| Choice | Configuration |
| --- | --- |
| Stage only | Select a staging environment and a finite observation policy. |
| Stage plus human approval | Add a Human Gate and select its exact approved Record before the next write/call. |
| Stage plus deterministic evidence | Select the evidence requirements and exact deterministic predecessor outcomes. |
| Canary | Select a production/staging environment, pilot cohort/allocation and bounded continuation/rollback criteria. |
| A/B | Declare the cohorts/allocation and comparisons in typed measurements and deterministic/human policy. |
| Progressive | Configure multiple bounded action/check invocations and explicit transitions or bounded repeats; each invocation freezes its own plan. |
| Immediate low-risk production | Explicitly select `production_environments` and set `allow_immediate_low_risk: true` for ungated production. Immutable admitted Work must have low risk. |

Environment aliases resolve to immutable contracts; a mutable URL/name alone is
insufficient. Every production alias must be in the selected environment map.
Allocation is bounded structured configuration interpreted by the CI/CD profile.
Evidence requirements must match the action's actual Build binding; empty extra
evidence still requires the independently retained primary Build report.

## Ordinary CI/CD execution

Install the exact `factory.deployment-build` capability reference (or publish a
replacement identity) and qualified `deployment-rollout` / `deployment-rollback`
Build profiles. The sample digest is an installation placeholder, not a production
credential. Set each profile's result output to `deployment_receipt`, matching the
explicit metadata projections in both decision nodes. Select an ordinary Build
configuration that materializes the deployment Pipeline; no separate Factory
execution platform is introduced.

`FactoryPromotionBuildAdapter` wraps the common Build adapter. The ordinary Build
request carries the unchanged action binding and bounded protected `action_input`
with `{ "plan": <frozen plan>, "plan_digest": <digest> }`. The installed CI/CD
compiler uses those owner-derived facts, the exact input digest and candidate,
never model-authored replacements. The document is recomputed from original
admission/input on replay. It is absent for model and ordinary command nodes.
CI/CD owns stable deployment identity, environment writes, observation timing,
rollout progression and rollback mechanics. Factory invokes/observes the same
ordinary operation after response loss; cancellation also uses its existing port.
Credentials belong to that trusted deployment system, not model Context or action
parameter bytes. Root installation/live backend qualification remains task 16.1.

CI/CD publishes `PromotionObservation` as the exact declared retained output.
Qualification checks actual Build/Attempt/Job/tool/plugin/schema provenance,
input/plan/candidate/environment identities, timestamp bounds and freshness.
Measurements must finish no later than publication of the immutable primary
receipt. The common Build contract independently checks publication before trusted
verification and verification before owner observation. Re-verifying old bytes
later cannot make a report about future measurements valid.
Passed observations must cover the complete selected window; failure may stop
sooner. The sample's one-millisecond window is illustrative; publish operational
windows inside the action and enclosing budgets. Reconcile/restart never extends
an attempt deadline. Generic Build intents reserve against the aggregate admission
budget; owner and store independently check each owning Flow's local reservation.
Sibling Flow consumption is not charged to a child's local budget.

## Bounded disposition and monitoring

The sample decision inputs combine typed CI/CD observations with the actual
verified output freshness deadline through explicit Record metadata projections.
Both policies compare that deadline with the authoritative owner
`observation_time`; a passed but expired report cannot continue or finish rollback.
Only configured transitions produce `promoted`, `rolled_back` or `escalated`.
The failed-observation path invokes another ordinary CI/CD Build for rollback.
Other measurements, profiles, human decisions, routes and terminal names can be
published through the same contracts.

Long-running monitoring is external. A report can submit a new task Artifact
through ordinary authorized Work admission, with its own identity, configuration,
claim and both triage phases. It cannot append to the completed rollout, change its
plan or reuse its owner claim. The full nested owner tests cover the small accepted
contract and research/specification path, independent black-box checks, review
publication, human approval, bounded rollout/rollback, restart from retained state
and adding/removing/reordering optional declared nodes by configuration alone.
They also configure a separate human post-observation disposition and verify it
waits for the exact authorized response before advancing.
