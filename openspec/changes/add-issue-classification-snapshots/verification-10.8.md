# Task 10.8: configured template forms and ordinary snapshot inputs

## Implemented scope

Preset discovery exposes the configured default field selections for ordinary
nodes in `editor.issue_fields`; the Triage example is visible in Console. The
existing template form reuses **Fill Issue fields**. One canonical selection,
including required/optional flags, goes to the resolver, which generates both
the accepted result and save declaration. Console neither copies the result
schema nor asks for value keys or result-to-Issue mappings. A changed Project
catalog or selection fences an in-flight resolution and retains the draft.

Consumer template inputs select direct built-in or Project properties using the
existing `FactoryFlowIssueFieldSelection`. Overrides are limited to configured
input aliases. Admission uses required finite choices without a semantic role
prerequisite; scheduling priority still uses the explicit Priority source.
Reading fields does not require an installed writer or a producer Run.

Configured condition metadata identifies an input alias. **Starting choice**
uses the currently selected field's labels/order and resolves through the common
choice-condition compiler, including non-Size properties. Changing a source
clears its previous threshold even when both fields contain identical choice
IDs. Published gates retain concrete values rather than reading live catalogs.

The ordinary snapshot editor now exposes its existing bounded evidence policy:
field, alias, required/optional, maximum references and optional exact trusted
environment/criterion/finding. Reopen preserves the policy and step projections;
renaming an input updates the evidence reference, and renaming evidence preserves
its projections. Reference constraints come from generated REST contracts.
Scalar/model claims remain separate from trusted proof. Mixed bindings and
exact-reference material contracts retain their existing explicit handling.

Template access requests use a Step-bound profile's selected execution mode,
plus the configured controller writer capability. They do not request other
profile modes or mutate the Agent Profile. No editor framework, provider
adapter, new endpoint or compatibility format was added. The readonly field type
view is shared independently of the write selector.

## Observed Red/Green

Cargo commands run from the repository root with `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, and `CARGO_PROFILE_TEST_DEBUG=0`. UI commands run from
`ui/`.

| Command / public boundary | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test --locked -p octacity-server-api-rest --test v1_handlers triage_example_discovery_exposes -- --nocapture` | Triage discovery returned no editor/form metadata. | Current configured selections and the optional form parameter match the published OpenAPI contract. |
| `pnpm exec vitest run src/features/factory/FactoryExampleFields.test.tsx` | The template had no Size selector; a late catalog response subsequently still opened an obsolete draft. | One common selector sends Size plus optional custom Impact without normalization/manual schema calls; catalog refresh retains selection and rejects the late response. |
| Same template suite, Step-bound mode | Execution requested `unselected-mode` from the shared profile. | It requests only `selected-mode`, plus the separate local writer capability. |
| `cargo test --locked -p octacity-server-application --lib delivery_template_uses_selected_direct_fields -- --nocapture` | The snapshot request rejected the new `fields` selection. | A catalog without semantic bindings resolves custom effort/risk properties; the gate explicitly includes medium/large from the effort property, independent of the Size catalog. |
| `pnpm exec vitest run src/features/factory/IssueAdmission.test.tsx src/features/factory/FactoryAuthoring.test.tsx` | A direct ordinary property was ineligible for admission; the template lacked its generic selected-field threshold. | EN/RU admission uses direct properties; template requests retain explicit sources and thresholds for both Size and a custom Choice field. |
| `pnpm exec vitest run src/features/factory/SnapshotFieldSelectors.test.tsx` | Switching between fields with the same choice IDs retained the old threshold. | Switching the source requires an explicit new threshold choice. |
| `pnpm exec vitest run src/features/factory/IssueSnapshotEditing.test.tsx -t 'reopens evidence'` | Retained evidence blocked the entire form and had no editable maximum; after introducing the editor, input rename left its evidence reference invalid. | Exact observation settings, digest projection and bounds reopen unchanged; renaming input/evidence retains the reference and consumer projection. An unchanged save reuses its schema. |

Fixture/setup corrections (a missing brace, control labels, root-level pnpm
invocations, and a nullable test request access caught by TypeScript) are not
counted as behavioral Red. An obsolete UI test offering a semantic-slot source
was replaced by the direct-property admission/reopen scenario.

## Final checks

- `pnpm api:generate`: current API, authoring descriptors and client regenerated.
- `cargo test --locked -p octacity-server-application --lib factory_presets`:
  **28 passed**.
- `cargo test --locked -p octacity-server-api-rest --test v1_handlers`:
  **296 passed**.
- `cargo clippy --locked -p octacity-server-application -p octacity-server-api-rest --lib -- -D warnings`
  and `cargo clippy --locked -p octacity-server-api-rest --test v1_handlers -- -D warnings`:
  passed.
- `cargo fmt --all --check`: passed.
- `pnpm test:unit`: **47 script checks and 1,262 tests in 146 files passed**.
- `pnpm exec playwright test tests/browser/factory-canvas.spec.ts tests/browser/factory-ticket-delivery.spec.ts`:
  **22 passed**. The existing EN/RU keyboard snapshot journey now creates evidence
  with exact trusted references, blocks incomplete references, reopens it and
  verifies exact projected settings. Both 390px screenshots were visually
  inspected; horizontal overflow is asserted.
- `pnpm build`, `pnpm lint`, `pnpm format`: passed, including TypeScript,
  production architecture and bundle budgets.
- `python3 tools/check_architecture.py`, strict OpenSpec validation and
  `git diff --check`: passed.

These are application/HTTP contracts and finite browser fixtures. They do not
claim a new native agent execution or confirmed PR. The unrelated CI scenario
and final ordinary-stand qualification remain tasks 10.9 and 10.10.

## Review corrections: source policies and untouched template defaults

Changing a template input's direct source now clears only admission conversions
and allowed values that refer to that alias. Other inputs' conversions and
thresholds stay intact. Changing required/optional flags alone retains the source
policy. Source comparison uses canonical identities, so equal choice IDs or
different display labels cannot transfer a policy to another field.

Configured write defaults are initialized once when the successful authoring
contract is first displayed. They retain the original catalog, field selections
and writer/policy even if no checkbox was edited. Refreshes reuse the ordinary
stale-form message and **Use current field definitions** action. In-flight
responses cannot open an obsolete draft; after explicit reconciliation, a fresh
request uses the reviewed version and opens normally. These corrections add no
new editor, endpoint, schema copy or reconciliation workflow.

Observed TDD (commands run from `ui/`):

| Command | Red before implementation | Green |
| --- | --- | --- |
| `pnpm exec vitest run src/features/factory/FactoryAuthoring.test.tsx -t 'replace risk=true'` | Switching risk from Impact to another property with identical `major`/`minor` IDs left **Open example draft** enabled with the old admission policy. | Opening is blocked, old risk conversions/allowed values are cleared, priority and work-kind settings remain; explicitly entered replacement mappings are sent with the new source and the retained threshold. |
| `pnpm exec vitest run src/features/factory/FactoryExampleFields.test.tsx -t 'default'` | Both untouched-default scenarios silently adopted a catalog/writer refresh without the stale-selection message. | Original version/selection remain visible, opening stays blocked and late responses are rejected. Explicit reconciliation followed by a fresh request succeeds for catalog and writer changes. |

Final verification after these corrections:

- Focused template, authoring, threshold and admission suites: **16 passed**.
- `pnpm test:unit`: **47 script checks and 1,265 tests in 146 files passed**.
- `pnpm exec playwright test tests/browser/factory-ticket-delivery.spec.ts`:
  **2 passed** (reviewed example and publication/manual Issue start).
- `pnpm build`, `pnpm typecheck`, `pnpm lint`, `pnpm format`: passed. A missing
  readonly tuple annotation in the test was corrected after TypeScript reported
  a possibly undefined option label; that compiler failure is not counted as Red.
- Architecture, strict OpenSpec validation and `git diff --check`: passed.

The retained tests extend existing Console boundaries. Rust behavior and wire
contracts were unchanged, so the earlier server checks were not repeated.

## Review correction: accepted code candidate references on outcome rename

The ordinary agent result rename now updates downstream
`code.revision.accepted_candidate` references only when both the producer node
and previous outcome match. The source, access and scope are retained. References
to another outcome or producer are unchanged. This extends the existing rename
operation; no new reference abstraction or compatibility branch was introduced.

TDD command (from `ui/`):
`pnpm exec vitest run src/features/factory/FlowGraphEditor.test.tsx -t 'retains accepted code candidate'`.
Red: after renaming `implementation.accepted` to `implemented` through the result
editor, the consumer still referenced `accepted`. Green: its exact binding uses
`implemented`, while both unrelated bindings stay unchanged and the control
transition follows the renamed result without schema regeneration.

Checks after the fix:

- `pnpm exec vitest run src/features/factory/FlowGraphEditor.test.tsx`:
  **27 passed**.
- `pnpm test:unit`: **47 script checks and 1,266 tests in 146 files passed**.
- `pnpm exec playwright test tests/browser/factory-canvas.spec.ts --grep 'fills Issue fields'`:
  **2 passed**, covering EN/RU step/result rename and retained field settings.
- `pnpm build`, `pnpm lint`, `pnpm format`, architecture checks, strict OpenSpec
  validation and `git diff --check`: passed. TypeScript first caught an unsupported
  Testing Library `exact` option in the new test; it was replaced with an anchored
  accessible-name matcher. This test setup correction is not counted as Red.

The Russian guide now names the configured input alias `risk`, matching the
template and Console, and both authoring guides document candidate-link
preservation. The alias correction is documentation-only and has no executable
behavior requiring a TDD cycle. Existing `admission_risk` domain metadata is a
separate concept and was not renamed.
