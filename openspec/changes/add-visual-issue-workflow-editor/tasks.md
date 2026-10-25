## 1. Graph viewing

- [x] 1.1 Through TDD in the existing workflow component tests, render default/catalog/exact historical definitions as one feature-local canvas with named status nodes, directed transition arrows and initial/terminal markers. Verify a branching return-path example shows only declared edges, including distinct arrows between the same statuses.
- [x] 1.2 Through TDD, add stable-ID node/arrow selection and a right-side read-only properties inspector, shared with the forthcoming editing mode. Verify selection exposes the exact element's settings and sends no mutation.

## 2. Canvas authoring

- [ ] 2.1 Through TDD, use the existing editor draft for empty-canvas creation and node add/delete, names/order and workflow initial/terminal settings in the inspector. Verify no default statuses are copied, removal cleans references, clearing is allowed and empty publication remains blocked; replace the all-rows form rather than retaining a second editor.
- [ ] 2.2 Through TDD, create/delete arrows by pointer connection and equivalent keyboard endpoint controls, with transition name/origin/evidence editing in the same inspector. Verify both input methods produce the same existing workflow DTO and retain generated constraints and stable identities.
- [ ] 2.3 Through TDD at the browser boundary, add node dragging, pan/zoom/fit controls and narrow-width inspector access with visible keyboard focus and English/Russian labels. Verify canvas movement changes neither status order nor the validation/publication body and creates no request or browser-persisted draft.

## 3. Existing publication flow and completion

- [ ] 3.1 Integrate the canvas with the existing validation, confirmed publication and separate default-selection controls; adapt the current workflow tests and one focused browser authoring journey to verify frozen requests and stale-version review. Update workflow guidance, run the relevant workflow tests plus existing UI format/lint/typecheck/build gates, and update only affected visual baselines. No new verification infrastructure or cross-product matrix.
