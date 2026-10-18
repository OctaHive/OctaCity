# Task 3.1 verification

## Scope

Implemented independently advertised and versioned Work Source/Reporter process
contracts in `octacity-work-source-protocol`, with role-scoped execution in
`octacity-server-work-source` through the existing verified adapter host. The
single GitHub executable recognizes their disjoint families alongside unchanged
Delivery framing. Source/Reporter protocol cancellation requires no Delivery
credential. Concrete Issue operations fail closed as `unsupported` until their
installed grants, provider reads and mappings are implemented in subsequent tasks.

The new wire records contain opaque ticket-container/item identities, exact
installed selector/effect references and normalized private data. No GitHub DTO,
Issue-state enum, provider SDK, Flow transition, Work admission, new poller,
outbox authority or manual-input requirement was added. Host contracts execute
for `github`, `gitlab`, `jira` and a custom identity without provider branching.

Source/Reporter credential classes, profile digests and version ranges are
independent. A valid future Source range cannot disable a compatible Reporter.
Known role schema, complete paired declarations, observation before projection
and capability-specific authority remain mandatory. Stored grant/resource/network
verification belongs to the concrete provider; a wire class is not a grant.

## TDD evidence

Public test boundaries: checked request/response decoders, verified registry
resolution/process execution, the actual GitHub process, and existing application
manual/source admission contracts. Tests use independently specified JSON facts;
they do not call private implementation methods.

| Slice | Observed Red | Observed Green |
| --- | --- | --- |
| Source cancellation | Checked decoder API absent | Exact Source version/request/scan acknowledgement round-trips; foreign correlation fails |
| Source scan request | Valid normalized scan returned `Invalid` | Separate read grant and exact installed selection accepted; wrong classes, invalid windows, hosts and caller ceilings rejected |
| Private page response | Normalized page returned `Invalid` | Exact scope accepted; changed scan/container/profile/selection, duplicate native items, oversized private fields and narrowed byte ceilings rejected; private Debug data redacted |
| Reporter | Reporter family returned `Invalid` | Independent observe/project/cancel accepted with write authority; substituted item/mapping/value and partial effects cannot claim application |
| Advertisement/version negotiation | Manifest/range API absent | Optional paired roles validated; incompatible Source and compatible Reporter negotiate independently; Delivery/read-class substitution fails |
| Classified failures | Safe transient outcome was rejected | Failure stays distinct from empty discovery; raw diagnostic fields and excessive retry hints rejected |
| Complete page byte ceiling | Padded received JSON incorrectly passed a 2000-byte ceiling | 2001-byte received frame rejected even when canonical re-encoding would fit |
| Verified Work host | Registry API absent | Existing executable verification and bounded process host execute a correlated page for four provider identities |
| Reporter host | Wrong-profile Reporter returned generic `Unsupported` | Installed Reporter validates its own profile before spawn and executes while Source's version is incompatible |
| Same GitHub process | New Source cancellation exited unsuccessfully | Both Work families acknowledge cancellation independently, while the original Delivery process test remains Green |

The lifecycle regressions also verify original-role/operation cancellation,
credential-free cancellation frames, pre-cancellation without spawn, absolute
timeouts with forced cleanup, wrong classes and executable drift before handle
exposure, and malformed/uncorrelated/oversized/extra responses. GitHub process
negatives verify explicit `unsupported` without credentials and no Delivery-class
fallback or diagnostic output.

The existing Delivery contracts passed before extracting exact host-set validation
into the shared adapter primitives and after extraction. Protocol owners retain
their own schemas/versions; only the existing canonical host validation mechanics
are shared. Boxing large wire enum variants preserves their serialized shape.

Slice commands (the first scaffold invocations omitted `--locked` and used
`--offline` only to register new internal packages in the workspace lockfile):

```sh
cargo test --locked --jobs 2 -p octacity-work-source-protocol --test contract source_cancellation -- --nocapture
cargo test --locked --jobs 2 -p octacity-work-source-protocol --test contract scan_requires -- --nocapture
cargo test --locked --jobs 2 -p octacity-work-source-protocol --test contract pages_are -- --nocapture
cargo test --locked --jobs 2 -p octacity-work-source-protocol --test contract reporter_accepts -- --nocapture
cargo test --locked --jobs 2 -p octacity-work-source-protocol --test contract advertised_roles -- --nocapture
cargo test --locked --jobs 2 -p octacity-work-source-protocol --test contract failures_are -- --nocapture
cargo test --locked --jobs 2 -p octacity-work-source-protocol --test contract -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-work-source --test host each_provider -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-work-source --test host incompatible_source -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-work-source --test host -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test process the_same_provider -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test process -- --nocapture
```

## Regressions and checks

All listed checks passed on 2026-10-16:

```sh
cargo test --locked --jobs 2 -p octacity-work-source-protocol -p octacity-server-work-source \
  -p octacity-delivery-protocol -p octacity-server-delivery -p octacity-server-adapter-host \
  -p octacity-adapter-protocol -p octacity-github --all-features
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --test manual_trigger --test factory_artifact_reader -- --test-threads=2
cargo clippy --locked --jobs 2 -p octacity-work-source-protocol -p octacity-server-work-source \
  -p octacity-github -p octacity-delivery-protocol -p octacity-server-delivery \
  --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --jobs 2 -p octacity-work-source-protocol \
  -p octacity-server-work-source --all-features --no-deps
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
cargo audit
cargo deny --locked check licenses sources
git diff --check
```

Observed: 53 protocol/host/provider tests, 40 application admission tests,
34 manual-trigger tests and 5 ordinary Artifact-reader tests: **132 distinct
tests**. The existing manual and staged-input/replay behavior remains available
without a Source/Reporter installation. Architecture: 63 packages, 250 edges.
Clippy, public rustdoc and format checks pass. Cargo.lock adds only two internal
packages and their workspace edges; no external dependency/version was added.
Audit completed successfully with the two existing allowed unmaintained warnings
for `bincode` and `unsafe-libyaml`; licenses and sources checks passed.

Linux verification ran in the owned disposable container labelled
`octacity.test=work-contracts-31`, with Rust 1.99.0, cached registry sources,
an isolated target directory and umask 022. The same command below passed all
15 new protocol/host tests. Twenty further concurrent host-suite runs completed
120 test executions without failure. Tests use hard links to an immutable fixture
executable on the same filesystem, avoiding freshly written executable races.
After verification, the container identity/ownership label was checked and the
owned container and its isolated build directory were removed.

```sh
cargo test --locked --jobs 2 -p octacity-work-source-protocol -p octacity-server-work-source --all-features
```

No live provider credential, user repository, local stand, paid model, database
migration or runtime worker was used. Provider configuration/read/write and worker
composition tasks remain unchecked. Only task 3.1 is completed here; progress is
10/29. No commit was requested for this implementation.
