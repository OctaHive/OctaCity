## 1. Declare summaries in Flow outcomes

- [x] 1.1 Add the optional outcome presentation contract and its publication validation. Through Flow publication tests verify legacy round trips, compatible built-in and custom bindings, exclusive producers for one slot, and rejection of incompatible or ambiguous bindings.
- [x] 1.2 Bind accepted triage results to the presentation contract, including a bounded conclusion and available assessment fields. Verify eligible and needs-review branches both produce correct summaries and that the original accepted records retain their provenance.
- [x] 1.3 Bind specification and review outcomes through the same contract using retained resource references. Verify accepted document and review references through ordinary published Flow fixtures, including one custom outcome with a short conclusion.

## 2. Read current summaries for an Issue

- [ ] 2.1 Resolve bounded, current cards from the exact Issue association and accepted outcomes. At the Issue query boundary verify per-slot coexistence, retries and out-of-order completion, requirement changes, and legacy Issues or Runs without a configured summary.
- [ ] 2.2 Resolve document, artifact and review links through existing read owners. Verify that cross-Project, revoked and hidden references reveal no link, title or count, and that visible expired references have an unavailable state.
- [ ] 2.3 Expose the summary query through REST and regenerate OpenAPI clients. Verify authorization, exact Issue and Run references, bounded response, and the absence of mutations or provider calls on read.

## 3. Present and configure the cards

- [ ] 3.1 Add **Show on Issue** to the outcome editor and publication review. Verify that a standard result needs one display choice and that a custom result supports its title and brief conclusion.
- [ ] 3.2 Add the **Outcomes** section to the Issue Overview. In browser tests verify the triage, specification, review and custom card presentations, meaningful links, unavailable states, English and Russian labels, and keyboard operation at narrow widths.

## 4. Apply local triage classification when enabled

- [ ] 4.1 Add a guarded local classification action over the Issue planning authority. Verify atomic priority and size-label updates, preservation of unrelated labels, unknown values, retired labels, absent write grants, operator edits, late actions, exact replay and unchanged Work scheduling.
- [ ] 4.2 Add the optional classification switch to authoring and show its current Project choices in publication review. Verify that the switch defaults to off, a Project mapping can be reused, and an enabled write with a missing mapping reports the setup needed.
- [ ] 4.3 Project the action receipt into the summary and distinguish recommendation, application and conflict. Verify through REST and the Overview that the Issue's current fields reflect only committed local changes.

## 5. Verify the complete local workflow

- [ ] 5.1 On a local stand, run configured outcomes against one OctaCity Issue and verify the visible conclusions, local priority and size label, specification and review links, and replacement of the triage card on a later accepted result. Confirm the Issue and Run histories retain their original records and the local-only workflow emits no provider intent.
