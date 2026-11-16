# Tasks 10.9–10.10: Signal authoring review corrections

## Changes

The installed routing binding now owns its node `input_schema` and each output's
canonical `schema`. Node input is distinct from `plan.state_schema`, which
validates the explicitly projected provider state. Those declarations participate
in the complete action pin. The composition root derives public routing metadata,
including input/results/action/permissions, from that same runtime binding.
A private Project reference cannot declare another `metadata.decision_signal`
contract. No old-format fallback or parallel schema declaration is added.

Authoring responses carry the exact policy reference independently of its
adapter. Console labels use the policy version. Reapplying the same canonical
outcome schema preserves its Issue presentation and public-result disclosure;
a different result contract requires fresh review of those settings.

The CI native fixture now declares schemas on its installed routing binding and
contains no copied authoring contract. Existing routing/shadow fixtures use the
same declaration. This updates the current development format directly.

## Observed Red / Green

| Command | Red | Green |
| --- | --- | --- |
| `cargo test --locked -p octacity-server --lib installed_routing_contract_follows_the_pinned_runtime_binding_without_copied_metadata` | The file boundary rejected the newly required runtime-owned schema declaration. | Installation derives the contract without copied metadata; changing input yields the current pinned action and input while retaining the declared result schema. |
| Same command after extending the duplicate-source assertion | A copied `metadata.decision_signal` declaration was accepted. | The private file loader rejects that second source. |
| `cd ui && pnpm exec vitest run src/features/factory/SignalStep.test.tsx` | Applying the same contract discarded `issue_presentation` and `public_result`. | Both settings survive reapplication. A characterization case also preserves the existing fresh-review behavior for a different result contract. |
| `cargo test --locked -p octacity-server-application --lib authoring_discovers_installed_signal_contract_without_a_published_flow` | The policy reference was absent (`null`). | The result carries the exact policy reference even when policy and adapter versions differ; hidden-adapter filtering remains covered. |
| `cd ui && pnpm exec vitest run src/features/factory/SignalStep.test.tsx` | Policy `policy-v2` appeared as adapter `adapter-v1`. | The selector shows `policy-v2`. |

Rust commands used `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0`. A missing finite-provider network-footprint
declaration during fixture setup was corrected; that setup failure is not
counted as Red. TypeScript also caught a missing required `subflow: null` in the
new UI fixture; the production Console build passed after correcting the fixture.

## Initial review-correction verification

- Signal application regressions: **31 passed**.
- Factory Console regressions: **510 passed**, including the three new Signal
  component cases and the existing full-editor capability/contract wiring case.
- Management OpenAPI and generated Console clients regenerated with
  `cd ui && pnpm api:generate`.
- `cd ui && pnpm build`: passed, including TypeScript and bundle architecture.
- Console lint and repository architecture checks passed.
- Complete Server library suite: **239 passed**, 72 explicitly ignored cases.
- Management REST handler suite: **296 passed**, including OpenAPI drift.
- `cd ui && pnpm typecheck` / `pnpm format`: passed after the fixture correction.
- `cargo clippy --locked -p octacity-server-application -p octacity-server -p octacity-server-api-rest -p octacity-release-harness --all-features --lib --tests -- -D warnings`: passed.
- Rust formatting, strict OpenSpec validation, relative documentation links and
  `git diff --check`: passed.

Additional regression commands:

```sh
cargo test --locked -p octacity-server-application --lib factory_decision_signal
cargo test --locked -p octacity-server --lib
cargo test --locked -p octacity-server-api-rest --test v1_handlers
python3 tools/check_architecture.py
# From ui/:
pnpm exec vitest run src/features/factory
pnpm typecheck
pnpm lint
pnpm format
pnpm build
```

These are corrections to the existing tasks, not another pipeline or qualification
system. The native CI run and the single ordinary-stand draft PR recorded in
[10.9](verification-10.9.md) and [10.10](verification-10.10.md) precede these
corrections. They remain historical evidence; this follow-up does not claim a
new native run or publish another PR. Documentation-only changes are checked by
OpenSpec validation, relative-link and whitespace checks; executable TDD does not
apply to prose.

## Retained-binding ambiguity and open-result follow-up

The installer now offers a routing contract only for a unique binding matching
both Project and exact policy reference. Multiple retained bindings remove only
the new-authoring descriptor; executor registration is unchanged. This uses the
existing capability response and adds no binding registry or selection protocol.

The selected step's existing `ResultDraft` now disables applying a Signal policy
until **Save result** or **Cancel result edit** resolves that form. Its edits stay
in component memory. Other steps do not inherit this flag. No draft is silently
discarded or applied to another outcome by its old array index.

Observed Red/Green:

| Command | Red | Green |
| --- | --- | --- |
| `cargo test --locked -p octacity-server --lib routing_` | The new ambiguity regression received an authoring descriptor despite two retained bindings. Existing installation cases passed. | Both binding orders hide the ambiguous descriptor, keep the executor registered, and retain the ordinary unique-binding path. |
| `cd ui && pnpm exec vitest run src/features/factory/FlowPrimitives.test.tsx -t 'keeps an open result draft'` | Both save/cancel cases observed an enabled Apply button while the old result form remained open. | Applying is disabled and edits remain intact; after either resolution, the replacement policy applies with its own canonical result and no old form remains. |

Latest related checks:

- `cargo test --locked -p octacity-server --lib`: **240 passed**, 72 explicitly
  ignored integration cases.
- `cd ui && pnpm exec vitest run src/features/factory`: **512 passed**. The two
  open-result cases also passed in a focused rerun after correcting unsupported
  test query options caught by TypeScript; that fixture type failure is not Red.
- `cargo clippy --locked -p octacity-server --all-features --lib --tests -- -D warnings`: passed.
- `cd ui && pnpm build`: passed, including TypeScript and bundle budgets.
- Console lint/format, Rust formatting, architecture, strict OpenSpec validation,
  documentation file links and `git diff --check`: passed.

The REST DTOs and persistence contracts are unchanged by this follow-up; their
previous regression results above were not rerun. These follow-ups do not repeat
the historical native/PR qualification.

## Pending input normalization follow-up

The existing `Accepted results` pending flag now reaches the Signal policy
selector through the graph editor. **Use Decision Signal policy** is disabled
while **Use selected field** awaits schema normalization. Both the button and its
handler honor that flag; EN/RU status text explains the wait. Completion of the
existing request restores the button. Applying the policy afterwards uses its
installed input schema. This adds no async request manager or second draft state.

The regression uses the public `FlowGraphEditor` UI and a delayed API response:
add a Signal step, select a preceding step's result, start connecting it, and
attempt to apply the policy before resolving `normalizeSchema`. It verifies that
the input is not replaced while connecting, the normalized field and mapping are
retained after the reply, and the selected policy can then be applied with its
own exact action, input and outcomes.

Observed Red/Green command, from `ui/`:

```sh
pnpm exec vitest run src/features/factory/FlowPrimitives.test.tsx -t 'waits for accepted-field normalization'
```

- **Red:** the Apply button remained enabled during the delayed request; the new
  behavioral assertion failed (`disabled: false` instead of `true`).
- **Green:** the delayed-response case passed after the guard was connected.

The operator guides now describe waiting for input connection before applying a
policy. Documentation-only edits have no executable behavior to drive with TDD;
they are validated through link, OpenSpec and whitespace checks.

Latest related checks for this UI-only follow-up:

- `cd ui && pnpm exec vitest run src/features/factory`: **513 passed** across
  64 files, including the delayed-response case and prior result-draft guards.
- `cd ui && pnpm build`: passed, including TypeScript and bundle budgets.
- `cd ui && pnpm lint` and `pnpm format`: passed.
- `openspec validate add-issue-classification-snapshots --strict`: passed.
- Relative links in both updated operator guides and this report, and
  `git diff --check`: passed.

Server code and contracts were unchanged in this follow-up, so server suites and
the historical native/PR qualification were not repeated. No commit or remote
publication was made.
