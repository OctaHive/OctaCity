# Task 1.3: typed provider-owned Issue selectors

Status: complete for task 1.3 only. Verification date: 2026-10-16.
Base commit: `4e20afafc02795042fa3f331fcc37ecdeeb1c74f`.
The uncommitted task 1.1–1.2 work was retained.

## Public adapter/configuration contract

`octacity_github::issue_selector` owns `IssueSelectorDraft`, the validated
immutable `IssueSelector`, separate observed/configured state enums and safe
field/reason errors. External integration tests exercise `IssueSelector::new`,
`IssueSelector::from_config`, serialization and `IssueSelector::matches`.
They do not call private validators or mock internal collaborators.

Matching requires every `all_of` name, at least one alternative when `any_of`
is nonempty, and no `none_of` name. Three empty arrays select labeled and
unlabeled Issues within the selected `open`/`closed`/`all` state. State selection
does not bypass label predicates. Names are whole literal values including
Unicode, spaces and punctuation; no name selects a business stage, and regex
metacharacters have no expression semantics.

Validation rejects duplicate labels within a predicate, required/excluded
overlap, and a nonempty alternatives set entirely excluded by `none_of`.
Partly excluded alternatives and overlap between required and alternative
labels remain valid. Local adapter limits are 32 names per predicate, 256
UTF-8 bytes per name and 64 KiB of raw selector JSON. Empty/whitespace-only
names, control characters and oversized names are invalid. Configured spelling
and surrounding spaces in otherwise valid literal names are preserved.

Local matching, duplicate detection and contradiction checks compare Unicode
lowercase forms. GitHub documents label-name parameters as
[case insensitive](https://docs.github.com/en/rest/issues/labels#remove-a-label-from-an-issue).
The chosen local comparison policy is tested with ASCII and Cyrillic case
changes; this slice does not qualify every provider Unicode collation behavior.

The checked decoder requires all four typed fields, rejects unknown predicates,
unsupported state values, duplicate JSON fields, malformed shapes/UTF-8 and
oversized documents, then applies the same semantic constructor validation.
Errors contain only a closed field/reason classification, with no input values
or underlying decoder messages. A validated selector has no unchecked
deserialization constructor.

## Observed Red/Green cycles

Each Red command used this exact pattern with the filter below:

```shell
cargo test --locked --jobs 2 -p octacity-github --test issue_selectors <filter> -- --test-threads=2
```

| Filter | Observed Red before implementation | Green outcome |
| --- | --- | --- |
| `compound_predicates` | Public `issue_selector` module absent, E0432; exit 101. | Compound all/any/excluded predicates match independently specified cases using arbitrary literal names; 1 passed. |
| `empty_predicates` | Empty observed labels did not match an unrestricted selector; assertion failed, exit 101. | Unrestricted/unlabeled, required-only and excluded-only cases work; accumulated 2 passed. |
| `issue_state_selection` | An open-only selector matched a closed Issue; assertion failed, exit 101. | All six configured/observed state pairs and independent label filtering work; accumulated 3 passed. |
| `contradictory_predicates` | Required/excluded overlap was accepted; `unwrap_err()` failed, exit 101. | Contradictions return safe field errors; satisfiable overlaps/alternatives remain valid; accumulated 4 passed. |
| `duplicate_labels` | Repeated required label was accepted; `unwrap_err()` failed, exit 101. | Each predicate rejects duplicates without echoing names; accumulated 5 passed. |
| `predicate_counts` | A 33-label predicate was accepted; `unwrap_err()` failed, exit 101. | 32/33-entry, 256-byte Unicode and oversized/invalid-name boundaries verified for each predicate; accumulated 6 passed. |
| `label_case` | ASCII/Cyrillic case changes hid a positive match; assertion failed, exit 101. | Matching, exclusions, duplicates and contradictions use the same comparison while preserving serialized spelling; accumulated 7 passed. |
| `configuration_decoding` | Checked `from_config` API absent, E0599; exit 101. | Strict decoding, 65,536/65,537-byte boundaries, round-trip and semantic revalidation work; accumulated 8 passed. |

The first Green used its specific `compound_predicates` filter. After each
subsequent implementation the accumulated integration suite was run:

```shell
cargo test --locked --jobs 2 -p octacity-github --test issue_selectors -- --test-threads=2
# Final result: 8 passed; 0 failed; 0 ignored.
```

All Red outcomes came from absent APIs or missing required behavior, not
environment failures. The bounds in assertions are independently declared
literal boundary examples, not expected values calculated from implementation
constants or matcher logic.

## Baseline and final verification

The existing provider/Delivery baseline was run before implementation:

```shell
cargo test --locked --jobs 2 -p octacity-github -p octacity-delivery-protocol -p octacity-server-delivery -- --test-threads=2
# Before changes: 21 passed; 0 failed; 0 ignored.
# After changes: 29 passed; 0 failed; 0 ignored:
# 14 provider unit, 4 wire, 1 credential, 1 provider-process,
# 1 verified-host and 8 selector integration tests.

cargo clippy --locked --jobs 2 -p octacity-github --all-targets --all-features -- -D warnings
# Exit 0.

cargo fmt --all -- --check
# Exit 0.

python3 tools/check_architecture.py
# Exit 0: 61 workspace packages, 245 dependency edges.

openspec validate add-github-work-source --strict
# Change is valid.

git diff --check
# Exit 0.
```

The plugin README documents the checked API, four required fields, predicates,
comparison policy, bounds and current integration scope. Documentation changes
have no executable behavior; JSON syntax and OpenSpec structure are checked
separately from the behavioral TDD cycles above.

## Scope and security review

Only the existing GitHub provider owner gained selector code. The common
`WorkSourceInput` model and Flow interpreter have no new provider imports,
state enums, label lists or SDK dependencies. Future GitLab/Jira/other adapters
can supply their own typed selector and state semantics behind the existing
exact capability/configuration references. This is not a shared GitHub-shaped
ticket schema.

Selector configuration is untrusted bounded data. Constructor checks run
before an immutable matcher exists; byte limits run before JSON decoding.
Validation diagnostics do not include submitted values. The module contains
no credential resolution, filesystem/network access, provider writes, Work
admission or lifecycle authority. No dependencies or lockfiles changed.

The tests use local facts for selector behavior and controlled peers/local Git
repositories for existing Delivery regressions. No live GitHub/GitLab/Jira
account, user's repository or paid model call was used. This task does not
advertise a Source capability or enable a worker. Group evaluation remains
task 1.4; installed configuration/credential/process integration and Issue
listing/pull-request exclusion remain section 3. Full repository qualification
remains task 6.4.
