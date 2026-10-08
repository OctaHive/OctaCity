# Signed JobSpec execution contract v3

Status: strict schema, compatibility negotiation, stable server-side template
derivation/signing, lease-scoped download authorization, Agent-side
protected-input staging, the pre-source/pre-spawn permission gate, and the
qualified containerd and Microsandbox projections are implemented. The shared
protected tool-action gate and redacted lease-bound exchange are implemented,
but the production Agent-local helper/IPC path and exact Codex action adapter
are not release-qualified. Host and unqualified isolation providers remain
ineligible for v3.

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
must be explicit and unique. Backends expose only the fixed portable roots
`/octacity/protected`, `/workspace/source`, `/workspace/scratch`, and
`/workspace/output`; the first is read-only and the other three are distinct
writable scopes below one private Job. v3 rejects direct Host execution.

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

## Placement and capacity diagnostics

Registration inventory exposes `factory_executions` separately from ordinary
`executions`. Each entry binds one validated execution route to its canonical,
ordered semantic enforcement ceiling. The nested provider identity is
diagnostic evidence only: placement matches the provider-neutral mode,
platforms, execution guarantees, and every capability in the signed
`required_enforcement` list. An Agent may advertise these entries only while
also advertising execution-contract v3, and only for a non-Host route already
present in its ordinary execution inventory.

The management Agent projection exposes the negotiated execution-contract
range, ordinary routes, and Factory-qualified route ceilings. Operators can
therefore diagnose a missing semantic control without teaching Factory policy
about containerd, Microsandbox, or another provider name. A route missing even
one required control is ineligible for that Job. Candidate selection continues
past incompatible ready Jobs, so unavailable Factory capacity does not block
unrelated compatible CI/CD work.

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
to Host execution.

After admission, the Agent passes a provider-neutral Factory layout to the
selected backend. Containerd binds only the three writable roots, binds the
protected root read-only/no-exec, and applies the effective process count to
the OCI cgroup. Microsandbox seals the `/workspace` parent, binds the three
writable roots separately, partitions their growth quotas so their sum cannot
exceed the Job disk ceiling, binds protected inputs read-only/no-exec, applies
`RLIMIT_NPROC`, and translates the exact restricted-host policy into its
default-deny network policy. Both retain the existing CPU, memory, elapsed-time,
identity, immutable image, read-only release/tool, and output supervision
boundaries. Concrete provider names are used only for diagnostics and
composition; signed policy and admission use semantic capabilities.

Apple VF is not Factory-qualified and does not receive v3 Jobs because its
current adapter cannot enforce an exact restricted-host allowlist. Host and
legacy Native routes likewise never advertise the v3 enforcement vocabulary.

## Protected tool-action decisions

This section defines the target contract and the implemented shared gate. It
does not claim that `tool_risk` bounded control is currently deployable. The
server route is unavailable unless composition explicitly installs a concrete
authorizer, and the Agent does not yet project an authorizer helper into a
running Codex task.

A coding harness proposal is untrusted input. Before any optional model-backed
assessment, the trusted Agent-side adapter must normalize it into one bounded canonical action covering
the exact tool and executable identities, concrete arguments, portable paths
and access modes, network destinations, logical secret and workload-identity
profiles, descendants, resources, and output authority. The action must fit
both the signed Factory Permission Set and the independent local/backend
ceiling. An out-of-envelope or malformed action is denied without invoking a
Decision Signal provider.

Deterministic policy then hard-allows, hard-denies, or marks an in-envelope
action for bounded `tool_risk` assessment. The provider receives only a
domain-separated proposal digest and bounded counts; it never receives raw
commands, paths, profile names, credentials, or secret material. Consuming code
can preserve allow for that same digest or narrow it to deny/escalate. Missing,
invalid, mismatched, or unavailable assessment applies the configured
deny/escalate fallback and cannot increase authority. Durable decisions contain
only the proposal digest, disposition, decision source, and optional immutable
receipt digest.

Immediately before backend use, the Agent canonicalizes the proposed action
again, compares its domain-separated digest with the decision, requires an
`allow` disposition, and reapplies both signed and local/backend permission
ceilings. Only the exact canonical value returned by this final gate may be
executed; a separately retained or modified proposal is not an executable
authorization. Stable errors and `Debug` output redact raw action material.

The signed managed execution declares `tool_control` whenever its permission
set contains protected tools. The declaration names the exact signed plugin,
the mode (`deterministic` or `tool_risk`), and the capability
`codex.blocking-pre-tool-authorization.v1`. Agents missing that exact verified
plugin capability are rejected at placement and again at preflight. Managed
execution without protected tools omits `tool_control`, so ordinary CI/CD and
Factory tasks without per-action control retain their existing behavior.

Octa 0.5.1 supplies the pinned synchronous Codex `PreToolUse` integration and
requires a protected native helper. OctaCity currently has the bounded parser,
deny/allow output encoder, redacted broker request, local permission checks,
fenced request/receipt response binding, and final unchanged-action gate as
tested components. It deliberately does not manufacture exact path, host,
secret, descendant, resource, or output requests by copying the complete Job
ceiling: the generic Codex hook document does not prove those effects. Until a
trusted exact-action adapter and Agent-local IPC helper are composed and
release-tested, no Agent advertises or enables `tool_risk` bounded control for
Codex. Ordinary execution and backend enforcement remain unchanged.

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
