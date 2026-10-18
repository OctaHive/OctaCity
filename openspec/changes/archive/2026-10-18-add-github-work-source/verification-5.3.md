# Task 5.3 verification: recoverable integration administration

## Qualified boundary

Connection metadata, credential administration and lifecycle administration use
separate typed commands and authorization targets. Credential commands require a
verified authenticated management actor in addition to the credential-specific
authorization grant. Job secret grants and connection visibility confer neither
credential administration nor Project-use authority.

The application/store contracts remain provider-neutral. Tests use opaque Jira,
GitLab and custom provider identities; GitHub alone implements the concrete private
publisher. No provider DTO, native API call, Project grant, input activation or
Factory routing decision is introduced into these commands.

`IntegrationAdministrationStore` reserves a safe operation before private I/O.
The original security scope, replay key and safe intent identify the operation;
timestamps and secret bytes do not identify it. The journal retains the original
receipt, requested time, immutable metadata version, revision and private generation.
Completed replay precedes mutable resolution and performs no private write, even
after another rotation, disablement or revocation. A repeated secret submission
uses an existing preparation's original bytes; changing a prepared credential
requires a new reviewed operation/key, not changing the replay body.

`ProviderCredentialPublisher` prepares privately, activates with a monotonic fence
and erases preparations independently. GitHub uses descriptor-relative private
files, bounded borrowed/zeroing values, atomic replacement and a nonblocking native
directory lock around the generation check and write. The runtime handle remains
the original installed purpose/resource handle. Revocation publishes an unusable
private tombstone retaining its generation; older publishers cannot restore it.
A fresh explicitly authorized provisioning operation can restore that same scope
with a higher generation. The older installation writer cannot rotate over a
managed generation. Legacy operator-installed files remain readable/adoptable.

Preparation is invisible to runtime credential reads. On restart the same operation
can resume without another secret submission. Material activation precedes the
transaction committing configured state and safe completion evidence. The optional
`activate_connection` flag explicitly requests connection activation in that same
operation; every other selected purpose must already be configured. It defaults
to false, and never grants a Project access or enables a Factory input. Preparation and failure cannot activate a previously inactive connection. A completion
transaction failure rolls back configured/active state, receipt, audit and outbox;
recovery reuses the already prepared/activated private generation.

There is also explicit lifecycle activation/disablement. Disablement abandons an
unfinished preparation and invalidates only its configured purpose. It cannot
make a cancelled preparation appear ready through an older flag. Metadata
replacement prevents completion against the previous metadata version. Ordinary
completed historical replay does not enable a subsequently disabled connection.

Cleanup accepts at most 100 operations per pass. HTTP derives the expiry cutoff
from its server clock and a 15-minute preparation retention window; clients cannot
supply a future cutoff. The store first abandons expired operations, then returns
a retryable bounded batch. Private cleanup erases only preparation contents and
retains a safe cancellation marker for delayed writers. It never deletes active
runtime material, redirects a handle or changes independent configured purposes.
Failed cleanup/acknowledgement remains eligible for another pass. Completed safe
receipts and private fencing/cancellation metadata remain retained for replay.

Private temporary filenames include the destination handle's hash and, for managed
publication, the original operation identity. Under the existing directory lock,
recovery removes exact owned temporary inodes even when a stopped writer left
incomplete JSON. Cleanup also covers an interrupted legacy write to that same
installed handle. It does not enumerate other operations or delete active files.
Expiry transitions commit attributed audit/outbox evidence with the state change;
the trusted cleanup request time is distinct from its retention cutoff. Retrying
an already terminal preparation does not duplicate that evidence.

Migration 0065 adds the safe administrative state, operation journal and unique
logical-handle ownership. Administrative consumers have a foreign key to their
exact immutable metadata version, so referenced history cannot be collected after
replacement. No destructive retirement API is exposed; disablement retains
metadata and runtime material. A connection cannot claim another connection's
logical credential handle. No migration adopts credentials or starts workers.

## Protected REST composition

Seven endpoints are registered through the existing typed management/OpenAPI
registry: metadata publication, current/versioned metadata inspection, credential
administration, lifecycle mutation/inspection and preparation cleanup. Mutations
retain the existing idempotency/current-version conventions, including
`If-None-Match: *` for initial publication and a matching strong `If-Match`
otherwise. All protected responses use `Cache-Control: no-store`.

`ManagementApplication::with_integrations` binds separately authorized services.
A trusted identity/transport verifier must install `ProtectedIntegrationRequest`
only after authenticating over TLS or an authenticated local proxy channel.
Client `Authorization` and forwarded-protocol headers alone are insufficient.
Without that proof, even the ordinary trusted-network router rejects integration
administration before body parsing/application dispatch. With valid proof but no
installed service it reports unavailability without invoking ordinary handlers.
This change does not install a live identity provider or silently enable an
operator's provider credentials.

The protected submission alone accepts the bounded transient `value`. The input
has redacted Debug and one-way deserialization, with no Serialize/Clone/Display.
The API sanitizes parsing/provider errors, limits the request body and reuses
request tracing that records request identity/method/route rather than bodies.
Public connection/history, operation receipts, audit, outbox and idempotency
fingerprints contain no submitted secret or secret-derived digest. OpenAPI marks
the submission write-only and documents the separate protected ingress. Existing
ordinary/manual management authorization remains unchanged.

The connection name schema uses the application-exported domain bound and
documents its UTF-8 byte semantics. Credential status includes retained references
across metadata versions, so its response schema has no per-version item ceiling.
Review corrections and their Red/Green evidence are recorded in
[verification-5.3-review-followup.md](verification-5.3-review-followup.md).

Current Project grants and per-operation Source/Reporter runtime authorization
are task 5.4. Readiness checks are task 5.5. A configured flag here proves protected
material publication, not remote permissions. These tests qualify administrative
state and the GitHub private publication boundary, not live GitHub/GitLab/Jira
access. Full input/access/check client/UI authoring remains in later section-5
work. No running local stand was changed.

## TDD evidence

Baseline: the existing 22 GitHub credential contract tests passed before changes.
Each row below records an observed Red before its implementation and a subsequent
Green at the public application/store/REST/private-installation boundary.

| Behavior | Observed Red | Green outcome |
| --- | --- | --- |
| Private preparation/recovery/fencing | Recoverable publisher API absent | Preparation stays private; restart resumes without a token; stale activation fails after rotation/revocation |
| Orphan cleanup and legacy bypass | Legacy rotation overwrote managed material | Abandoned secrets are erased and fenced; old writer cannot bypass generations |
| Safe durable operations | Administrative store API absent | Stable receipts, revision conflicts, historical replay and disabled state retained |
| Bounded cleanup | Cleanup returned unavailable | Expiry, batch limits, repeated cleanup, acknowledgement and late completion fencing pass |
| Authorized orchestration | Credential command/handler API absent | Denial/authentication precede effects; private interruption recovers once; old replay makes no private call |
| Lifecycle and safe evidence | Lifecycle API absent; evidence counts were only metadata `(1,1,1)` | Explicit lifecycle commands and exactly-once begin/completion audit/outbox pass |
| PostgreSQL | Persistent administrative store API absent | Shared memory/PostgreSQL contracts pass with transactional safe evidence |
| Protected REST and preconditions | Endpoint absent; missing precondition returned 200 | Proof required, write-only values, bounded/sanitized rejection, no-store and 428/412 behavior pass |
| Typed registry/OpenAPI | Credential operation/schema absent | Every route has its typed authorization declaration; protected security and write-only schema pass |
| Cancelled/current metadata fences | Cancelled configured flag enabled a connection; memory completion accepted old metadata | Cancelled preparation cannot enable; replacement fences completion in both adapters |
| Referenced version retention | Deleting a referenced historical version succeeded | PostgreSQL foreign key rejects deletion with 23503 |
| Explicit restoration | New provisioning after revocation returned conflict | Higher-generation provisioning restores only the same reviewed installed scope |
| Corrupted preparation/recovery | Invalid prepared token was activated; equal-operation recovery accepted invalid current material | Preparation and recovered current material are revalidated before activation/receipt |
| Independent purposes | Cleanup cleared a separately configured purpose | Only the abandoned publication's purpose is invalidated |
| Unconfigured verified ingress | Valid proof received a policy denial from a transport fallback | Missing service returns 503 without an application/policy implementation in REST |
| Read-only inspection | A read-only PostgreSQL connection failed because inspection took a write lock | One consistent SELECT reads state without a writer transaction or row lock |
| Opaque handle isolation | A preparation configured another permitted installed handle with a colliding name | Domain-separated hashed preparation names cannot overlap active handles |
| Retained scope integrity | A substituted retained reference reached private I/O | Mismatched intent/reference/operation/generation fails before private access |
| Atomic requested activation | Credential intent lacked reviewed activation | Private recovery and requested connection activation complete under the original operation |

Additional characterization forces completion-outbox failure in a disposable
PostgreSQL database and verifies rollback followed by one recovery commit. It also
exercises concurrent identical reservation and reloads the persistent adapter.
No timeout increases, test serialization or production worker changes were used.

## Verification commands

```sh
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-secrets -p octacity-server-store -p octacity-github -p octacity-server-vcs -p octacity-delivery-protocol -p octacity-server-api-rest
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-application --lib integration_
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-application --lib management_security
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-application --lib factory_admission
OCTACITY_POSTGRES_URL=<disposable-service> CARGO_INCREMENTAL=0 cargo test --locked --all-features -p octacity-server-store-postgres --test integration_administration --test integration_connections --test migrations --test factory_discovery -- --ignored
CARGO_INCREMENTAL=0 cargo clippy --locked -p octacity-server-secrets -p octacity-server-store -p octacity-server-application -p octacity-github -p octacity-server-store-postgres -p octacity-server-api-rest --all-targets --all-features -- -D warnings
python3 -m unittest discover -s tools/tests
python3 tools/check_architecture.py
cargo fmt --all -- --check
openspec validate add-github-work-source --strict
git diff --check
# From ui/, using the locked existing dependency installation:
CARGO_INCREMENTAL=0 pnpm api:generate
pnpm typecheck
```

Final results: 430 relevant Rust tests passed, including 38 PostgreSQL tests
(5 administrative recovery/retention/read-only tests, 4 metadata tests, 6 migrations
and 23 Source/discovery contracts), 27 GitHub credential tests and 84 targeted
application tests (3 integration, 21 management security, 60 admission). The
six-package provider/core/REST regression passed 308 tests, including compile-fail
doc tests. No Rust failures or ignored selected PostgreSQL tests remained.

The tools suite ran 432 tests successfully with 8 platform/environment skips.
Strict Clippy passed for all six changed Rust packages, with focused rechecks after
the final GitHub/PostgreSQL changes. OpenAPI generation, TypeScript type checking,
architecture (63 packages/253 edges), formatting, strict change validation and diff
whitespace checks passed. Generated UI types remain in the repository's existing
ignored `.generated` directory; no dependency installation or package version
change was needed.

PostgreSQL used a task-owned disposable PostgreSQL 18.6 container with generated
isolated databases and default parallel test execution. The container and its
anonymous volume were removed after verification. No commits or pushes were made.

Keep the safe journal, immutable referenced versions and private fencing markers
when disabling or rolling back managed administration. Managed files require a
compatible provider decoder; stripping a generation or restoring an older token
would discard the revocation/replay guarantee and is not a supported rollback.
