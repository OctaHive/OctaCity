# Server adapter protocols

These crates contain strict, versioned, provider-neutral process contracts.
They do not start processes, load credentials, or depend on provider SDKs.
Infrastructure hosts select and supervise adapters that implement them.
Their relationship to core ports and application interfaces is documented in
the [server ownership and seams guide](../../docs/server-architecture.md).

Every protocol provides explicit version negotiation, bounded values,
cooperative cancellation, stable failure classes, and language-neutral JSON
fixtures. Provider payloads are decoded inside adapters and never become these
normalized protocol types.

| Contract | Language-neutral specification | Golden fixture |
| --- | --- | --- |
| Webhook provider v1 | [`docs/protocols/webhook-provider-v1.md`](../../docs/protocols/webhook-provider-v1.md) | [`verify-delivery-v1.json`](octacity-webhook-provider-protocol/fixtures/verify-delivery-v1.json) |
| VCS provider v1 | [`docs/protocols/vcs-provider-v1.md`](../../docs/protocols/vcs-provider-v1.md) | [`fixtures`](octacity-vcs-protocol/fixtures/) for refs, commits, trees, file content, and revision resolution |
| Agent provisioning v1 | [`docs/protocols/agent-provisioning-v1.md`](../../docs/protocols/agent-provisioning-v1.md) | [`provision-v1.json`](octacity-agent-provisioning-protocol/fixtures/provision-v1.json) |

The shared artifact transfer contract is documented in
[`docs/protocols/artifact-transfer-v1.md`](../../docs/protocols/artifact-transfer-v1.md)
and implemented by `shared/octacity-protocol`.
