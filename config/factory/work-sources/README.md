# Editable automatic input settings

These four files supply values for three existing `WorkSourceInputDraft` fields:
`enabled`, `predicate` and `reporting`. They are **settings fragments**, not full
input publication requests, manifests or a new format loaded by the server.
Use the [setup guide](../../../docs/reference/factory-work-sources-setup.md) for the
normal UI path and the [typed management contracts](../../../docs/reference/integration-management-contracts.md)
for API composition.

| File | Intended use |
| --- | --- |
| [assessment.json](assessment.json) | Open tasks, including unlabeled tasks, excluding already approved/rejected tasks. Explicit accepted defect/feature and declined mappings. |
| [defect.json](defect.json) | Approved defects, excluding rejected tasks; completion adds `implemented`. |
| [feature.json](feature.json) | Approved tasks carrying either feature alternative, excluding rejected tasks; independently editable completion effects. |
| [documentation.json](documentation.json) | A separate example using review/documentation labels and a `documented` action, with no approval/classification vocabulary. |

All names, states, predicates, status rows, additions and removals are editable.
`enabled: false` prevents copying a fragment from activating intake. Each nullable
effect `state` is initially `null`, preserving native state. An empty removal list
preserves existing labels; an addition does not replace the Issue's full label set.
The literal `open` state must be advertised by the selected Source. Another
provider may advertise different opaque states; select its actual supported values.

## Apply through an authorized input

Select a visible connection/native resource and existing Source profile, an exact
target Factory Configuration and the independently selected code Repository. Select
a separately granted compatible Reporter if using these reporting rows. The installed
Source must advertise normalized predicate editing over its unrestricted base;
the Reporter must advertise the exact managed label/state editing ceiling. Keep
provider configuration, selector/mapping and parent connection pins from authoritative
choices. Never fabricate digests or copy another deployment's resource IDs.

Copy the predicate into the guided state/all/any/excluded controls and the reporting
rows into the existing status-mapping controls, or set those three fields in a typed
input intent using the generated API client. The complete request still needs its
Project, stable input ID/current version and exact granted installation/target/
Repository references. The samples contain no authentication, grants or target IDs.
Validate/preview the whole intent, review it, and explicitly enable/publish it.
The server does not expose a JSON-fragment import button or discover these files
at startup.

Create distinct stable inputs and exact targets for assessment, defect and feature.
For alternative defect/feature routing, publish an explicit exclusive group over the
reviewed member IDs/versions. Required labels are ANDed, alternatives are ORed, and
exclusions win. Both implementation label sets produce visible ambiguity and no
admission. An ungrouped input remains independent. Repeated matching after adding
`implemented` reuses the original admission; changing labels is not an execution retry.

Publish the corresponding ordinary Flow status actions on the accepted outcome
routes. `accept-defect`, `accept-feature`, `decline`, `implementation-complete` and
`documented` have no built-in runtime meaning. Each resolves only its configured
row after an accepted action; an unknown value fails safely. An input mapping is
not an automatic action, and per-stage typed `source_update` effects override its
default for that accepted action. GitHub-managed labels must already exist.

## Validation and qualification limits

[Public application contract tests](../../../server/application/src/factory_source_example_tests.rs)
read the actual files, bind their unchanged predicates/effects to isolated installed
connection/Project/Repository fixtures, validate disabled drafts, explicitly activate
only those in-memory test inputs, publish them and read the retained definitions.
The tests independently assert unlabeled assessment, explicit effects, rejected
exclusion, alternative labels, exclusive ambiguity and the unrelated vocabulary.
They run against GitHub/GitLab/Jira/custom common-contract identities, without
claiming native support for additional providers or live repository access.

Run the focused checks without creating another build cache:

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2 cargo test --locked -p octacity-server-application --lib source_configuration::examples
```

Installed native configuration and actual provider access still need their own
validation. The canonical private GitHub
[Source installation fixture](../../../server/infrastructure/octacity-github/tests/support/issues.rs)
illustrates the existing file layout with synthetic credentials only. Shared public
[read profile](../../../server/app/src/runtime/work_source_read_profile.rs),
[Source composition](../../../server/app/src/runtime/work_source.rs),
[Reporter profile](../../../server/app/src/runtime/work_reporter.rs) and
[read-only checker](../../../server/app/src/runtime/integration_check.rs) define the
installation boundary. Neither these fragments nor their tests deploy that composition.
Use the [opt-in live procedure](../../../docs/reference/github-work-source-live-qualification.md)
only after explicitly selecting a disposable repository and its installed grants.
