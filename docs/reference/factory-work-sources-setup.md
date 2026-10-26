# Configure Factory task sources and reporting

The setup path is **Integrations → Project access → Factory pipeline inputs →
Flow actions → Run/reporting history**. Choose **Admit manual Work** in Factory
for the existing Artifact-based intake; that path needs no connection or token.
Manual internal task-card creation/execution belongs to `add-factory-work-card`
and remains independent of integrations when that capability is enabled.

## What each setting owns

| Setting | What it selects |
| --- | --- |
| Integration connection | A reusable named installed provider, endpoint, native resources and credential references. It does not select a pipeline. |
| Project access | Which native resources an OctaCity Project can use, separately for task reads, task updates, repository reads and Delivery. |
| Automatic input | A stable intake identity, exact Factory target/code Repository, installed Source/Reporter profiles, filtering and immutable reporting settings. |
| Flow action | An accepted configured stage outcome can enqueue exact source effects through the existing outbox. |
| Optional task card | Its owning capability supplies persistent task identity/revisions, source association, import/refresh policy and explicit execution requests. |

A GitHub repository used as a ticket container is identified by its native
repository ID with owner/name display context. It is distinct from both the
OctaCity Project and the code Repository used to build a result. Several inputs
can reuse one connection, and the code Repository may use another host/provider.
Only actually installed capabilities are usable. Native GitHub is implemented;
GitLab/Jira/custom peers in contract tests qualify the common envelope, not native
adapters for those services.

## Installation owner: make capabilities available

Connection setup selects installed material; it does not install executable files
or bootstrap deployment authentication. The deployment owner supplies:

1. A real, confined adapter registry directory with a regular executable and
   validated `adapter.toml`, including its SHA-256, independently declared Source
   v1/Reporter v2 capabilities and finite installed resource/network ceilings.
   Use the [Work Source/Reporter contract](protocols/work-source-reporter-v1.md).
2. The native GitHub plugin's private `github-work.toml` in its installed working
   directory. It records the API endpoint/version, native repository IDs and
   owner/name aliases, separate Source/Reporter repository allowlists and confined
   credential/selector/mapping directories. `GithubWorkProvider::load` validates
   the settings; changing these bytes changes its configuration digest. Keep the
   directory/files private (owner-only access, conventionally `0700`/`0600`), with
   regular files and no path or symlink escape. TLS verification remains enabled.
3. Adapter-validated immutable selectors, input/acceptance mappings and reporting
   mappings. Their exact references/digests must match the manifest and hosted
   profiles. A bounded explicit acceptance mapping is required; missing Issue
   content must not be replaced with invented acceptance criteria.
4. Independently installed purpose/resource handles and the protected credential
   publisher. GitHub's `work_source_read`, `work_reporter_write`, `vcs_read` and
   `delivery_write` classes remain separate even if remote token permissions are
   broader. Installed logical references map to private handles; clients cannot
   submit a credential path or widen the allowed repository aliases.
5. Protected management/discovery/access/check/source-read services, Source worker,
   preview and Reporter composition. Managed operations resolve current connection
   authority rather than borrow a legacy installed profile. Stage-owned effects
   also require the existing source authorizer on Flow and Factory Configuration
   publication. The console reports unavailable services; a browser header cannot
   supply verified ingress or install a profile.

For editable normalized predicates, the installed Source profile must explicitly
advertise permitted states over its unrestricted base selector. This supports only
narrowing the installed scope. For editable reporting, both the server profile and
native adapter must support the bounded managed label/state ceiling; ordered
Reporter v2 must advertise `custom_effects`. Otherwise select the available frozen
installed settings instead of treating an unknown description as editable.

The released server can explicitly load the protected connection-management
catalog described in [integration management contracts](integration-management-contracts.md#released-server-composition).
For the personal stand, see the [GitHub installation command](../operations/local-stand.md#github-connections-in-the-personal-factory-stand).
Automatic input, preview and Reporter execution retain their separate installed
profile contracts. Public composition types and the canonical native test
installation are linked in the [example README](../../config/factory/work-sources/README.md).

## Operator: set up a connection and private credentials

Open **Integrations**, choose an installed provider and its approved endpoint,
authentication method and native resources. Give the connection a useful display
name, validate, review and publish. The metadata version starts without an implicit
Project grant or enabled pipeline input. Display names are editable context, not
native or replay identities.

Use the separate credential workflow on the connection page:

| Operation | Use it when |
| --- | --- |
| Provision | Configure an absent installed purpose handle with a transient token. |
| Adopt | Use an existing operator-installed handle without reading or resubmitting its value. |
| Rotate | Replace material behind that same logical purpose/resource handle. |
| Revoke | Make the selected private handle unavailable; this cannot request activation. |

Select only an authentication method advertised by the installation. Submit raw
values only through verified protected management transport; never put them in
pipeline parameters, ordinary metadata, URLs, committed configuration or chat.
The form clears secret input and provides only safe configured state/receipts.
There is no token read-back. The supported protected GitHub file publisher runs on
Linux/macOS; unsupported storage/platform installation is an unavailable capability.

Credential preparation and metadata activation are recoverable stages. If private
activation fails, inspect the safe administrative receipt/state and retry the
original operation; do not create a new connection or an untracked credential copy.
Recovery can resume without secret resubmission. Completed operation replay returns
its original receipt and cannot reinstall an earlier token after rotation/revocation.

Connection activation is explicit. Credential administration begins with the
connection disabled; if continued use is intended, review activation with the
credential operation or activate after it completes. Use the current administrative
revision, not the metadata or Project-access version. Rotation preserves logical
scope, input/native identity and frozen pending effects; endpoint/resource changes
need explicit metadata replacement and do not redirect accepted Work.

All selected credential scopes must be configured before activation. For example,
when read and update use separate handles, save the first token without activation;
then configure the other handle and explicitly activate. The Console keeps both
activation controls unavailable while another required scope is missing and
explains that token storage remains available. A single-scope connection can
still request activation together with its first token. This prerequisite is
separate from checking the provider's actual permissions.

Connection details list configured/missing credentials by resource and purpose.
**Manage credentials** initially selects the first missing scope with **Provision**,
instead of offering rotation of the already configured read scope. Separate handles
do not require different provider tokens: the same value may be entered for both
if its remote permissions allow both purposes. Configuration is not proof of those
permissions. For a read-only connection, remove the update purpose in **Edit connection**.

## Grant the Project access and check readiness

On the connection page, choose **Manage Project access**, select the exact OctaCity
Project, then select native resources and required purposes. Publication requires
administration authority over both the connection and Project. A pipeline editor
does not acquire those rights by selecting a connection.

Grant `task_read` for automatic intake. Grant `task_update` separately for Reporter
effects. Code repository reading and Delivery keep their own rights; selecting the
same connection or possessing a broader token does not combine them. Review the
exact connection and current access revision before publication.

Run **Check access (read-only)** for each required resource/purpose. Review
configuration, authentication, native identity/access and capability readiness
separately. GitHub can confirm read readiness while leaving write capability
unverified. A check does not mutate a label/state, grant access, activate polling,
advance cursors, admit Work or refresh a task card. A throttled/unavailable response
is not a successful empty source. Live write qualification uses the separate
[opt-in disposable-repository procedure](github-work-source-live-qualification.md).

## Configure and review automatic inputs

In **Factory**, inspect the exact Factory Configuration and open **Pipeline
inputs → Configure automatic intake → Add automatic input**. Select:

- The permitted connection/native task container and installed selector/input mapping.
- The code Repository independently, with its server-supplied immutable definition
  digest; the target Factory Configuration already pins the root Flow closure.
- An advertised state and literal all/any/excluded labels, if editing is supported.
- An optional compatible Reporter with separate update authority and reviewable effects.
- Enabled state only after preview, validation and confirmation of the complete draft.

New examples and UI inputs start disabled. An input filter matches when all required
labels are present, at least one alternative is present if configured, and no excluded
label is present. Empty label lists impose no label restriction, including unlabeled
tasks; state remains independent. Contradictory/bounded/unsupported settings are
rejected. Provider labels select an input, not an authoritative business outcome or
a way around the target Flow's eligibility/duplicate/acceptance policies.

**Preview** is a bounded read of this exact intent. Inspect selector reasons,
unavailable versus empty, existing visible Work/Run references and group ambiguity.
It neither publishes the input nor imports a card/reserves execution. Validate
against current server authority, review all immutable references/effects, then
confirm publication. A draft change invalidates earlier validation. A stale-version
response needs a refreshed baseline and a new review; never silently repin a target
or a group member. Drafts and preview payloads remain outside browser persistence.

## Editable routing examples

Use the [four JSON settings fragments](../../config/factory/work-sources/README.md)
as values for the existing input's `enabled`, `predicate` and `reporting` fields.
They contain no credentials or fabricated installation/target IDs. Select the
remaining authorized references in the UI/API; these fragments are not standalone
POST bodies or automatically loaded input definitions.

| Example | Selection | Accepted configured effects |
| --- | --- | --- |
| Assessment | Open tasks, including unlabeled tasks; exclude `approved`/`rejected`. | `accept-defect` adds `approved` + `bug`; `accept-feature` adds `approved` + `feature-request`; `decline` adds `rejected`. Remove conflicting managed values; preserve native state. |
| Defect | Require `approved` + `bug`, exclude `rejected`. | `implementation-complete` adds `implemented`, preserving unrelated labels and native state. |
| Feature | Require `approved`, any of `feature-request`/`enhancement`, exclude `rejected`. | Independently editable completion mapping, initially adding `implemented`. |
| Documentation | Require `ready-for-review` + `documentation`, exclude `investigating`. | `documented` adds `documentation-done` and removes `ready-for-review`, with no approval/defect vocabulary. |

Select separate target configurations and stable input IDs for assessment and
each implementation path. Put defect and feature inputs in an explicit exclusive
group using their exact immutable member versions if they are alternatives. A task
matching both remains ambiguous and starts neither target. Independent inputs
outside that group may each admit it once. The sample status/label names are all
ordinary editable configuration. The native adapter requires mapped repository
labels to exist; it does not create them automatically.

An input mapping is a default for a configured `set_status` action; it does not run
when a task merely matches a label. In the existing Flow editor, route an accepted
declared stage outcome to the installed deferred status action. Its published
logical status selects one exact mapping row. For per-outcome additions/removals,
configure the typed stage `source_update` and review the original Source/Reporter/
connection/effect pins. It overrides that input default for the accepted action;
different outcomes may select different action nodes. Model output is schema-checked
data; prose alone cannot call a provider action. Stages without actions write nothing.

Effects can follow intermediate or nested stages before the Run completes. A stage
action's explicit input binding to the accepted predecessor/record supplies provenance.
Input activation validates compatible pins across the full target closure. Reporting
keeps the original native target/mapping after input edits or later card association.
See [typed stage effects](integration-management-contracts.md#stage-local-source-effects).

Normal server startup dispatches these Work effects through the same installed
Reporter inventory and ordered outbox as Issue push. A terminal status still needs
its own configured action: finishing a Run creates no implicit update. Reporting
diagnostics retain each action's definition, invocation, cycle and attempt, plus
only its explicitly consumed predecessor records. Preserve one producer for a
business decision: overlapping Issue push and adjacent Work source-update effects
are rejected; unrelated native labels remain outside the reviewed effect scope.

## Versions, repeats and optional task cards

| Operation | Resulting identity/behavior |
| --- | --- |
| Edit selector, target, mapping or enabled state under the same input ID | Publish a new immutable version; original Work/Run and pending effects stay frozen. An already admitted native task is not automatically processed again. |
| Rotate credentials or rename the connection | Preserve input/native/replay identity; recheck current grants before the next protected use. |
| Intentionally create a new automatic intake cycle | Explicitly review a new stable input ID. This is a new admission scope and may re-admit matching tasks; ordinary edits/retries must not allocate one. |
| Retry an existing execution | Use the existing Run/attempt policy rather than alter source identity or replay key. |
| Observe a linked task after it changes/stops matching | A separately authorized consumer can request current source provenance. Observation alone creates no Run and does not replace frozen Work Artifacts. |
| Execute from a future internal card | The card owner accepts a distinct explicit execution request and frozen revision; it does not reset an automatic input identity. |

Native correlation uses the trusted Project/security partition, installation,
native container and item. It deliberately excludes the input ID so assessment and
implementation can correlate one external task, while each has its own admitted
Work/Run. It is neither the admission key nor a task-card ID. A visible card link is
optional; original Work/Run navigation remains usable without it.

`add-factory-work-card` owns durable Issue creation/revisions, association uniqueness,
import/link-refresh policy, field mappings/ownership, conflict resolution, and manual
card execution. Execution filters here are not a global import/refresh policy.
Disabling an execution input stops its new admissions; separately configured refresh
still needs its own enabled policy and current read grants. With no consumer there
are no extra provider reads or task-store dependencies. Manual card creation must
remain possible without integrations when that capability is available.

## Read history, recover and roll back

Open **Source and reporting progress** on the exact input/group version. Follow its
original Work/Run links and inspect accepted stage/action correlation. Keep task
business status, execution result and reporting health as separate facts:

| Observation | Operator action |
| --- | --- |
| Preview selected, no admission receipt | Check eligibility/scan/disposition; a label match is not a Run. |
| Filtered or ambiguous group | Review the predicates/member versions and authoritative selection reasons; no default target is guessed. |
| Missing/denied/unavailable profile or grant | Repair the exact installed scope/access; do not copy a different credential or redirect pending effects. |
| Throttled/transient/timeout | Respect the retained retry time and original finite deadline. Do not increase timeouts or renew the operation to hide failure. |
| Pending/claimed reporting | The outbox has not confirmed the external effect. This is independent of a completed stage/Run. |
| Applied reporting | Native effects were confirmed for that accepted operation; downstream admission still needs its own receipt. |
| Conflict/superseded/exhausted reporting | Inspect managed-field ordering, original pins and possible human edits. Preserve unrelated labels and accepted Work; reconcile within the existing policy instead of manually replaying an older provider request. |
| Administrative operation interrupted | Retry the original safe operation identity/precondition; inspect private preparation state without exposing a token. |

Rollback starts by disabling the affected automatic input IDs to stop future intake
and pausing only the relevant Reporter dispatch if required. Revoke selected Project
purposes or disable the connection when access should stop. These operations block
subsequent protected calls, including retained retries; they cannot recall an already
sent remote request or erase accepted history. Inspect authorized dependencies before
retirement and retain referenced metadata/private material, original mappings, outbox,
scan progress, Work/Run and audit. Only expired unreferenced private preparations are
eligible for bounded cleanup. Do not automatically reverse external labels or create
new input identities as rollback. Resume under reviewed current authority and the
original finite recovery policy.

The [diagnostic contract](factory-source-diagnostics.md) describes bounded safe
history, and the [live procedure](github-work-source-live-qualification.md) limits
any remote cleanup to resources whose ownership was recorded by that pilot.
