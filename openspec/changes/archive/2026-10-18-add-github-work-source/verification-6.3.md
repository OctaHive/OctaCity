# Task 6.3 — Setup guide and validated editable input examples

## Delivered scope

[Factory task-source setup](../../../../docs/reference/factory-work-sources-setup.md)
documents the full operator sequence and the separate installation-owner boundary:
reusable connections, protected provision/adopt/rotate/revoke, explicit activation,
Project/resource/purpose grants, bounded read-only checks, Source installation,
independent task/code resources, normalized filters, input/stage effects, retained
history, retries/conflicts and dependency-aware rollback/retention.

It distinguishes immutable input replacement, an explicitly new automatic input ID,
existing execution retry and a future card-owned execution request. Native task
correlation remains separate from input-scoped admission. Observation does not
request execution, change frozen Work, or establish a refresh policy. Import/link
refresh, association/field ownership/conflicts and manual card creation/execution
remain in `add-factory-work-card`. Manual Artifact intake stays available without
a connection. No additional provider, card, loader or credential mechanism is added.

The guide and existing Flow/management/diagnostic references link to four editable
[settings examples](../../../../config/factory/work-sources/README.md):

- `assessment.json`: unlabeled open intake excluding already evaluated labels;
  explicit accepted defect/feature and declined effects.
- `defect.json`: required approval/defect labels, rejection exclusion and independently
  configured completion mapping.
- `feature.json`: approval plus either alternative feature label and rejection
  exclusion; it can be an exclusive alternative to defect.
- `documentation.json`: an unrelated review/documentation vocabulary and explicit
  additions/removals, with no privileged approval/classification names.

Each file contains only three existing `WorkSourceInputDraft` fields: `enabled`,
`predicate`, `reporting`. All start disabled and preserve native state. The operator
selects the remaining actual granted source/connection/target/Repository references
through the UI or generated typed client. These are not complete REST submissions,
an input-file import feature or a new format automatically loaded at startup.

## Public test boundary

`server/application/src/factory_source_example_tests.rs` reuses the existing
isolated source-configuration fixture. It reads the actual JSON bytes, accepts only
the closed three-field settings shape, and applies the unchanged predicates/effects
to an independently declared installed editing policy and separate current read/write
grants. It uses `AuthorizedQueryHandler<ValidateWorkSourceConfigurationQuery>`,
`AuthorizedCommandHandler<PublishWorkSourceConfigurationCommand>` and the public
store read contract. Disabled examples are validated first; only the test's in-memory
inputs are explicitly activated and published. The retained definitions are read
back through the public store boundary.

Expected selector outcomes and native effects are literal requirements independent
of the configuration parser. Public `WorkSourceGroup::select/finish` and predicate
contracts prove one implementation route or explicit ambiguity; no private resolver,
mocked internal collaborator or second runtime router is used. Tests use GitHub,
GitLab, Jira and custom common-contract identities, four variants per test. These
are application configuration contracts, not native GitLab/Jira or live GitHub
qualification. The native installed adapter still validates its own managed-label
and state scope before use.

## TDD evidence

The existing source-configuration suite first passed unchanged: **24 tests**.
The examples were then added one vertical slice at a time, after their behavioral
tests were written and run. The test environment and existing public APIs were
available; each Red was the specifically required new configuration artifact being
absent, not a compiler, service, permission or dependency failure.

All commands used:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

| Command following `cargo test --locked -p octacity-server-application --lib` | Observed Red before example creation | Observed Green |
| --- | --- | --- |
| `documented_assessment_settings` | `required editable input example assessment.json is missing` | 1 test passed, four provider variants; unlabeled assessment, exclusions and configured accepted/declined effects passed public validation/publication. |
| `documented_defect_and_feature` | `required editable input example defect.json is missing` | 1 test passed after adding both alternative input settings; required/alternative labels, rejection and two-member ambiguity behaved as specified. |
| `documented_unrelated_labels` | `required editable input example documentation.json is missing` | 1 test passed, four provider variants; unrelated labels selected only their declared route and exact additions/removals, without an inferred approval action. |

The focused examples remain reproducible with:

```sh
cargo test --locked -p octacity-server-application --lib source_configuration::examples
```

The setup/reference prose has no executable behavior and uses the repository's
documentation-only TDD exemption. No production behavior was changed to make the
examples pass. Existing finite limits and authorization/publication semantics remain
in force; no timeout increase, serialized test execution or dependency was added.

## Final verification

```sh
cargo test --locked -p octacity-server-application --lib factory_admission_tests
cargo clippy --locked -p octacity-server-application --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
python3 tools/security_matrix.py validate
python3 tools/verify_local_stand_context.py
openspec validate add-github-work-source --strict
openspec validate add-factory-work-card --strict
git diff --check
```

The relevant regression suite passed **87 tests**, including the three new examples
tests, existing manual admission, input-scoped replay, immutable publication and
stage compatibility. Clippy/formatting, architecture (63 packages, 254 edges),
security matrix, local-stand context, both OpenSpec changes and diff checks passed.
JSON syntax and local Markdown links were checked independently as documentation
validation, in addition to the behavioral tests above.

The existing target directory was reused with incremental/debug output disabled
and two build jobs. Space was checked before and during builds and remained
approximately **12 GiB**. No cache/user files were deleted, credentials read,
network/provider writes performed or stand/service started. Uncommitted 6.1–6.2
work was preserved. Their hermetic qualification and blocked/unqualified live status
remain distinct. The final broader release regression gate is still task 6.4.
