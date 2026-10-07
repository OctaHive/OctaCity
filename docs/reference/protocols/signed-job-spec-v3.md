# Signed JobSpec execution contract v3

Status: strict schema, compatibility negotiation, stable server-side template
derivation/signing, lease-scoped download authorization, Agent-side
protected-input staging, and the generic pre-source/pre-spawn permission gate
are implemented. v3 execution remains disabled until the concrete isolation
backends project and prove the complete permission set.

Revision 3 adds protected managed execution without changing the bytes or
meaning of revision 1 or 2. The signed envelope, signature verification,
source, Octa, provider-neutral runtime, cache, output, lease binding, and
validity rules remain the existing contracts. The authenticated top-level
`protocol_version` selects the strict decoder only after signature
verification.

## Compatibility and rollout

The server supports execution contracts `1..=3`. Released Agents that have not
implemented protected input staging and enforcement continue to advertise
`1..=2`, negotiate v2, and remain eligible for ordinary v1/v2 Jobs. They are
ineligible for v3 Jobs. An Agent must not advertise v3 until it can enforce the
complete semantic vocabulary and protect the managed execution layout.

The Agent also compares every verified payload revision with the revision
selected for its registration. A server cannot bypass negotiation by placing a
validly signed newer payload in an older registration. Such a payload is
rejected before workspace creation, source materialization, secret delivery,
or process spawn.

## Strict v3 shape

`JobSpecV3` retains the common Job and toolchain identity fields and uses these
v3-only sections:

- `execution` selects a protected input containing the server-generated
  Octafile and an ordered non-empty list of exact Octa task names;
- `factory` optionally carries only bounded logical Factory identities,
  immutable configuration version, stage kind, and SHA-256 digests;
- `protected_inputs` contains bounded logical Artifact identities, exact sizes,
  lowercase SHA-256 digests, media types, and canonical destinations below
  `/octacity/protected`;
- `permissions` carries the complete deny-by-default Factory Permission Set;
- `required_enforcement` carries the complete ordered semantic enforcement
  vocabulary.

The canonical language-neutral example is
[`shared/protocol-fixtures/job-spec/job-spec-v3.json`](../../../shared/protocol-fixtures/job-spec/job-spec-v3.json).
Equivalent v1 and v2 fixtures freeze the exact bytes emitted for the released
legacy revisions.

All v3 objects reject unknown fields. Canonical collections reject duplicate
or out-of-order entries. Identities, paths, media types, command arguments,
entry counts, and aggregate protected-input bytes are bounded before they can
influence allocation or execution.

## Protected inputs and managed Octa execution

The signed manifest contains no object-store URL, authorization header,
credential, lease fence, or transfer bearer. Lease-scoped transfer grants are
separate short-lived transport values. They are minted only after the Lease
commits, never outlive that Lease, and repeat the signed metadata so the Agent
can require an exact match before touching its filesystem. Grant replay never
persists a capability URL in the stable Job template or signed payload.
Transient capability-mint failures keep the committed claim replayable under
the same Lease identity. A permanent manifest, publication, or integrity
failure atomically fences that Lease and records a server-owned
infrastructure failure; it is never returned as a retryable assignment.

The Agent accepts capabilities only from configured HTTPS origins (plus
explicit loopback HTTP for a local stand), disables redirects, bounds each
download, and verifies exact length, digest, and media type. It resolves every
portable destination below the reserved root without following symlinks or
Windows reparse points, publishes only a fully validated temporary file by an
atomic rename, and seals the complete subtree read-only. Immediately before a
runner may receive the root, the Agent walks it again, rejects added or
replaced entries, reopens each file without following the final link, and
revalidates size and digest. Partial state is removed on every failure, and
its stable errors contain no source bytes, capability URLs, credentials, or
host paths.

The managed Octafile is selected by logical protected-input identity. It
cannot be replaced by a workspace-relative repository path. Every named task
must be explicit and unique. Source, protected inputs, scratch space, and
outputs are separate portable roots; v3 rejects direct Host execution.

## Factory Permission Set

The permission set contains canonical ordered allowlists and ceilings for:

- exact plugin, executable, and tool identities;
- exact executables with fixed-length argument patterns whose entries are
  either exact values or wildcards carrying an explicit positive UTF-8 byte
  ceiling; the aggregate pattern is bounded independently as well;
- descendant and total process counts;
- non-overlapping portable filesystem roots and mount modes;
- exact DNS names or canonical IP addresses;
- logical secret and workload-identity profiles;
- CPU, memory, writable disk, and elapsed time;
- output kinds and Artifact/report count and byte limits.

Command executables must occur in the executable allowlist. Octa plugin
digests must exactly match the plugin permissions. Permission resource,
network, identity, and output authority must be no broader than the common
signed runtime and output sections. Unrestricted network access is invalid for
v3.

The initial v3 revision requires every known semantic enforcement capability,
including protected-input integrity, read-only publication, workspace
separation, and managed Octa execution. A future vocabulary extension requires
a new execution-contract revision; unknown values never imply best-effort
fallback.

## Agent admission boundary

Immediately before source checkout, the Agent intersects the already verified
signed permission set with an independent operator-owned local grant ceiling.
It rejects any widening of plugin, executable, tool, command/argument,
descendant, mount, network, secret-profile, workload-identity, resource, or
output authority. Portable path containment is segment-aware; a lexical prefix
such as `/workspace-escape` is never a child of `/workspace`.

The Agent also requires the lease transfer metadata to exactly repeat the
signed protected-input manifest, requires the protected root to be read-only,
and matches runtime, identity, resource, and output projections against the
same permission set. The selected route must advertise every signed semantic
enforcement capability. Routes advertise none by default and can opt in only
after backend conformance is proven.

Runner, Octa plugin, source plugin, and explicitly selected external executable
files are reopened and rehashed against their verified inventory immediately
before use. Drift, missing local grants, capability gaps, and
repository-requested widening are secret-safe preflight failures: they create
no workspace, perform no source checkout, spawn no process, and never fall back
to Host execution. Until backend-specific projection is implemented, a request
that passes this generic gate is still rejected rather than partially run.

## Stable template derivation

The server persists a stable unsigned template before a Job becomes ready.
For Factory-owned Jobs the application repeats the four-way intersection of
Project, Factory Configuration, task, and local permission ceilings at this
boundary, converts only the narrowed result to the shared v3 representation,
and binds it to the exact source revision, Factory causality, protected-input
metadata, generated Octafile/task identities, and required enforcement
capabilities.

Attempt identity, Job identity, issue and expiry times are added only when the
ordinary Job state machine moves the Job to `Ready`. Signing validates the
complete resulting `JobSpecV3` first. A retry therefore derives the same
stable intent with fresh bounded lease-time identity, while Factory input can
never expand Project authority.

Stable template types cannot represent object-store URLs, bearer tokens,
credentials, lease identities, or fences. Those short-lived transfer values
remain outside both the persisted template and signed payload.

## Validation order

Consumers perform these steps in order:

1. bound and decode the envelope;
2. verify the Ed25519 signature over the exact payload bytes;
3. select the decoder from authenticated `protocol_version`;
4. enforce strict JSON decoding and all v3 semantic invariants;
5. compare the payload revision with the negotiated registration revision;
6. revalidate local grants, resolved paths, executable identities, profiles,
   and backend capabilities immediately before protected activity.

Steps 5 and 6 can only reject or narrow signed authority. No provider name,
repository file, model output, transfer grant, or local fallback can widen the
signed v3 intent.
