# Operating optional Dark Factory

Ordinary CI/CD is the default deployment mode. Factory adds Work admission and
an immutable Flow owner over ordinary Builds; it does not install a second Job
executor. Use this guide alongside [server operations](server.md),
[Agent operations](agent.md), and the [local stand](local-stand.md).

The separate [OpenAI Decisions shadow guide](openai-decisions-shadow.md)
describes the optional adapter, private installation files and deferred live
experiment. Installing it does not enable Factory or qualify a model.

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

### Project AI-agent compatibility declarations

An optional `role_profiles` map inside each `factory_profiles_file` Project entry
attaches authoring compatibility to existing `node_capability` aliases. Each alias
must already name the exact runtime executable, and its ordinary Build configuration
must belong to that same Project. Existing files omit this map and keep their
previous behavior. Programmatic installers use the same
`FactoryConfigurationChoices::with_role_profile_capability` contract.

Each declaration contains:

| Field | Content |
| --- | --- |
| `ceiling` | An ordinary complete Role Profile definition declaring the exact toolchain, approved argv, skills, MCP/integration bindings, credential purpose, permission and budget ceilings |
| `parameters` | An existing content-addressed `FlowDataSchema` declaring supported model parameters and thinking values |
| `public_environment` | A bounded map from exact environment names to `FlowDataSchema` value contracts |
| `enforcement` | The existing backend permission categories that this installation can enforce |

Public environment values must match their declared contracts; they cannot replace
approved secret entries. Secret entries select only exact installed logical
references. The argv vector must match the installed approved vector, and requested
permissions must survive the existing local permission intersection unchanged.
These declarations install no executor and confer no execution or credential grant.
Names and instructions remain editable Project content rather than installed
compiler instructions.

Application catalogs require visible runtime/model and Build dependencies before
visible counts and cursors. Unavailable optional skills, MCP servers, integrations
and permission selections are removed from the available ceiling, without hiding
the runtime. An MCP selection tied to an unavailable integration is removed too.
Catalogs and dedicated details share this filtered ceiling. Validation checks the
unchanged candidate against it: omitting an unavailable option is valid, while
explicitly requesting it is rejected rather than silently narrowed.
Managed resources remain in the existing Integration
Connection and Project-grant owner: active connection metadata, exact connection and
access revisions, granted resource/purpose and configured credentials are rechecked
without contacting the provider or resolving secret values. They need no duplicate
resource entries in the Factory capability catalog. General collections expose safe
metadata and tool identities; approved argv, command patterns, logical secret
locators and instructions require dedicated authorized content reads.

Role/profile and starter-template catalogs remain available without an enrolled
worker or enabled Factory execution. A normalized compatibility preview retains the
exact role and candidate digest without publishing a version, reserving a mutation
key, or starting a Build. Executable admission and dispatch still require their
ordinary installed compiler, Project and backend authority.

### Reproducible AI-agent execution packages

`FactoryRoleExecutionPackage` assembles an explicit core snapshot from the existing
Task Envelope, Context Manifest, exact role/profile content, ordered Project rules
and selected skill inventories. Project rules reference `policy` inputs in that
same Context Manifest and must match their exact UTF-8 digest and size. Rule,
shared-role and profile instruction layers remain separate, with Project rules
controlling; this records precedence rather than proving that a model obeys text.
All runtime/model parameters, environment/CLI, MCP/integration bindings, technical
pins and stage task/output contracts participate in the versioned package digest.

`FactoryRoleSkillBundle` pins a canonical relative-path inventory using existing
Artifact references, including root `SKILL.md` and supporting files. It reuses
Context file/byte bounds and rejects file/directory conflicts. The package requires
exactly the profile's selected bundle references, the original role and Context
pins, required Context entries and matching task toolchain. Task permissions and
budget may only narrow the selected profile. Shared instruction bounds and the
common 1 MiB document bound apply; restoration repeats validation and rejects
unknown contract versions or substituted content.

Assembly performs no catalog, filesystem, credential or provider lookup and grants
no execution authority. Skill/task file bytes remain under their existing Artifact
owner. Physical skill import/transfer and compiler integration remain tasks 3.2
and 3.3; profile/Run retention remains task 3.5. Callers still qualify
current installed capabilities and authorized inputs before executable admission.

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
