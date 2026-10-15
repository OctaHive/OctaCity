# Operating optional Dark Factory

Ordinary CI/CD is the default deployment mode. Factory adds Work admission and
an immutable Flow owner over ordinary Builds; it does not install a second Job
executor. Use this guide alongside [server operations](server.md),
[Agent operations](agent.md), and the [local stand](local-stand.md).

## Choose and observe the mode

| Observation | Meaning | Operator action |
| --- | --- | --- |
| `/health/ready` succeeds and `/health/factory` reports `disabled` | Ordinary CI/CD is available; no Factory owner is installed | Use ordinary Builds, or deliberately prepare an opt-in installation |
| Both readiness scopes succeed | Shared dependencies and installed Factory capabilities are healthy | Check the exact Configuration and qualified Agent inventory before admitting Work |
| Ordinary readiness succeeds; Factory reports `unavailable` | An optional Factory worker or installed capability is degraded | Investigate that capability; keep ordinary CI/CD available |
| Shared readiness fails | A required shared dependency is unsafe for mutations | Follow the server incident runbook before admitting either mode |

Health is an observation, not a permission grant or capacity reservation. A
healthy model endpoint does not qualify an Agent/backend, validate a Flow, or
prove that a candidate passed required verification.

## Prepare an explicit installation

The server requires an operator-owned `factory_profiles_file` or an explicit
trusted runtime installer, together with bounded `[factory]` worker settings.
Install only exact profiles and capabilities needed by the published Flow.
Unknown configuration, invalid bindings and missing required authority fail
validation. An installation timeout can leave Factory unavailable while ordinary
server startup continues; recovering missing installation material in that
initial state requires a server restart.

The [runtime installation contract](../architecture/server.md#optional-factory-runtime-installation)
defines the profile catalogue and composition boundaries. The
[local-stand procedure](local-stand.md#optional-factory-installation) shows the
supported private-file inputs. Server-side Signal connectors use a separate
private catalogue and credential file. Never put a provider key, private URL,
raw provider payload or mutable model alias into a Flow or management command.

Keep authoring, implementation, protected validation, review, verification,
delivery and promotion profiles independently scoped. Install the pinned Codex
and task/source plugins, exact question/model identities, read-only inputs,
permission ceilings, original deadlines and resource/output limits. A role name
does not create authority: the installed compiler, profile, local Agent policy
and backend must all agree. Delivery and promotion credentials must not enter
implementation, protected-test authoring or ordinary source reads.

Agent registration must advertise the exact v3 semantic enforcement route
needed by each Factory Job. Host, Apple VF and Windows WHP preview do not gain
Factory qualification from their ordinary CI/CD support. Missing v3, a blocking
tool hook, workspace anchoring or another required capability leaves that Job
unscheduled or rejected; it must never select a weaker route. See
[JobSpec v3](../reference/protocols/signed-job-spec-v3.md).

## Admit and follow one Work item

1. Select the owning Project, exact enabled Factory Configuration and Repository
   version. Confirm that the immutable Flow closure, budgets, permissions,
   profiles, input retention and mandatory verification are the intended ones.
2. Prepare task and acceptance Artifacts through an ordinary Build and select
   their exact retained references. The current manual intake requires those
   inputs. Direct-text quickstart and a unified Work card are separate proposals,
   not prerequisites already supplied by this release.
3. Admit Work through [management REST](../reference/management-rest-v1.md#optional-factory-management)
   or **Admit Work** in the [Factory workbench](operator-console.md#manual-work-and-run-investigation).
   Keep the confirmed request and idempotency key for exact transport replay;
   never silently resample a branch, input or Configuration after an unknown
   response. The server returns the original Work/Run and frozen base revision.
4. Follow the Run's current Node Attempts and linked Builds. The pinned graph
   explains permitted control and Context/data routes; the execution journal
   records what happened. An absent row on a bounded page does not prove absent
   work. A model recommendation or completed implementation Build does not mean
   the candidate is accepted.
5. Answer a declared HumanGate or bound operation with its exact typed value,
   target and current Run precondition. A recorded command is intent awaiting
   owner processing, not a successful node, delivery or deployment. A stale
   response requires explicit refresh and review.
6. Inspect deterministic evidence, independent Assessments and the Decision for
   the exact candidate. Retain its ChangeSet and immutable result references.
   Delivery for review and policy-qualified promotion remain separate actions;
   no screen or model summary grants merge authority.

Every step is available headlessly. The Console has no separate lifecycle
authority, and leaving an unconfirmed form publishes no Work or configuration.

## Provider failure, cancellation and recovery

In bounded control, treat timeout, invalid result, unsupported capability, low
confidence and unknown provider outcome as classified failures under the frozen
fail-closed fallback. Shadow/advisory retain the deterministic baseline and
record the unusable observation without granting the signal authority. Keep exact
provider/model/policy and logical request identities. Observe or replay the
original operation; do not substitute a model or dispatch the same logical
decision again merely because its response was lost. Server-side JEV credentials
remain outside Agent workloads and protected tool operations stay blocked until
their broker returns a valid recorded disposition.

Disabling a Configuration prevents fresh admission; already admitted Runs retain
their pinned versions. To stop existing Work, use the exact Run cancellation
command. Draining capacity stops new leases and is not a replacement for cancelling
an active Run. Confirm terminal stop and retained measured usage through the
journal before treating effects as stopped. Never fabricate a completion from a
killed process or delete claim rows manually.

On restart, restore the same immutable profile/connector identities and let fenced
recovery use durable inputs, receipts, original deadlines and budgets. A missing
provider session is not missing authoritative state. Reporter retry/failure affects
only its independent outbox, not an accepted Flow result. For disaster recovery,
use the complete [backup/restore procedure](backup-restore.md); preserve all
Factory metadata, compiler-input objects, private configuration references and
retained candidate/evidence objects in the same consistency unit.

## Signal rollout and release qualification

The [calibration seed and comparison guide](../../tools/fixtures/factory-signal-calibration/README.md)
documents the offline routing/tool-risk corpus and comparison command. Use it
to prepare and inspect measurements; its reports cannot authorize promotion.

Routing and tool risk have independent `shadow`, `advisory` and `bounded_control`
policies. Shadow records comparison only; advisory exposes a recommendation while
the deterministic baseline acts. Do not treat either as permission to enable
bounded control. Require labeled purpose-specific corpora, exact provider/model
and probability semantics, comparison and drift/failure/latency limits, an audited
promotion, and demonstrated immediate demotion/fallback. A fixture endpoint tests
the adapter; it cannot calibrate or approve a real model. If any of those gates is
unqualified, retain shadow/advisory and the existing deterministic policy.

Retain release evidence for two independent modes:

- Ordinary CI/CD: Linux/macOS/Windows portable checks, old v1/v2 canonical wire
  contracts, packaged Host and qualified ordinary backends, headless REST,
  non-Factory Console activities, migrations, retention and local-stand lifecycle.
- Opt-in Factory: signed v3 positives/negatives on qualified real backends,
  protected input/tool boundaries, exact nested Flow/candidate/evidence lineage,
  independent agents, server/Agent restart, unknown responses, cancellation,
  audit/retention and required delivery/promotion qualification.

Use the pinned release tools and locked dependencies. Run formatting, architecture,
strict Clippy/rustdoc, tests/coverage, security scans/fuzzing, locked Console quality
checks and package verification. Record the source revision, dependency/plugin
identities, commands, outcomes and retained artifacts. A skip is not a pass; an
old successful revision cannot attest to a later correction. See
[backend qualification](../testing/backend-contracts.md) and
[security release gates](../testing/security.md).

The backend workflow intentionally runs packaged Host contracts on push. Its full
hosted matrix runs on the nightly schedule or manual `hosted` suite; self-hosted
platform qualification needs the documented provisioned runners. Do not infer
full Linux performance or Factory qualification from the push workflow alone.

External ticket discovery/reporting and an Octa-local JEV task retain the
[separate extension boundaries](../architecture/factory-extension-boundaries.md).
