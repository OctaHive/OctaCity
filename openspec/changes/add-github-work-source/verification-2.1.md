# Task 2.1 verification: trusted source-neutral admission

Status: complete for task 2.1; change progress is 5/29. Task 2.2 and later remain pending.

## Scope and public boundary

This slice adds `FactoryWorkAdmissionIntent` and the mandatory installed-source
ingress `FactorySourceAdmissionHandlers::admit`. The existing
`AdmitManualFactoryWorkCommand` name remains a compatible alias and keeps its
management authorization contract. Both use the same authoritative admission
store, immutable Work/Run construction, repository selection/resolution and
atomic publication. No second duplicate or admission authority was introduced.

`FactorySourceAuthorizer` is a trusted server port bound to a verified installed
source. It must authorize the exact input for that installation on every call
and return the accepted installation-owner context. There is no default
implementation and no provider or management transport accepts an implementation
or a claimed context. Tests supply controlled implementations at this public
authorization boundary and use the real in-memory admission store.

Before replay or VCS access, the source handler validates:

- Exact input identity/version, enabled state and independently installed Source
  role, Project and complete installation/capability/configuration/resource binding.
- The intent's Project and exact Configuration/Repository identities and versions
  against that input.
- Current Project visibility, execution permission and visibility of Configuration,
  Repository, task, acceptance and every specification Artifact.

Before resolving a new candidate's repository source, the common path also
compares the selected Configuration reference/root Flow and Repository pin to
the authoritative definitions. The Repository pin is SHA-256 of the published
`RepositoryDefinition`'s normalized JSON serialization.

The source namespace is derived from the authorized installation's stable
identity and Project; the accepted owner context supplies its security scope.
Transport retry keys are separately namespaced because the existing store's
transport-key partition is wider than its external-identity partition. Raw
source names, actors, scopes or replacement authority cannot be supplied in a
ticket intent. Even an installation named `manual` receives an installed-source
namespace. Audit context comes only from the trusted authorizer.

## Baseline and preserved manual behavior

Before implementation/refactoring:

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
```

Observed: all 10 existing tests passed (374 filtered out). The command required
a cold dependency build, which completed successfully before production edits.

The manual source kind, original intent digest domain/field order, transport
keys, security partition and replay-before-definition/VCS-resolution order
remain unchanged. Existing manual public tests were rerun after extraction and
again after the source authorization and pinned-definition checks.

## Observed Red/Green cycles

New tests were added one at a time before each corresponding production change.
Red runs used:

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib installed_sources -- --test-threads=2
```

Greens used that accumulated suite or the encompassing manual/source suite:

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
```

| Cycle | Required behavior | Observed Red | Observed Green |
| --- | --- | --- | --- |
| 1 | Uninstalled/unauthorized source fails without VCS resolution or publication | E0432 for missing source command, authorizer, authorization result and handler APIs (after correcting a test import) | 1 source test passed |
| 2 | Installed providers share ordinary admission but cannot obtain or replace manual Work, even with equal external identities and transport keys | Installed intake returned `CapabilityUnavailable` instead of admitting separate Work | 2 source tests plus 10 manual tests passed |
| 3 | Exact enabled input and Project-scoped Source capability are mandatory | Wrong resolved/requested input identity was accepted and published Work | 3 source tests passed |
| 4 | Ticket intent cannot substitute Project, target Configuration or code Repository/version | Substituted Project reached the store and returned `NotFound` instead of safe early `Invalid` | 4 source tests passed |
| 5 | Hidden dependencies and denied execution prevent both new admission and disclosure of replay receipts | Hidden Project returned `Replayed`; a subsequent Red also exposed an explicit `None` execution visibility grant being accepted | 5 source tests plus 10 manual tests passed after both paths were fixed |
| 6 | Pins must match authoritative Configuration content, root Flow and Repository content before VCS resolution | Altered Configuration digest still produced `Applied` | 6 source tests plus 10 manual tests passed |

Cycle 2 covers GitHub, GitLab, Jira and a custom installation, then characterizes
an installation literally named `manual`. Both source and manual retries retain
their own Work/Run identities, with only two VCS resolutions and two publications
per case. Cycle 3 covers wrong input identity/version, disabled input, Reporter
role, foreign capability Project and substitution of each binding reference.
Cycle 4 covers all five selected identity/version fields. Cycle 5 checks every
protected resource under both an explicit policy denial and empty visibility,
using both an existing receipt and a fresh candidate; the original Work snapshot,
audit and mutation counts remain unchanged. Cycle 6 separately checks the
Configuration digest, pinned root and Repository digest.

## Scope limits and provider independence

The public intent and authorization contracts carry no GitHub DTO, Issue-state
enum, Jira workflow or provider SDK. Concrete adapters normalize observations
outside admission. Existing repository source policy, Flow construction, WIP
limits, immutable artifact references, audit and publication remain shared.

The port contract requires live process/grant verification and current input
availability from its concrete implementation; this task does not install one
or activate any worker. Stable input/resource/item identity construction and
retained-intent recovery across changing observations/input revisions remain
task 2.2. Durable source definitions/provenance and scan fencing remain task 2.3;
Source process/credential integration and worker composition remain tasks 3.x.
This slice qualifies the admission boundary, not live GitHub/GitLab/Jira intake.

No dependency, migration, REST/UI schema, credential store or Flow interpreter
changed. No live tracker calls, provider activation or user stand changes were
performed. No commit was created for this task.

## Regression and quality checks

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_flow_action_tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --lib management_security::tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-factory -p octacity-github -p octacity-delivery-protocol -p octacity-server-delivery --all-features -- --test-threads=2
cargo clippy --locked --jobs 2 -p octacity-server-application --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

Observed: 16 admission tests (10 existing and 6 new), 15 configured action/status
tests, 16 management security tests, 292 Factory tests and 29 GitHub/Delivery
tests passed: 368 distinct behavioral tests plus 1 compile-fail doctest, without
failures or ignored tests in the final runs. The full application/release
journey suite was not run; relevant public admission, reporting and security
contracts were selected explicitly. No PostgreSQL migration changed or was
qualified in this slice.

Clippy completed with warnings denied. Formatting passed after normalizing the
new module declaration's order. Architecture validation passed with 61 workspace
packages and 245 dependency edges; strict OpenSpec validation and diff checks
passed. Documentation-only changes have no executable behavior and use these
documentation/specification checks rather than a TDD cycle.
