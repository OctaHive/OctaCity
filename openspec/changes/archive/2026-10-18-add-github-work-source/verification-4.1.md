# Versioned GitHub status mappings — task 4.1

Completed on 2026-10-17. OpenSpec progress: 15/29 tasks. No live provider,
operator credentials, paid model calls, deployment changes or commits.

## Boundaries and behavior

Public boundaries: `status_mapping::StatusMapping::from_config` / `effects`,
`GithubWorkProvider::load` / `execute`, checked Reporter response decoding,
and the actual executable's JSONL contract. HTTPS fixtures use private generated
CA certificates, hostname verification, real sockets and separately installed
write grants. Assertions check specified effects, exact scoped HTTP methods,
paths/bodies, closed failures and correlated receipts.

Mappings have an explicit configuration version and exact installed identity /
SHA-256 reference. They validate bounded arbitrary status values, disjoint managed
groups, explicit additions/removals and optional open/closed state. Group membership
does not implicitly remove other group labels. State-only effects need no dummy
labels. Duplicate status/group definitions, conflicting effects, unsupported
fields/versions/states and unmanaged labels fail safely. Literal dot-segment
labels are rejected before URL construction to prevent a targeted removal from
normalizing into a collection delete.

Reporter checks installed profile, native resource, network and independent
Issue-write grant before reading the pinned mapping or calling GitHub. It checks
every selected mapped label exists before any mutation, validates the original
stable Issue id and number, excludes pull requests, observes current labels and
applies only the missing explicit effects. It never creates repository labels,
replaces the complete Issue label collection or modifies unrelated fields.
Applied receipts require a subsequent matching observation.

Source and Reporter share bounded Issue API transport with configured TLS/API
version, no redirects or SDK retries, safe failures and retained rate-limit
hints. Source regressions passed before and after extracting that transport.
Current Issue label reads accept at most 100 labels and reject pagination rather
than treat an incomplete observation as complete or follow an untrusted URL.

Factory core, the Work protocol and `FactoryWorkStatusProjector` remain provider
neutral: opaque provider/container/item identities, arbitrary statuses and pinned
configuration references. GitLab/Jira/custom adapters retain their native mapping
semantics. Existing host fixtures exercise multiple provider identities. Manual
Factory admission still requires no Source/Reporter installation or credentials;
its 60-test public regression suite passed, including the retained manual-intake
case alongside source replacement and label churn.

## Observed TDD cycles

| Public behavior | Observed Red | Observed Green |
| --- | --- | --- |
| Arbitrary status mapping | Required public `status_mapping` API missing (E0432) | Exact configured effects resolve; unknown and differently cased values do not gain special meanings |
| Mapping validation | Empty managed group accepted | Conflicts, ownership overlap, unmanaged/duplicate labels, invalid states/fields and collection/name bounds rejected |
| Installed Reporter observation | Provider rejected the newly required mapping-directory configuration | Read-only HTTPS observation returns exact matching/nonmatching receipts under a pinned mapping |
| Native projection | Project returned `Unsupported` | Explicit label add/remove and optional open/closed state applied; unrelated labels retained; final observations required |
| Duplicate mapping definitions | Duplicate status/group JSON keys silently replaced earlier definitions | Checked decoder rejects both ambiguous maps |
| Dot-segment safety | Literal `.`/`..` labels accepted | Such labels rejected before any targeted removal can become a collection operation |
| State-only mapping | Empty managed-label map rejected | Explicit closed-state effect accepted without synthetic labels |

Focused commands used for Red and then Green:

```sh
cargo test -p octacity-github --test status_mappings --locked
cargo test -p octacity-github --test issue_reporting --locked
cargo test -p octacity-github --test issue_reporting projection_changes --locked
cargo test -p octacity-github --test status_mappings duplicate_status --locked
cargo test -p octacity-github --test status_mappings dot_segment --locked
cargo test -p octacity-github --test status_mappings state_only --locked
```

Final focused suites: 5 mapping tests and 6 HTTPS/process tests passed.
Additional characterization covers missing labels before either addition/removal,
unknown status, pinned mapping drift, malformed mapping, substituted read grant,
pre-cancellation, foreign repository/item/number, pull requests, incomplete label
observations and real-process confirmation of already matching effects without
writes. No fixture/setup failure was treated as behavioral Red.

## Regressions and checks

Before implementation, these commands passed:

```sh
cargo test -p octacity-github --locked
cargo test -p octacity-server-application factory_admission_tests --locked
cargo test -p octacity-server-application factory_flow_action_tests --locked
```

After the shared Issue transport extraction, all 19 Source listing tests and
the Reporter observation test passed:

```sh
cargo test -p octacity-github --test issue_listing --test issue_reporting --locked
```

Final checks passed:

```sh
cargo test --locked -p octacity-github -p octacity-work-source-protocol -p octacity-server-work-source -p octacity-delivery-protocol
cargo test -p octacity-server-application --lib factory_flow_action_tests --locked
cargo clippy -p octacity-github --all-targets --all-features --locked -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

The combined provider/protocol/host regression passed 88 tests: 66 GitHub,
11 Work protocol, 7 verified host and 4 Delivery protocol tests. Application
contracts passed 60 admission and 15 Flow action tests. Architecture check:
63 workspace packages, 252 dependency edges. No dependencies or lockfiles changed.
Documentation/formatting edits have no executable behavior requiring a TDD cycle.

## Remaining scope

This completes adapter-side mapping validation and effects only. Connecting
accepted `set_status` actions through the existing outbox remains task 4.2;
durable uncertain/partial-write recovery and cross-input managed-field ordering
remain tasks 4.3–4.4. Multi-request GitHub updates are not atomic. Operator-facing
publication/activation and live-account qualification retain their later tasks.
