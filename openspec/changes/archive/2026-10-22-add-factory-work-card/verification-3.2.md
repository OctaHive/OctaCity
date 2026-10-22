# Task 3.2 — Shared recoverable Project text publication

## Scope and shared ownership

`ProjectTextIntake` supplies one provider-independent application service for
Project-owned task/acceptance publication and ordinary manual Factory admission.
Its `ProjectTextInputStore` and `ProjectInputQueryStore` contracts have memory
conformance. `PublishIssueExecutionCommand` is a narrow adapter from a retained
Issue request; standalone submissions need neither an Issue nor an installed
Source. Manual Issue creation continues through its existing independent handler.

The shared operation reserves original private text, Project/security-scoped replay,
logical/upload identities, input roles, exact digests/sizes, mapping, target/source
policy and classification before physical I/O. VCS resolution is frozen once under
the current preparation fence before any object write. Retry observes/verifies an
existing object before writing a missing object; the complete pair is published
only after independent byte verification. Unknown writes preserve the original
identities and frozen base. Corrupt bytes never become admitted inputs.

The service reuses ordinary Project/admission rights, immutable target checks and
Repository source selection, ordinary manual admission probing, one common Work/
Run constructor, and the existing admission transaction. The memory contract links
the original ordinary admission receipt atomically with the Project-input record.
Completed scoped replay precedes target/source resolution; current rights and exact
intent still apply. No second execution owner, admission path or outbox is added.

Preparation uses a 30-second fenced lease, an original five-minute deadline and
at most 16 ownership attempts. Replay does not renew these bounds or the original
30-day private input lifetime. Every byte effect and freeze/publication/admission
checks the current owner. A failed eager attempt releases only its own lease;
interrupted owners can be replaced after lease expiry. Cleanup identities are
retained before effects. These bounds are distinct from the eventual Run budget.

Private UTF-8 documents are nonblank and limited to the existing 1 MiB prepared
input ceiling per document and aggregate Context bound. Tabs, CR/LF and Unicode
bytes are preserved; other control characters are rejected. Deserialized payloads
are revalidated. Quickstart retains its stricter 16,384-byte ingress ceiling.
Bodies are excluded from generic Debug diagnostics.

`FactoryInputObject`/`FactoryInputRole` provide common immutable byte metadata;
existing Source names are aliases preserving their serialized shape. Source and
Project input reads reuse one byte/ownership/retention verifier, with different
provenance domains. Project input reads require exact Project, logical identity,
digest/size, plain-text media and sufficient retained lifetime. Ordinary output/
evidence reads reject these inputs; no Build/Job/Agent provenance is fabricated.

The Issue adapter checks current Issue/admission rights and the request's original
security partition, reads its exact retained revision for first publication and
uses the retained shared intent for resume. It checks original digests/sizes and
retains reviewed classification. Its response contains a Work/Run link only after
Issue association commits. Association failure preserves admission for replay;
there is no Work deletion or replacement as compensation.

## Quickstart reconciliation and qualification boundary

[The quickstart handoff](../../add-factory-bugfix-quickstart/shared-project-inputs.md)
records the concrete API and remaining quickstart tasks. Quickstart must reuse
this publication operation, owner fences and retained reader instead of creating
another submission journal or publisher. Its preset/readiness and stricter closed
ingress schema remain its own work; no quickstart task is marked complete.

This task qualifies application/store behavior with memory and existing byte
ports. PostgreSQL migrations/transactions, process restart crash qualification,
runtime input-reader composition, signed transfer, retention/holds/physical
cleanup and supervised recovery remain task 3.3. HTTP/generated contracts and
Console execution controls remain tasks 3.4–3.5. No live provider, paid model,
Docker/image build, deployment, stand mutation or commit is part of this task.

## Public-boundary Red/Green evidence

Repository AGENTS.md selects public application/store contracts as the test
boundary. Each slice's test preceded implementation. Missing-contract compilation
failures below concern newly required APIs; fixture/import/borrow compiler mistakes
were corrected separately and are not counted as behavioral Red.

| Slice | Observed Red | Observed Green |
| --- | --- | --- |
| Shared input publication | Shared intake/documents/metadata APIs absent | Exact UTF-8 pair, reserved identities and frozen base before writes, one ordinary Work/Run, original replay and Project provenance |
| Text validation | Empty/invalid documents were accepted | Nonblank/byte/control validation before reservation or resolution, deserialization revalidation and closed caller proof fields |
| Authority and review | Submission succeeded without Project rights | Missing Project/admission/definition visibility, unknown exact versions and forbidden source reject before effects; changed key reuse conflicts |
| Uncertain/corrupt bytes | Recovery performed three writes instead of two after a lost first-write acknowledgement | Observe/verify-before-retry, no second resolution after restart, exact original objects, and corrupt-byte refusal without publication/admission |
| Ownership bounds | Required preparation guard/fence APIs absent | Active/stale owners reject, first freeze cannot be overwritten, and deadline/attempt/lifetime are never renewed |
| Retained reads | Project input reader extension absent | Exact private bytes without producing Build, cross-Project/media/digest/time/retention rejection, and no output/evidence authority |
| Issue integration | Explicit Issue publication command/adapter absent | Exact original revision after edits, original security scope, association failure/restart recovery to one Work/Run and frozen classification |

Cargo environment for focused and regression commands:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

Focused Red/Green commands:

```sh
cargo test --locked -p octacity-server-application --lib project_text_intake_publishes
cargo test --locked -p octacity-server-application --lib project_text_intake_rejects_invalid
cargo test --locked -p octacity-server-application --lib project_text_intake_requires_reviewed
cargo test --locked -p octacity-server-application --lib project_text_intake_recovers_unknown
cargo test --locked -p octacity-server-application --lib project_text_intake_fences
cargo test --locked -p octacity-server-application --lib project_text_intake_inputs_are_readable
cargo test --locked -p octacity-server-application --lib project_text_intake_issue_publication
cargo test --locked -p octacity-server-application --lib project_text_intake_
```

All seven scenarios passed before final regression. Existing public admission/
Source behavior passed 87 tests before shared admission/metadata/reader refactoring;
the store baseline passed 81 tests. The common test-support module was consolidated
after Clippy detected duplicate module loading, preserving the feature-gated
`octacity_server_application::testing` alias and avoiding lint suppression.

## Qualification commands

```sh
cargo test --locked -p octacity-server-store -p octacity-server-application --lib
cargo clippy --locked -p octacity-server-store -p octacity-server-application \
  --all-targets --all-features -- -D warnings
cargo check --locked -p octacity-server
cargo check --locked -p octacity-server-application --all-targets --all-features
cargo test --locked -p octacity-server-application --all-features --lib project_text_intake_
cargo fmt --all --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
openspec validate add-factory-bugfix-quickstart --strict
git diff --check
```

Documentation/handoff edits have no executable behavior. They use formatting,
whitespace, link checks and strict OpenSpec validation rather than an invented
Red/Green cycle.

## Final results

| Qualification | Result |
| --- | --- |
| Application/store library regression | 526 application and 81 store tests passed; zero failures |
| Shared intake with all features | All seven behavioral scenarios passed |
| Clippy, all targets and all features | Passed with warnings denied |
| Server composition check | Passed |
| Application all-target/all-feature check | Passed |
| Rust formatting | Passed |
| Architecture boundaries | Passed, 63 packages and 256 edges |
| Both OpenSpec changes, strict validation | Passed |
| Documentation links and diff whitespace | Passed |

Feature qualification found ambiguous test-support imports after consolidating the
shared module. Explicit store imports corrected test compilation without changing
assertions; the final all-feature behavioral run and Clippy passed afterward.

Existing target outputs were reused with incremental compilation and debug
information disabled and two build jobs. Available disk space was checked around
builds: approximately 21 GiB initially and 19 GiB at completion. No Docker images,
additional target directory or cache deletion was needed.

Task 3.2 is complete; work-card progress is 16/56. Task 3.3 and all quickstart
implementation tasks remain unchecked. No commit was created.
