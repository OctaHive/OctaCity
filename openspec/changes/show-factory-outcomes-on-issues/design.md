## Context

`FactoryRunDiagnostics` already renders reviewed triage outcomes. The Factory journal retains accepted outcome records with exact Flow version, schema and input provenance. The diagnostic `public_result` flag controls a separate view. Issue results currently start from a selected Work association. The Issue store owns the current revision, bounded priority and Project planning catalog. Its labels have stable identities, and its planning catalog can provide optional type, size and component properties. The active triage preset spec keeps advisory classification and immutable admitted scheduling priority distinct from authoritative Issue planning.

## Goals / Non-Goals

**Goals**

- Use one authoring interaction and one Overview section across built-in and custom results.
- Keep recommendations, applied planning values and historical results distinguishable.
- Resolve links and enforce permissions with the same owners used by existing OctaCity detail views.

**Scope boundaries**

The change covers local presentation and an optional local triage classification write. Execution history, discussion, provider reporting and Issue business transitions continue through their existing contracts.

## Decisions

1. **Add a presentation binding to a declared outcome.** Use the existing immutable Flow publication and schema validation. The binding names a stable slot and a versioned presentation type. Built-in types resolve their known fields from a supported schema. A custom presentation uses a bounded short conclusion and optional typed reference fields. Default custom slot identity is scoped to the stable Flow and outcome so unrelated custom flows can coexist. An explicitly shared slot lets mutually exclusive branches supply the same card. Validate bindings before publication. This keeps one display control on the result the author is already editing.

2. **Use the accepted outcome as the source of card content.** For standard triage, connect the reviewed eligibility or classification outcome as appropriate. Where the existing classification summary lacks short prose, explicitly carry a bounded conclusion through its reviewed result contract. Structured fields drive localized labels and state. Specifications and review use explicit retained document, artifact or review references from their accepted outcomes. Use producer-owned links with independent visibility and retention checks. Render summary text as plain text, and resolve link labels from the referenced resource. The Overview reads the designated fields regardless of the diagnostic `public_result` setting. It never requests the raw report or exposes its payload as card data.

3. **Resolve current cards within the Issue read boundary.** Use the exact Issue–Work–Run association, current requirements binding, immutable outcome binding and accepted journal record. Rank eligible outcomes by admission order and then by accepted occurrence within the Run, with a stable identity as a tie-breaker. A newer Run with no eligible result does not displace an earlier current card. Use a bounded repository query or a journal-backed index if needed to avoid a client-side scan of Run history. Apply current Issue and Run visibility before ranking and apply independent reference visibility before emitting links. The API returns the display fields and source links only. Preserve generic historical result readers for historical and unconfigured Flows.

4. **Separate display from a trusted local planning write.** The second switch enables an explicit accepted action using the same Issue authorization, current requirements checks and catalog authority as manual edits. Use the existing Issue priority vocabulary and resolve size labels against reviewed Project mappings by stable identity. Reuse a Project mapping across pipelines. If it is missing, surface that at authoring when writes are requested. The action compares only relevant planning fields against their applicable baseline and respects later manual or newer automation edits. Remove a prior size label only if the same policy actually owns its addition. Apply the accepted changes in one version-fenced Issue mutation with an operation identity tied to the original accepted result. Reuse the existing durable action receipt and audit mechanisms. The card can describe a successful assignment as of that operation and distinguish a newer current value, if one exists.

5. **Use one card layout.** The Overview shows a title, plain-language conclusion, compact facts and named links. Built-in presenters supply field labels and ordering in English and Russian. Use the existing Issue properties to show the current priority and labels. Keep the card limited to the outcome and what it produced, with deeper execution detail accessible from the Run link.

## Risks / Trade-offs

- An earlier Run can finish after a newer Run. Order current cards by admission and use field preconditions on planning writes.
- A result can outlive a linked artifact or a reader's authorization. Resolve exact references through their existing owners at read time and distinguish expiration from a hidden resource.
- A published mapping can refer to a retired label. Check the current Project catalog when applying it and record a readable failure when its configured effect is no longer applicable.
- A hand-authored Flow can retain an older text-only result. Enable the display binding on a new compatible Flow version and keep the old result accessible in its original execution.

## Migration Plan

Release the optional Flow contract and generated read API before enabling the new authoring controls. Existing publications and Runs continue to decode under their original contracts. Install or publish compatible template versions for triage, specifications and review through ordinary Flow authoring. Validate the first configured version against a local Issue and the existing Run detail views. Roll back a newly configured presentation through the ordinary Flow replacement path. An Issue planning revision already committed remains part of its ordinary audit history.
