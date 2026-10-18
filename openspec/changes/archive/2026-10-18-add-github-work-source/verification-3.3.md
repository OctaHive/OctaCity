# Task 3.3 verification

## Scope

The existing GitHub provider now implements one read-only Issue page through
`GithubWorkProvider::execute` and its actual Source process family. It uses the
same pinned Octocrab/reqwest transport as Delivery, with private installed API
trust and independently scoped Issue-read authority. Source configuration adds
an optional private `selection_directory`; opaque selector identities resolve
only to hashed JSON filenames, with exact byte digests and the existing checked
Issue selector schema. Absent selector installation stays `unsupported`.

Each page verifies the installed native repository id, then lists only that
repository's Issues. Item `id` remains independent of display `number`, title,
labels and update time. Local all/any/none/state matching enforces the installed
selector; presence of `pull_request` excludes that record. Private normalized
snapshots retain title, optional body, labels, native state and update identity.
Comments, attachments and linked resources are never fetched.

Pagination emits opaque page cursors bound to the scan, profile, resource,
selector, fixed window and page size. Untrusted Link headers cannot change the
HTTPS origin/port, repository path or query, skip/loop pages, add credentials or
fragments, or introduce another fetch URL. A filtered empty page retains its
continuation. Raw JSON bodies have declared/streamed 4 MiB bounds before decode;
the Work contract also enforces item, snapshot and complete response ceilings.
Malformed/oversized observations fail without partial success or truncation.

Inclusive millisecond windows are enforced locally, with one second of overlap
in GitHub's exclusive `since` query. Rate limits/server outages are transient;
denied/missing resources require repair. Responses contain closed failures rather
than raw provider diagnostics. The SDK neither retries nor follows redirects.

Process cancellation handles only the original protocol family and operation,
stopping an in-flight HTTPS read under its original correlation. Source and
Delivery share one bounded stdin/cancellation loop while retaining their own
decoders, scope checks and failure types. Their existing regressions pass.

No GitHub DTO, state enum, cursor interpretation or SDK type was added to Factory
or the provider-neutral Work contracts. GitLab, Jira and other providers retain
their own selector/installation/translation formats. Manual admission remains
available without an integration. Durable polling/progress and worker composition
belong to tasks 3.4–3.5; Reporter effects belong to section 4. No worker, provider
write or live-account qualification is introduced here.

## TDD evidence

Public boundaries: installed provider loading/execution, checked Work response
decoding and the actual executable's JSONL process contract. Controlled HTTPS
peers use generated private CAs with hostname verification, real sockets, bounded
scripted responses and private fixture installations. They inspect HTTP methods,
headers, paths and accepted requests without mutating the private transport or
mocking provider internals. No live provider credentials are used.

| Slice | Observed Red | Observed Green |
| --- | --- | --- |
| HTTPS Source page | Public execution API missing (E0599) | Exact repository/Issue data returned over trusted HTTPS with separate read authority |
| Installed API version | Wire header was the SDK's compiled-in `2022-11-28`, then duplicated by an additive override | Exactly one configured API version is inserted after SDK request construction; shared with Delivery |
| Local selector/PR exclusion | Returned PR-shaped records caused failure instead of an exactly selected page | All/any/none/state predicates applied locally; records carrying `pull_request`, including null, excluded |
| Pagination | Filtered page lost its next cursor | Empty filtered page continues to the next exact repository/query page |
| Forbidden pagination destinations | Host-substituted Link was ignored as successful empty discovery | Foreign host/scheme/repository/query, loop, duplicate query, credential and fragment rejected |
| Invalid identities/raw item counts | Issue id zero was returned as a candidate | Zero ids/numbers, oversized titles, malformed root, duplicate candidates and excessive raw item counts fail without partial candidates |
| Fixed update window | Four candidates returned instead of the two inclusive boundary candidates | Local lower/upper millisecond boundaries enforced; equal timestamps kept; edited content preserves native id |
| Declared wire byte ceiling | Oversized Content-Length became a transient stream failure | Declared oversize rejected as protocol fault; chunked oversize and invalid JSON rejected too |
| Provider failure classification | Rate limit returned `ProtocolFault` | 429, rate-limit 403 and 5xx remain transient; denied/missing scopes and conflicts remain distinct |
| Actual process listing | Process still returned placeholder `Unsupported` | Binary returns a checked normalized Issue page |
| Actual process cancellation | Stalled HTTPS read exceeded the 2-second cancellation deadline | Matching Source cancellation stops it; Reporter substitution is a protocol fault |
| Native repository metadata | Native repository id zero qualified an empty page | Zero, missing or nonnumeric repository ids fail before Issue listing |

Focused commands used to observe Red; Green was verified by focused or complete
suite runs after each slice:

```sh
cargo test --locked --jobs 2 -p octacity-github --test issue_listing installed_read_scope -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test issue_listing every_returned -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test issue_listing pagination_keeps -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test issue_listing pagination_destinations -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test issue_listing malformed_identity -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test issue_listing fixed_update -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test issue_listing wire_byte -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test issue_listing provider_outages -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test issue_listing the_actual_source -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test issue_listing the_source_process_honors -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test issue_listing malformed_repository -- --nocapture
```

The initial registration of existing workspace `http`/`chrono` dependencies used
`--offline` without `--locked`, then all checks used the updated lockfile. No
external package or version was added. `http` supports safe SDK request header
replacement; `chrono` parses/formats provider timestamps already used by the SDK.

Additional passing public regressions cover untrusted CA, hostname mismatch,
same-origin/foreign redirects, private/digest-pinned contradictory selector files,
selector symlinks, recreated native repositories, forged/URL cursors, a cursor
reused under a changed query, pre-cancellation and direct in-flight cancellation.
Rejected provider bodies never appear in Debug diagnostics. Existing Delivery
behavior passed before extracting its common client builder and before extracting
the process wait loop, then passed again after each change. Documentation and
formatting-only edits have no executable behavior to drive with TDD.

## Regressions and checks

Passed on macOS on 2026-10-16:

```sh
cargo test --locked --jobs 2 -p octacity-github --all-features
cargo test --locked --jobs 2 -p octacity-work-source-protocol -p octacity-server-work-source \
  -p octacity-delivery-protocol -p octacity-server-delivery -p octacity-server-adapter-host \
  -p octacity-adapter-protocol -p octacity-github --all-features
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --test manual_trigger --test factory_artifact_reader -- --test-threads=2
cargo clippy --locked --jobs 2 -p octacity-work-source-protocol -p octacity-server-work-source \
  -p octacity-github -p octacity-delivery-protocol -p octacity-server-delivery \
  --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --jobs 2 -p octacity-github --all-features --no-deps
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
cargo audit
cargo deny --locked check licenses sources
git diff --check
```

Observed: 78 protocol/host/provider tests, 40 application admission tests,
34 manual-trigger tests and 5 Artifact-reader tests: **157 distinct tests**.
Architecture: 63 packages, 250 dependency edges. Audit passed with the two
existing allowed unmaintained warnings (`bincode`, `unsafe-libyaml`); licenses
and sources passed. Application/core/REST/UI implementation and manual paths
were unchanged in task 3.3.

Linux passed all 28 Issue-listing/private-installation/process tests in an owned
disposable container labelled `octacity.test=issue-listing-33`, using installed
Rust 1.99.0, umask 022, cached registry sources, read-only repository/dependency
mounts and an isolated writable target directory. Networking was disabled apart
from container loopback. The final native-id correction was verified by rerunning
the complete Linux selection and affected macOS suites:

```sh
cargo test --offline --locked --jobs 2 --target-dir /tmp/issue-listing-target \
  -p octacity-github --test issue_listing --test work_credentials --test process
```

The successful exit, exact container identity and ownership label were checked
before removing the container and its isolated build directory. No user's stand,
live provider, paid model or external repository was used. Only task 3.3 is
completed here; progress is 12/29. No commit was requested or created.

## Provider reference evidence

Primary documentation checked on 2026-10-16:

- [Repository Issues API](https://docs.github.com/en/rest/issues/issues#list-repository-issues): read permission, PR discriminator, state/labels, sort/direction, exclusive `since`, page sizing and native item fields.
- [Pagination](https://docs.github.com/en/rest/using-the-rest-api/using-pagination-in-the-rest-api): Link continuation semantics.

This evidence defines the controlled provider fixtures. It does not substitute
for an opt-in qualification against an explicitly selected disposable GitHub
repository, which remains task 6.2.
