# Task 7.4 — Operator, headless and stand documentation

## Updated procedures

- [Factory authoring](../../../docs/operations/factory-pipeline-authoring.md#prepare-canonical-issue-inputs)
  now starts with Project field administration and canonical Issue data. It
  distinguishes optional triage application from assessment display, explicit
  material membership from publication/append, and per-step snapshot selection
  from exact producer-result input. Delivery's main walkthrough no longer
  requires a triage Run. It also explains validation/explicit target activation
  and independent Source/Reporter configuration.
- [Console operations](../../../docs/operations/operator-console.md#prepare-an-issue-for-independent-delivery)
  links the complete Issue preparation, review/start and historical snapshot
  journey, including separate administration, Issue and source-body permissions.
- [Headless examples](../../../docs/reference/issues.md#headless-issue-snapshot-sequence)
  cover a fresh Project catalog, manual built-in/custom values, Issue-document
  publication/versioning, ordinary comment append, complete exact material
  selection and reviewed snapshot admission. The selection example includes all
  three owner-aware reference variants. It retains the exact ETag/digest/key on
  retries and explains pending publication versus read-only status polling.
  A separately prepared producer-result request shows its unchanged selector
  and scheduling-classification contract; sample IDs/digests/sizes are explicitly
  illustrative, not fabricated producer/source identities.
- [Stand qualification](../../../docs/operations/local-stand.md#qualify-issue-snapshot-delivery)
  now has an ordered manual checklist for field administration, local action
  installation, independent delivery, Issue versus Project documents, explicit
  comment/material selection, source-owner isolation, per-step projections,
  stale inputs, restart/replay and reusable delivery. It preserves Triage/Delivery
  access ceilings and the shared Standard launch policy.
- [Qualification commands](../../../docs/testing/manual-factory-authoring.md#ticket-delivery-qualification)
  distinguish the observed 7.1–7.2 packaged journeys and 7.3 regressions from
  historical producer-result evidence and the still-pending material journey.
- [Cutover and rollback](../../../docs/reference/factory-pipeline-authoring-migration.md#issue-snapshot-cutover-and-spec-order)
  preserve existing definitions, numeric receipts, exact producer-result inputs,
  retention and admitted snapshots. Definition rollback uses a new Configuration
  selecting the compatible previous Flow; binary/schema rollback still uses a
  matched pre-upgrade consistency-unit restore.
- The [v2 example README](../../../config/factory/presets/ticket-delivery/README.md#current-v2-issue-snapshot-example)
  now distinguishes the generic editor's working simple snapshot controls from
  the example-opening form's unsupported required admission parameters. The
  complete v2 example still uses API resolution/publication; no new UI capability
  is claimed or implemented by this documentation task.

## Spec order and scope

The predecessor `add-factory-ticket-delivery-pipeline` is already archived under
`archive/2026-11-10-add-factory-ticket-delivery-pipeline`, and main delivery specs
contain its accepted-triage/triage-size requirement names. The procedure records
that baseline first, then this change's `RENAMED` requirements and matching
`MODIFIED` definitions, with the related deltas synchronized coherently only
after implementation/qualification. Neither archived planning history nor main
specs are edited here. No publication, stand update, provider write or new native
Job qualification was performed. The complete packaged material journey remains 7.5.

Task 7.3's test-fixture fixes/report are preserved. This task changes
Markdown documentation only. Under the repository's documentation-only exemption,
TDD is not applicable: there is no new executable behavior. The checks below
validate documentation rather than claim behavioral Red/Green or qualification.

## Observed checks

The current compiled REST exporter provides the OpenAPI used for validation:

```sh
target/debug/examples/export_openapi > /tmp/octacity-74/openapi.json
python3 /tmp/octacity-74/check_api_examples.py
python3 /tmp/octacity-74/check_local_links.py
python3 -m unittest tools.tests.test_local_stand_documentation
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

The temporary schema checker uses `jsonschema 4.25.1` / Draft 2020-12 with
format checks, installed under `/tmp/octacity-74/validation-libs`, without
changing repository dependencies. It checks eight bodies against the exported
closed request DTO schemas: catalog publication, Issue creation, Issue-document
publication, comment append, snapshot review, exact producer-result start,
three-kind material selection and snapshot start with the reviewed digest.
All eight pass. These checks establish wire structure, not source resolution,
authorization, field/evidence applicability or actual HTTP execution.

All 122 local Markdown links and heading anchors pass across the seven changed guides
and this report. The first check found one new nonexistent anchor, corrected
before the final successful check. Existing external historical URLs are not
requalified. All three existing stand-documentation tests pass; strict OpenSpec
validation and whitespace checks also pass. Result files are
`/tmp/octacity-74/api-examples.json`, `/tmp/octacity-74/local-links.json` and
`/tmp/octacity-74/documentation-tests.log`. No new runtime/test dependencies,
generated API files or executable scripts are added to the repository.
