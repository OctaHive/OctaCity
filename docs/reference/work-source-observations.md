# Optional Source observations

`WorkSourceObservations` is a private application read port for an explicitly
enabled consumer, such as the future internal task owner. Its runtime adapter is
`HostedWorkSourceObservations`. Constructing it starts no worker, subscription or
provider read. Existing intake/reporting do not call it or wait for a card. It has
no admission, Artifact publisher, Reporter or task-store dependency.

Observation and preview constructors take `WorkSourceReadProfile`: installed
binding/resource, verified executable/configuration, current read access, security
context, selector, network and finite polling bounds. Intake composes this same
profile as `FactorySourceProfile.read` alongside its separate `preparation`.
An independent task reader supplies no acceptance mapping, Artifact settings,
code source selection or Work classification.

## Authority and identity

The consumer authorizes its own operation before calling the port. Each request
selects a trusted installed Source binding and Project; the request context must
match the installed security partition. Managed profiles carry their resolved
`ManagedIntegrationAccess` for task reading. Current grants are checked before
dispatch and again before handing off an in-flight response, within the original
operation deadline. Legacy private installations retain their explicit installed
authority. Neither mode falls back to another credential. The verified Source
process enforces credential class, native resource, profile, network and endpoint
ceilings.

Observations retain Project/security scope, exact installation/binding, verified
plugin, provider configuration and selector digest. Native correlation reuses the
existing Project/security/installation/container/item hash. It excludes input
identity, selector/version, code Repository, display name, credentials and mutable
task fields. Distinct inputs can retain separate Work/Run records with the same
correlation. This is neither association-write authority nor an admission key.

The request needs no Factory Configuration, Flow or pipeline input id. A consumer
can select its own permitted installed selector, such as GitHub `state: all` with
empty label predicates, to observe closed tasks that no longer match execution.
This still applies that installed selector and resource access: the port cannot
synthesize a broader grant. Execution input creation enables no linked-task reads.

## Bounds, progress and freshness

Each call performs one existing Source v1 `scan`. Original scan identity,
inclusive window (or full reconciliation) and private continuation pass through
unchanged. A continuation retains its original pass context. The consumer can
retain these existing bounded progress facts; this port adds no second scheduler,
checkpoint journal or polling algorithm. Import/link-refresh policy and persistent
progress belong to `add-factory-work-card`. An empty/incomplete page is not proof
that a linked task was deleted.

Shared `SourcePollingPolicy` validates finite page/item/byte/retention limits.
Requests narrow installed limits. Verified protocol validation rejects malformed
or oversized pages and substituted identities atomically. Cancellation drains the
existing process. Throttling preserves bounded retry hints; outages do not become
empty success. Consumer publication and retry scheduling are separate operations.

Private observations contain bounded normalized title/body/field data and a
SHA-256 digest of the complete snapshot, excluding execution selection matches.
They have no automatic Debug/Serialize output and are not REST/diagnostic DTOs.
The port retains no cache. Consumers must protect retained content and honor its
original expiry.

`observed_at` is trusted time at read start; `expires_at` uses the installed finite
retention period from that time. Reusing an observation cannot renew it.
`freshness_at(now, previous)` reports:

- `Fresh`: a live first observation or changed complete content.
- `Unchanged`: a live reread with the same complete snapshot digest.
- `Stale`: expired, future-dated, or older than the consumer's previous read of
  the same native task.

Provider update ids remain opaque evidence. Equal read times or delayed replicas
provide no remote revision ordering or compare-and-swap guarantee. Field ownership,
revision ordering and conflict handling belong to the card owner. A previous
observation of another native task cannot establish unchanged content for this one.

An observation alone creates no Issue, association, Work or Run, changes no frozen
Artifacts and grants no Reporter authority. Manual Factory admission remains
independent. See [safe source diagnostics](factory-source-diagnostics.md) and the
[reused Source protocol](protocols/work-source-reporter-v1.md). Persistent Issue
CRUD, import/refresh and explicit card execution remain in `add-factory-work-card`.
