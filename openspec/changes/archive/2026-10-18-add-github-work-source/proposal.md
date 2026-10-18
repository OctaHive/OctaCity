## Why

Factory needs configurable external task sources and reusable provider connections so operators can configure access once and select it for several pipelines. GitHub Issues is the first native implementation; the connection and Project-access model must accommodate GitLab, Jira and other providers while preserving manual Factory intake and existing execution authority.

## What Changes

- Add a dedicated provider-neutral **Integrations** section for named connections, installed provider selection, service endpoint, authentication, permitted external resources and safe connection checks. Add a separate privileged write-only secret setup/rotation path; pipeline forms select references and never request credentials.
- Separate reusable connection configuration, explicit access for an OctaCity Project, and pipeline input configuration. Keep external ticket containers independent of the code Repository; a future Jira task source can target code in GitLab. Discover only installed capabilities and authorize task reads, task updates, repository reads and Delivery independently.
- Add versioned Project-owned Work Source inputs selecting an authorized connection/resource, bounded predicates, exact Factory Configuration/root Flow and explicit input/reporting mappings. Preserve stable input identity across replacement and credential rotation; reuse ordinary admission, eligibility/business duplicate checks and the reporting outbox.
- Keep provider observation separate from pipeline admission. Preserve native task identity and bounded snapshot provenance for optional consumers such as the internal Issue card planned by `add-factory-work-card`; one external task can correlate multiple input-scoped Work/Run records without using the pipeline input as the task identity. Internal Issue creation/revisions, execution requests and ongoing linked-task synchronization remain owned by that change.
- Extend the existing `octacity-github` plugin with independently scoped Source/Reporter capabilities and bounded restart-safe polling. GitHub is the concrete provider in this change; native GitLab/Jira adapters, additional authentication mechanisms and webhook ingress remain follow-ups.
- Support arbitrary all/any/excluded labels and unrestricted selection, explicit exclusive groups and editable assessment-to-implementation examples. Let any configured stage/outcome enqueue pinned source effects through ordinary trusted actions, preserving unrelated labels and reconciling uncertain or superseded writes without changing accepted Factory history.
- Add typed management REST, guided connection/access/input configuration, read-only preview and safe diagnostics. Existing manually installed credentials remain usable through an explicit adoption path. Manual intake and ordinary CI/CD remain independent of integrations.
- Keep integration UI focused on connections, access, automatic pipeline inputs and supported source effects. Optional internal Issue links do not replace Work/Run diagnostics; manual Issue creation must not acquire an integration prerequisite when the card capability is enabled.

## Capabilities

### New Capabilities

- `server/github-work-source`: GitHub Issue discovery, scoped credentials, bounded polling, immutable intake, reusable native observation provenance and non-authoritative reporting.

### Modified Capabilities

- `server/dark-factory`: Stable pipeline inputs referencing reusable authorized connections, distinct ticket/code resources, replay and stage-local source effects.
- `server/rest-control-plane`: Versioned connection and Project-access management, protected secret submission/rotation, safe capability/resource choices, previews and diagnostics.
- `ui/operator-console`: Dedicated Integrations administration and Project access, with reusable connection selection at pipeline inputs and typed stage-local effects.

## Impact

- Extend existing verified provider/process contracts, composition, application/store ports and additive PostgreSQL persistence for public connection/access metadata. Keep raw secrets in protected server-side credential storage, separate from serializable configuration, Work, audit and ordinary API responses.
- Extend management OpenAPI, generated clients, REST fixtures, console navigation and integration/Factory forms. Test authorization, secret redaction, atomic publication/recovery, rotation/revocation, provider-neutral reuse and existing admission/Delivery behavior through public-boundary TDD.
- Reuse the pinned GitHub transport and private credential installation mechanisms; do not introduce a general-purpose secret vault, provider SDK per stage, new Flow primitives, paid-model calls or changes to the user's running stand.
- Preserve completed tasks 1–4 and 5.1–5.3 and their retained qualification. Remaining tasks add the observation/correlation seam and compatible management/UI guidance. `add-factory-work-card` owns persistent Issues, import/refresh policy, field ownership/conflict resolution and manual execution requests; no second poller, admission identity authority or reporting outbox is introduced here, and existing integration execution remains usable without cards.
