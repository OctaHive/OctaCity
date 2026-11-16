# Task 10.7: ordinary agents fill selected Issue fields

## Implemented scope

An ordinary AI agent's successful result offers **Fill Issue fields**, off by
default. A bounded Project selector shows supported core/custom field labels,
types, choices and bounds. Fields are required by default; the existing checkbox
pattern makes individual fields optional. The existing normalization query
receives only the selected `issue_fields` form, not a copied shape or mapping.
The same form becomes the outcome's automatic guarded save declaration.

Reopening retains identities/requiredness. Unchanged saves reuse the original
schema. Catalog/writer changes retain the draft and require explicit review;
unavailable fields require explicit removal, with no substitution or widening.
Late normalization responses cannot commit a replaced draft or contract.
Disabling filling removes the write declaration and unused node-local writer
grant, preserving unrelated grants. The common access review requests the local
controller capability without modifying the Agent Profile.

Publication reviews the exact step/outcome, field types and save scope.
**Show on Issue** remains an independent display option; a card incompatible
with the generated form produces a local actionable problem and retains the
draft. Agent outcome renames preserve control/projection/record references and
the installed compiler's selected result. Ordinary free-form results remain
non-writing.

Removed the superseded manual apply-node editors and Project semantic-role
controls, rather than retaining legacy modes. Their obsolete UI tests were
replaced by selected-form tests; unrelated input/snapshot behavior remains
covered. The unchanged optional-assessment projection helper now has its own
small module. No new framework, endpoint, schema validator or writer was added.

## Observed Red/Green

Commands below run from `ui/`.

| Command / public boundary | Red observed before the correction | Green |
| --- | --- | --- |
| `pnpm exec vitest run src/features/factory/FillIssueFields.test.tsx` | Ordinary result had no Fill Issue fields control. | One selection generates the form/save scope for core Size plus custom choice, text, boolean, integer and date; required/optional values reopen. |
| Same file plus `src/features/issues/ProjectIssueFields.test.tsx` | Trusted-action editor still exposed manual apply; Project settings still exposed fixed role selectors. | Both obsolete control paths are absent; ordinary Project scalar authoring/publication remains covered. |
| `pnpm exec vitest run src/features/factory/AccessRequirements.test.tsx` | Automatic writer was omitted from the proposed access; review could not apply the required local grant. | Review supplies node/Flow/admission controller permissions without adding the writer to the Agent Profile. |
| Fill Issue fields suite: pending normalization | A catalog replacement still committed the stale response and closed the draft; a writer replacement offered no reconciliation. | Both retain selected identities and reject the late response, exposing explicit reconciliation. |
| Same suite: unchanged reopen/save | Normalization ran twice and generated a new schema identity. | Unchanged selection uses the exact existing reference. |
| Same suite: optional downgrade | A field not supporting required values could still save as required, with its Required control disabled. | Required saving is blocked; an explicit optional downgrade retains the selected identity. |
| Same suite: disable filling | The result lost its write declaration but the node retained the local writer grant. | The unused local writer is removed while unrelated permissions remain. |
| Same suite: independent display | An incompatible existing Issue card was not reported against the generated form. | A finite local validation message preserves the draft; display settings require review. |
| `pnpm exec vitest run src/features/factory/FactoryPublication.test.tsx` | The exact step/outcome heading was missing from field-save review. | Review identifies the successful producer, selected fields, types and requiredness. |
| `pnpm exec playwright test tests/browser/factory-canvas.spec.ts -g 'fills Issue fields'` | Renaming an agent outcome left the old control route, visibly blocking validation. | EN/RU keyboard journeys preserve selection across step/outcome rename and navigation; compiler result and route point to the renamed outcome. |

Fixture/setup errors were corrected before counting behavioral Red: the first
publication fixture lacked an executor, a browser executor lacked its typed input,
a test used an unavailable translation key, and an early card fixture had a syntax
error. Incorrect root-level pnpm invocations are not counted as Red.

## Final checks

- `pnpm test:unit`: 47 script checks and **1,258 tests in 144 files passed**.
- `pnpm exec playwright test tests/browser/factory-canvas.spec.ts tests/browser/factory-ticket-delivery.spec.ts`:
  **22 passed**, including new EN/RU keyboard journeys at 390px and the existing
  reviewed publication/start case. Screenshots of the selector in both languages
  were visually inspected; horizontal overflow is asserted.
- `pnpm build`: TypeScript, production build, frontend architecture and bundle
  budgets passed.
- `pnpm lint` and `pnpm format`: passed.
- The publication fixture was subsequently tightened to put the write on a
  successful outcome; its complete 27-test file passed again.
- Repository architecture (64 packages / 264 edges), strict OpenSpec validation
  and `git diff --check`: passed.

## Documentation and qualification boundaries

The general authoring guide, Issue reference and saved Russian triage/Delivery
instructions describe the ordinary selector, explicit reconciliation, required
access and automatic application without a manual apply step. Documentation
edits have no executable behavior and do not need an executable Red/Green cycle.

This is task 10.7 only. Generic template forms and snapshot/condition controls
remain task 10.8; the unrelated CI/Decision Signal UI case is 10.9; live ordinary
stand/model/PR qualification is 10.10. The browser tests use finite management
peers and claim no new live provider, real model execution or PR publication.
Rust/API/storage code and wire schemas were unchanged, so their already-recorded
qualification was not rerun for this UI-only slice. No commit was requested.
