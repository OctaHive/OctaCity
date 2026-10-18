# Task 1.4 verification: explicit exclusive source input groups

Status: complete for task 1.4.

## Scope and public boundary

Task 1.4 adds the provider-neutral `WorkSourceGroup` publication, replacement
and selection interface in `octacity-server-factory`. Tests use public
constructors and outcomes; shared test fixtures publish ordinary exact Factory
targets and source inputs. No private selection helper is tested or mocked.

Each immutable Project-owned revision freezes 1–64 complete input definitions,
including their exact target/root Flow and adapter configuration references.
Members have distinct stable input identities and share the exact Source
installation, capability and ticket container. Code Repositories and selector
configurations may differ. Publication and replacement validate the same
invariants; replacement preserves group identity and accepts only the next
positive revision. The bound of 64 is a local safety limit.

`select` evaluates only enabled members against one borrowed provider-owned
observation. A pending pass exposes no disposition until `finish` compares its
complete frozen group with the resolved current group. Zero matches yield
`Filtered`, one yields the exact frozen input in `Selected`, and multiple yield
`Ambiguous` with the matching stable input identities and no selected target.
Different groups do not impose exclusivity on one another.

## Baseline

Before implementation:

```sh
cargo test --locked --jobs 2 -p octacity-server-factory source_input -- --test-threads=2
```

Observed: all 6 existing source input publication tests passed (280 filtered
out). The existing task 1.2 input contract remained unchanged.

## Observed Red/Green cycles

Each row was written and run before its corresponding production change.
The accumulated selection suite was rerun after each minimal implementation.
Missing new public APIs are behavior-related compiler failures, not
environment failures.

Command for each Red and Green run:

```sh
cargo test --locked --jobs 2 -p octacity-server-factory exclusive_groups -- --test-threads=2
```

| Cycle | Independently specified behavior | Observed Red | Observed Green |
| --- | --- | --- | --- |
| 1 | An exclusive group with no matches filters the observation, including observations containing example business labels | E0432: group/identity/disposition public APIs absent | 1 passed |
| 2 | One match selects its exact input independently in two groups, with GitHub/GitLab/Jira/custom installations and different label vocabularies | E0599: `Selected` absent | 2 passed |
| 3 | Multiple matches have an ambiguous disposition; a separate group still selects its own input and a later corrected observation can select one member | E0599: `Ambiguous` absent | 3 passed |
| 4 | A revision change during evaluation invalidates filtered, selected and ambiguous old passes; every evaluated member still comes from the original revision and observation | E0432/E0599: group version, replacement and inspection APIs absent | 4 passed |
| 5 | Publication/replacement reject empty or oversized membership, duplicate stable identities even across input versions, foreign Projects and incompatible observation scopes | Test failed: an empty group was accepted (`unwrap_err` received `Ok`) | 5 passed |
| 6 | Disabled inputs cannot match or create ambiguity and are never evaluated | Test failed: disabled plus enabled inputs produced `Ambiguous` instead of the enabled input's `Selected` | 6 passed |

Cycle 4 also verifies stable group identity, unchanged original serialized
definition, exact new input/target versions after a fresh pass, rejection of
same/skipped replacement revisions, and rejection of substituted definitions
under the same group identity/version. Cycle 5 accepts exactly 64 members and
rejects 65 through both publication paths.

## Provider and authority boundaries

The group consumes a typed observation and a caller-supplied pure predicate;
it never interprets label strings or pipeline names. GitHub label/state
semantics stay in `octacity-github::issue_selector`. A GitLab or Jira adapter
can resolve its own pinned selector and use native fields/statuses without
adding a provider enum or wire DTO to the Flow interpreter. Tests include
opaque numeric tracker observations as well as label-shaped observations.
This does not qualify live GitLab/Jira integrations.

The trusted predicate must resolve each exact frozen adapter configuration;
the caller must handle provider/normalization failures before selection rather
than turn them into negative matches. Groups from different installations or
ticket containers can select independently, but cannot share one observation
within a single exclusive group.

These are pure publication/selection contracts, not authorization, durable
admission, scan ownership or persistence. `finish` requires an authoritative
current group supplied under the future store transaction and scan fence.
Atomic publication/current-version fencing belongs to task 2.3; worker
composition belongs to task 3.5. The in-memory check does not close a later
check/commit race on its own or lock external tracker edits. Selection neither
reserves identities nor cancels, migrates or changes existing Work.

No network calls, credentials, new dependencies, migrations, UI, admission
behavior or process activation were added. Errors contain static field names,
without provider payloads or submitted values. Published groups are not
deserializable without validation.

## Regression and quality checks

```sh
cargo test --locked --jobs 2 -p octacity-server-factory -p octacity-github -p octacity-delivery-protocol -p octacity-server-delivery --all-features -- --test-threads=2
cargo clippy --locked --jobs 2 -p octacity-server-factory --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
git diff --check
```

Observed: 292 Factory tests and 29 GitHub/Delivery tests passed, plus 1 Factory
compile-fail doctest. Clippy and formatting passed; architecture validation
passed with 61 workspace packages and 245 dependency edges. No dependencies
or workspace edges changed.

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_flow_action_tests -- --test-threads=2
openspec validate add-github-work-source --strict
```

Observed: all 10 admission and 15 configured action/status/outbox tests passed.
In total, the final scoped regression runs passed 346 behavioral tests plus
1 compile-fail doctest, with no failed or ignored tests. The full application
suite was not run; the two relevant public contracts were selected explicitly.
OpenSpec strict validation passed. Documentation-only changes have no executable
behavior; their validation uses formatting, diff and specification checks.

Task 1.4 is marked complete; change progress is 4/29. Tasks 2.1 and later remain
pending. No commit, staging, provider activation or live tracker mutation was
performed; earlier uncommitted task 1.1–1.3 work was preserved.
