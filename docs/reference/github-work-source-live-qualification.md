# Opt-in live GitHub Work Source qualification

This procedure qualifies one explicitly selected disposable GitHub repository
through the installed Source/Reporter and ordinary Factory execution. It is an
operator-run procedure, separate from the hermetic
[`factory_work_source_pilot`](../../server/app/tests/factory_work_source_pilot.rs).
A passing hermetic test does not qualify a live account, deployment or token.

Start only after the repository owner selects the test repository and the
deployment owner supplies its installed scope. A project checkout, Git remote,
GitHub CLI login, environment token or existing connection does not establish
that selection. Missing prerequisites mean **blocked / unqualified**; no check,
preview, activation or remote mutation should be attempted with inferred inputs.

## Record the exact qualification scope

Create a private operator evidence record with the following safe metadata.
Fill every required value from authoritative deployment/provider records before
starting. Keep tokens, credential files, raw request bodies, task bodies and
private installation paths out of the report.

| Required scope | Record and verify |
| --- | --- |
| Authorization and ownership | Operator, repository owner's explicit disposable selection, permitted test actions, isolated deployment and evidence time. |
| Service identity | Exact API endpoint, installed GitHub API version and permitted network hosts. Keep TLS/hostname verification enabled. |
| Native repository | Immutable native repository ID and current owner/name. The installed alias and endpoint must resolve to that same ID; a familiar name alone is insufficient. |
| Installation and executable | Exact installation reference, adapter reference/version, executable SHA-256 and manifest Source/Reporter protocol/capability declarations. |
| Connection | Stable connection ID, metadata version, display name, active metadata version and administration revision. A display name is not an identity. |
| Project access | OctaCity Project ID, exact connection version, current access revision, native resource reference and separate `task_read` / `task_update` grants. |
| Private credentials | Configured state and safe logical read/write references, authenticated protected management ingress, independently installed purpose/resource classes. Record no token values or secret digests. |
| Inputs and native mapping | Stable assessment/downstream input IDs and immutable versions, exact Source/Reporter bindings and parent connection configurations, provider configuration, selector and mapping digests. |
| Execution target | Exact Factory Configuration/root Flow closure, code Repository ID/version/definition digest, immutable base and installed deterministic fixture-output capability. The code Repository is independent of the ticket container. |
| Bounds | Finite page/item/byte/operation limits, retry/deadline/retention policy and a bounded observation window selected for this run. Do not increase limits to force success. |
| Owned resources | Unique run marker, exact native test Issue ID and number, every created label and its original ownership evidence; original labels/state and expected effects. |

Use a disposable repository and isolated Project/connection/inputs so preview and
polling cannot unintentionally admit unrelated tasks. Only synthetic task content
belongs in this qualification. Keep the existing manual Artifact intake usable
without a provider connection. Internal cards are optional and are not a setup step.

## Confirm the installed deployment before provider access

The deployment owner must supply the existing protected integration management,
Project-access, read-only check, input publication, preview and diagnostics services,
plus managed Source and Reporter runtime composition. The standard console cannot
install an adapter or establish verified ingress by sending a header. A missing
service, unsupported manifest or unavailable installed profile is a blocker.
See [integration management contracts](integration-management-contracts.md).

The verified adapter registry checks the executable pin and installed manifest;
the native plugin loads private `github-work.toml` from its installed working
directory. Its native repository IDs, endpoint, network ceiling, selector/mapping
files and independently scoped private handles must agree with the server's
installed profiles. The operator checks these pins without exporting credential
material. A directory or environment variable containing a token is not a grant.

Require Source v1 with the read-only check/preview operations used below, and
ordered Reporter v2 for Factory status effects. Require `custom_effects` only if
using the supported input/stage-owned editable mappings. Keep the installed label
and state editing ceilings explicit; do not downgrade to bulk Reporter v1.
Use the [process contract](protocols/work-source-reporter-v1.md) for manifest and
wire definitions rather than inventing an installation configuration format.

Before activation, the owner prepares only the agreed fixture resources through
their normal authorized GitHub administration workflow. Create a synthetic Issue
and pre-existing, uniquely named run-marker/ready/rejected labels plus an unrelated
label to preserve. Record exact native IDs and created resources immediately.
The Reporter does not create missing repository labels. Restrict installed
selectors/input predicates to the owned run-marker label and this isolated
repository; do not start an unrestricted production input as a smoke test.

## Read-only phase

Use the existing console review flows or a client generated from the selected
deployment's `/api/v1/openapi.json`. Substitute recorded IDs and typed request
values, not the canonical synthetic fixtures' IDs. The relevant operations are:

| Operation | Expected evidence |
| --- | --- |
| `GET /api/v1/integration-installations` | Visible installed GitHub declarations match the reviewed endpoint/resource/executable scope. Unsupported providers are not usable choices. |
| `GET /api/v1/integrations/{integration_id}/versions/{version}` | Exact retained connection metadata matches the recorded immutable pins. |
| `GET /api/v1/integrations/{integration_id}/state` | Intended active version and independent configured credentials are present; no secret is returned. |
| `GET /api/v1/integrations/{integration_id}/projects/{project_id}/access` | Current explicit Project/resource/purpose grant matches the reviewed access revision. |
| `GET /api/v1/integrations/{integration_id}/projects/{project_id}/resources` | Choices contain only permitted resources and purposes. |
| `POST /api/v1/integrations/{integration_id}/checks` | Check each exact resource and purpose; distinguish configuration, authentication, resource access and capability readiness. No remote mutation, grant, polling or admission occurs. |
| `GET /api/v1/projects/{project_id}/factory-source-choices` | Source/Reporter choices and parent configurations agree with the connection/access pins and editing ceilings. |
| `POST /api/v1/projects/{project_id}/factory-sources/validate` | The reviewed disabled input/group intent validates against its exact targets and current authority. |
| `POST /api/v1/projects/{project_id}/factory-sources/preview` | Bounded safe summaries identify the owned Issue and selector reasons; no input publication, production cursor advance, task import or Run occurs. |

Read-only GitHub Reporter checks can confirm authentication/resource identity
while leaving write capability **unverified**. Do not mark it ready from a token's
claimed permissions or test it by creating a label. Only the reviewed write phase
below can supply live write evidence.

Retain before/after safe diagnostics and the owned Issue's labels/state to verify
that checks/preview made no changes. Empty, filtered, ambiguous, rate-limited and
unavailable results remain distinct. A preview match or an empty visible admission
lookup does not prove admission or the absence of hidden history. See
[preview and diagnostics](factory-source-diagnostics.md).

## Reviewed execution and write phase

1. Review a finite experiment: the exact owned Issue, two stable input identities,
   separate exact assessment/downstream targets, literal logical statuses, label
   additions/removals and optional native state effect. Configure deterministic
   fixture assessment outputs through existing installed Flow execution; no paid
   model is needed. Do not infer an action from Issue prose or a model response.
2. Start with the assessment input requiring the owned marker and excluding the
   configured ready/rejected labels. Downstream requires marker plus ready and
   excludes rejected. These are editable example predicates, not built-in business
   states. Revalidate and publish reviewed enabled versions under their exact
   current-version preconditions and stable idempotency keys. Keep a Source pass
   paused/bounded until the scope is reviewed. If that control is unavailable,
   stop before enabling inputs.
3. Run one bounded Source pass. Retain the authoritative assessment Work/Run and
   original input version. Verify that no downstream admission exists yet. Preserve
   the accepted snapshot/Artifact references, resolved code base and native
   correlation; do not fetch arbitrary linked content.
4. Let ordinary Factory execution accept the configured fixture result and trusted
   status action. Retain its exact action/stage/operation provenance and queued
   outbox identity. Before Reporter dispatch, a pending projection is not an
   applied native effect. Never invoke a raw Reporter request as a shortcut around
   accepted action history and the live outbox fence.
5. Dispatch through the existing managed Reporter. Verify exactly the reviewed
   additions/removals, optional state effect and preservation of the unrelated
   label. Record safe applied/retrying/exhausted diagnostics separately from
   business status and Run execution. A failed projection does not undo assessment.
6. Read again under the original Source scope. Verify one downstream Work/Run with
   its own input identity and the shared native correlation. Neither admission
   requires an internal card. Replay the bounded pass/reconstruct the worker and
   verify that edits and the system's own labels create no third Run or new input
   identity, and both original Work snapshots remain frozen.
7. Exercise rejected/excluded and ambiguous grouped selection only with additional
   explicitly owned fixture Issues or reviewed changes within this disposable
   scope. Record no downstream admission for rejection or a multiple-match group.
   No label spelling may choose a target outside the published input configuration.

## Rotation, revocation and compatibility

Rotate only the independently installed credential reference selected for this
qualification through the protected write-only credential workflow. Preserve
connection/input/native identity and exact configuration scope. Use the current
administration precondition, a stable administrative operation key and explicitly
review connection reactivation if continued use is intended. Clear transient secret
input; store only the safe receipt. An exact old command replay must return its
original receipt without restoring old authority.

For a retained projection, preserve its original operation, deadline, native target
and mapping across rotation/restart. Confirm effects before further writes after
an unknown response; do not manufacture response loss by disrupting shared network
or services. If no isolated fault mechanism exists, retain the hermetic lost-response
qualification separately and mark the live fault variant **not exercised**.

Revoke the test Project's update purpose and verify that a subsequent retained
dispatch makes no new native write, while history remains inspectable. Revoke its
read purpose before a later Source/optional observation call and verify safe denial
with no credential fallback. Record these independently of remote token permissions.
Perform these changes only on the owned Project/connection, not a shared production
grant. Preserve ordinary manual Artifact admission during this unavailable period.

If an optional task consumer is explicitly enabled, it can use the
[existing observation port](work-source-observations.md) to read fresh authorized
provenance after admission or selector mismatch without changing frozen Work or
starting a Run. With no consumer there must be no extra Source read or task-store
prerequisite. Persistent cards, manual card creation/execution, import/refresh
policy and field conflicts remain in `add-factory-work-card`; this live procedure
does not add a provider-specific path to those capabilities. Native GitLab/Jira
qualification remains separate from the common contract peers in the hermetic pilot.

## Stop and clean up within the recorded ownership scope

Disable only the pilot's input identities before changing remote fixture data;
stop/pause only its own polling and Reporter dispatch. Account for any already
accepted or in-flight effect rather than treating disablement as remote rollback.
Retain Work/Run, original mappings, outbox/cursor history and safe audit evidence.

Before each remote cleanup, verify the endpoint, immutable repository ID and the
exact Issue/resource ownership recorded during setup. Close only owned fixture
Issues and remove only labels/effects created by this qualification. Preserve
unrelated labels/state and other users' work. A label name/prefix match alone is
not ownership evidence. Retain repository-wide labels by default; removal requires
proof they were created here and are unused by other resources. If ownership or
concurrent state is uncertain, record manual cleanup pending and stop that action.

Revoke only the pilot's grants, disable its connection, and retain referenced
metadata/private material according to existing dependency-aware lifecycle rules.
The preparations-cleanup endpoint handles expired unreferenced private preparations;
it does not delete remote Issues or replace runtime-material retention. Do not
delete a repository, wipe provider directories, reverse all managed labels or
garbage-collect retained Work to make the test appear clean.

## Qualification result

Record safe exact references, before/after observations, read-only evidence,
accepted operations and Work/Run IDs, expected versus observed effects, replay and
rotation/revocation results, owned cleanup receipts and any unexercised variants.
Only observed passing steps receive **passed**. Missing prerequisites are
**blocked**, a detected mismatch is **failed**, and an unrun optional variant is
**not exercised**. Report the live deployment **unqualified** until its required
checks actually pass; never promote the hermetic result or this procedure's
existence to live activation evidence.

For this change's recorded preparation and missing inputs, see
[task 6.2 evidence](../../openspec/changes/archive/2026-10-18-add-github-work-source/verification-6.2.md).
