# Task 1.2: provider-neutral immutable Work Source inputs

Status: complete for task 1.2 only. Verification date: 2026-10-16.
Base commit: `4e20afafc02795042fa3f331fcc37ecdeeb1c74f`.
The uncommitted task 1.1 characterization and planning artifacts were retained.

## Public validation contract

`WorkSourceInput::publish` validates an initial Project-owned input;
`WorkSourceInput::replace` validates only the next immutable version while
preserving its stable `WorkSourceInputId` and Project. The validated record
exposes read-only settings and the root Flow selected by the exact published
Factory Configuration. Published input records are serializable, with no
unchecked deserialization constructor; drafts are closed typed inputs.

The code Repository reference pins Project, ID, version and definition digest.
It is independent of the ticket container. Source/Reporter bindings contain
only exact installation, capability, configuration and resource references.
The referenced provider configuration owns selector, mapping and policy
semantics; this task does not add a provider DTO or interpret those settings.
The provider test identities are GitHub, GitLab, Jira and a custom tracker.
They exercise the same public publication contract without a provider allowlist.

Inputs default to disabled when `enabled` is omitted. Disabled definitions can
pin a disabled target, but activation requires an enabled target. Both enabled
and disabled input publication require compatible installed declarations.
Source-only input needs no Reporter declaration; a selected Reporter must have
its own exact role/configuration, the same Project, installation and ticket
container, and cannot borrow Source authority.

`WorkSourceInputContext` and `WorkSourceCapability` are trusted application
resolution inputs, not deserializable provider responses or credential grants.
The application must supply authorized published resources and verified
installed metadata. Equality/ownership checks here do not authenticate callers
or prove a remote installation exists. No store, process host or network call
was added to this pure domain model.

## Observed Red/Green cycles

All tests call the public publication/replacement interface and use existing
public Factory Configuration constructors/fixtures. No private validator or
mock of an internal collaborator is asserted against.

The Red commands use this exact prefix and suffix:

```shell
cargo test --locked --jobs 2 -p octacity-server-factory source_input_tests::<filter> -- --test-threads=2
```

| Filter | Observed Red before its implementation | Observed Green |
| --- | --- | --- |
| `input_publication` | Missing public Work Source types/API, E0432; exit 101. | Exact source/target/root settings retained for all four provider identities; 1 test passed. |
| `input_publication_rejects_target` | A substituted Configuration ID was accepted; `unwrap_err()` failed; exit 101. | Configuration/Repository ID, version, digest and Project substitutions rejected, including matching foreign-owned selected resources; the 2-test module passed. |
| `replacement_keeps` | Missing public `replace` method, E0599; exit 101. | Stable ID/Project, version 2, new target/settings and unchanged prior snapshot; versions 1 and 3 rejected; the 3-test module passed. |
| `inputs_default` | Missing `enabled` field failed decoding. After adding its default, activating a disabled target was still accepted. Both runs exited 101. | Missing flag becomes false, optional Reporter remains absent, and activation of a disabled target fails; the 4-test module passed. |
| `source_selection` | A definition was accepted without an installed Source; `unwrap_err()` failed; exit 101. | Missing Source, wrong Project/role and installation/capability/configuration/resource substitution rejected with either enabled value; the 5-test module passed. |
| `reporting_is_optional` | A selected Reporter without an installed declaration was accepted; `unwrap_err()` failed; exit 101. | Source-only activation and independent Reporter selection succeed; missing/wrong-role/foreign/drifted/incompatible Reporter selections fail; the 6-test module passed. |

After each implementation slice the accumulated module was rerun:

```shell
cargo test --locked --jobs 2 -p octacity-server-factory source_input_tests -- --test-threads=2
# Final result: 6 passed; 0 failed; 0 ignored.
```

The first Green used its specific `input_publication` filter. Subsequent Green
runs used the accumulated module filter above, observing 2, 3, 4, 5 and 6
passing tests respectively. None of the Red outcomes was an environment failure.

## Regression and quality verification

```shell
cargo test --locked --jobs 2 -p octacity-server-factory configuration_tests -- --test-threads=2
# Before implementation: 22 passed; 0 failed; 0 ignored.

cargo test --locked --jobs 2 -p octacity-server-factory --lib -- --test-threads=2
# 286 passed; 0 failed; 0 ignored.

cargo test --locked --jobs 2 -p octacity-server-factory --all-features --lib -- --test-threads=2
# After the final Clippy correction: 286 passed; 0 failed; 0 ignored.

cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
# 10 passed; 0 failed; 0 ignored.

cargo test --locked --jobs 2 -p octacity-server-application --lib factory_flow_action_tests -- --test-threads=2
# 15 passed; 0 failed; 0 ignored.

cargo clippy --locked --jobs 2 -p octacity-server-factory --all-targets --all-features -- -D warnings
# Exit 0 after collapsing the Reporter condition as requested by Clippy.

cargo fmt --all -- --check
# Exit 0.

python3 tools/check_architecture.py
# Exit 0: 61 workspace packages, 245 dependency edges.

openspec validate add-github-work-source --strict
# Change is valid.

git diff --check
# Exit 0.
```

The final selected regressions contain 311 passing tests: 286 Factory core,
10 manual admission and 15 action/status/outbox contracts. The additional
all-features core run verifies the same 286 tests, not another 286 distinct
contracts. No full application/workspace run was needed for this model slice.

## Scope and security review

The implementation is in the existing Factory core owner, with the existing
identifier/positive-version value conventions and no new dependencies. The
Flow interpreter, existing Factory Configuration wire shape, manual admission,
reporting worker and provider process implementations were not modified.

Untrusted definitions cannot choose a different Project or substitute selected
immutable references. Static field errors do not echo provider data. Capability
declarations are separate trusted inputs, cannot be supplied in the draft,
and do not contain credential material. All persisted fields are bounded IDs,
references and flags; there are no URLs, ticket bodies, tokens or raw provider
configuration objects in the binding. No new PII category, credential access,
external effect, migration, endpoint or dependency was introduced. The
architecture documentation describes the model and its integration limits.

Typed selectors remain task 1.3; exclusive groups remain 1.4. Durable
publication, replay identity and input ownership remain tasks 2.1–2.3. Provider
configuration normalization/validation and wire/host/credential enforcement
remain their adapter tasks. REST/UI publication remains section 5. Model
conformance with open provider identities is not a live GitLab/Jira/GitHub
integration qualification, and no source worker was activated.
