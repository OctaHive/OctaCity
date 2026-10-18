# Integration management contracts

The live `/api/v1/openapi.json` document is the source for the console's generated
`components`, `paths` and `operations` types. Connection, Project access, credentials,
connection checks, automatic input selection and Source diagnostics have separate
contracts. Provider names and native task containers are open identities; a task
container does not select the code Repository or an OctaCity Project.

Canonical synthetic examples live in
[`integration-contracts.json`](../../server/api/octacity-server-api-rest/fixtures/v1/integration-contracts.json).
They are published as OpenAPI component examples, validated against that document,
checked against typed application intents and reused by console transport tests.
Negative fixture checks reject invalid nullable credential types, oversized UTF-8
status values and over-limit preview arrays, while accepting their valid boundaries.
The credential example contains only a documentation placeholder. Ordinary responses
contain configured state and safe references, never the submitted value. The secret
submission's `writeOnly` field has a UTF-8 byte ceiling in addition to `maxLength`.

Initial connection/access/administrative publication uses `If-None-Match: *`;
replacement uses `If-Match` with the expected metadata version or administrative
revision. Mutations also require an `Idempotency-Key`. The individual precondition
headers are optional in OpenAPI because the required choice depends on the intent.
Checks, validation and preview need neither mutation header. Integration operations
require verified protected ingress; setting a browser header does not supply that proof.

`FactoryWorkStatusActionParameters` describes the existing installed `set_status`
action: an arbitrary `status` plus an optional finite `outbox` retry policy. The
original input's immutable Reporter configuration resolves the status to permitted
provider effects. An ordinary `trusted_action` node can use these parameters on a
stage outcome route, including an intermediate or nested stage. The generic
`FactoryFlowAction.parameters` envelope remains open for other installed plugins.
This contract does not add label-effect fields or a new source-update endpoint;
guided per-outcome effect editing belongs to task 5.13.

Source diagnostics retain bounded native correlation separately from input-scoped
Work/Run identities. An `issue` link is optional and supplied only by the authorized
association capability. The fixtures include absent and visible associations; no
card, Issue CRUD, explicit card execution or field-ownership API is required.
[Manual Artifact admission](management-rest-v1.md) continues independently of a
connection, token, automatic input or internal Issue.

Regenerate the ignored local client files using the repository's locked exporter:

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2 pnpm --dir ui api:generate
pnpm --dir ui typecheck
pnpm --dir ui exec vitest run src/api/integrationContracts.test.ts src/api/client.test.ts src/features/factory/api.test.ts
```

CI regenerates from the live Rust document before console type checking. Do not
hand-edit `.generated/api/`, maintain a second schema snapshot, or introduce
handwritten provider DTOs alongside these generated contracts.
