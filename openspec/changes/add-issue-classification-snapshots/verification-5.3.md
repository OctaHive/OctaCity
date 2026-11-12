# Task 5.3: delivery routing from frozen Issue data

## Behavior and public boundaries

The released v2 Flow is resolved and published through ordinary authorized
Factory handlers. A manually prepared Issue supplies its Project-defined
classification and explicit implementation acceptance through the authorized
Issue creation, execution request and publication commands. No producing Flow
or Run is selected. Tests then edit the live Issue before reconciling delivery.
The common Factory owner retains the original Incoming data and Work.

The route matrix independently specifies these outcomes:

| Frozen fields | Next stage |
| --- | --- |
| Feature, small, API, high priority | Protected tests |
| Feature, medium, compiler component, high priority | Protected tests |
| Feature, medium, API, low priority | Protected tests |
| Feature, medium, API, high or blocker priority | Human requirements review |
| Feature, large | Isolated specification author |
| Defect, unknown reproduction | Reproduction command |
| Defect, needs input | Operator reproduction review |

The live edits change size, component, priority, type and reproduction choice to
values that would select another route. The assertions use the original
expected route and original values, not a recalculation of the implementation's
predicate. Deterministic/human routing stops at the ordinary external Build
boundary; it neither fabricates Build results nor dispatches an unselected stage.

The medium review test observes the pending gate, confirms that waiting is
stable and dispatches no tests, rejects a generic response attempting to bypass
the typed operator command, and exercises both authorized approve and reject.
Approval freezes an Accepted Work Contract with the exact reviewed input;
rejection selects correction without an accepting result or downstream Build.
The command-use-case boundary receives an authenticated actor and explicit grant,
as in the existing human-gate contracts; it does not weaken the installed
trusted-network policy to accept that actor.

The large requirements branch is additionally qualified through the existing
ordinary Build adapter and independently qualified capture-output fixtures. It
consumes the same already frozen Issue payload and Work in an isolated branch
Run, while the actual admitted Run remains unchanged. It retains proposal,
design, tasks and delta-specification paths, manifest and bundle, and preserves
the original choice-valued priority, component and size through specification
acceptance. Matching API/high fields request human review; compiler or low
priority continue automatically. Both approve and reject are exercised. There
is one specification-author invocation before tests and no extra AI reviewer.

Existing common-owner delivery regressions continue to qualify protected-test
isolation and observed behavioral failure, candidate review before validation,
mandatory independent verification, trusted PR publication, expired/negative
proof rejection and recovery of the original publication. These are reused
owners and unchanged released parts, not newly simulated authority. A packaged
end-to-end Issue-to-PR journey remains task 7.1.

## Observed Red/Green

The first valid public v2 execution request failed at
`/issue_snapshot/fields/dependencies` with `issue_snapshot_inapplicable`: a normal
Issue's empty dependency list had no assessment origin. This prevented an
otherwise eligible manually prepared feature from entering delivery.

The minimal product correction keeps the canonical empty array but attaches a
dependency-assessment reference only to a nonempty claim. An empty collection
neither invents provenance nor claims a verified dependency. The first routing
test then passed (1/1, 1.92s), including live Issue edits and unchanged admitted
Work/Incoming data. A negative regression verifies that retained nonempty
dependencies without attribution still reject before another admission.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib \
  issue_delivery_routes_the_original_small_feature_after_issue_edits -- --nocapture
```

Further route/review qualification passed through:

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib issue_delivery_ -- --nocapture
```

At that point all nine selected tests passed (17.87s): five prior resolver
contracts and four new routing/review tests. The nonempty-dependency regression
was added next and included in final qualification.

Compilation, generic fixture policy, test-actor authorization and route-driver
setup failures were corrected and were not counted as behavioral Red. The
original 14 delivery owner tests passed before extending their shared fixture
(127.09s, using the existing compiled application test binary). The extended
fixture continues to load v1 unchanged, while selecting the three released v2
requirements parts for the new branch qualification. No existing JSON asset,
executor implementation, permission policy or database schema changed.

Documentation-only edits have no executable behavior and need no separate
Red/Green cycle.

## Final qualification

Observed results:

- Full application unit suite with all features: **1,153 passed**, no failures
  or ignored tests (540.81s). This includes all five new tests, the negative
  nonempty-dependency regression and all existing delivery proof/publication
  scenarios after the shared fixture extension.
- Selected Issue-delivery resolver/routing/review tests: **9 passed** (17.87s),
  before adding the final nonempty-dependency regression.
- Application Clippy, all targets/features with warnings denied.
- Clippy compilation of the shared PostgreSQL `factory_configured_flow` test
  target, all features with warnings denied. Native services were not started;
  this check verifies reuse of the extended fixture, not a PostgreSQL test run.
- Rust formatting, repository architecture, strict OpenSpec and whitespace checks.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib --all-features -- --quiet
CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-application --all-targets --all-features -- -D warnings
CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server --test factory_configured_flow --all-features -- -D warnings
CARGO_INCREMENTAL=0 cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```
