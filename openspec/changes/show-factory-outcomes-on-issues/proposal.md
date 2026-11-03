## Why

People open an Issue to learn what came out of its work. Today they must select an execution and inspect its results or reports. Triage, specification preparation and review need a short, current summary with direct links to the work.

## What Changes

- Add **Show on Issue** to supported Flow step outcomes. Built-in result types supply the card title and field labels. A custom step can supply a title and a short summary through the same authoring control.
- Show one current card per configured result slot in the Issue Overview. Each card uses a brief conclusion from the accepted outcome, selected facts, links to available work and a link to its original Run.
- Support triage, specification preparation and review through the same mechanism. Keep full results and history available through the existing execution views.
- Offer an optional **Apply classification to Issue** setting for triage. Apply the accepted priority and configured size label through existing local Issue rules and Project choices. Both display and updates operate within OctaCity.

## Capabilities

### New Capabilities

### Modified Capabilities

- `server/dark-factory` — Retain a bounded Issue presentation contract on declared Flow outcomes.
- `server/factory-work-collaboration` — Resolve current Issue cards and apply explicitly enabled local classification changes.
- `server/rest-control-plane` — Expose bounded, authorized Issue summaries and their original execution references.
- `ui/operator-console` — Author Issue summaries with a single display setting and show the resulting cards on the Overview.

## Impact

Factory Flow authoring and accepted outcomes, Issue authorization and planning revisions, retained document and review links, generated REST clients and the Issue Overview. Existing Flow publications opt in through a new version. The triage assessment continues to use its existing validation and immutable scheduling contracts.
