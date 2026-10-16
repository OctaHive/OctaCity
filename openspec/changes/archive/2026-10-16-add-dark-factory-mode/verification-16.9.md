# Task 16.9: bounded integration follow-ups

Status: complete for this slice's documented exclusion and extension-boundary
requirements. No concrete ticket poller/webhook, GitHub/GitLab/Jira Work Source
or Reporter, or Octa-local JEV plugin is implemented or release-qualified here.

[Factory integration follow-ups](../../../../docs/architecture/factory-extension-boundaries.md)
records candidate discovery, scoped admission/replay, arbitrary configured
status/label/resolution projection, shared GitHub plugin capabilities with
separate contracts/credentials, and a local task's Job-only authority. Its
acceptance gates explicitly require separately retained provider/task pilots.
The server architecture now distinguishes those follow-ups from installed
Delivery and the existing server-side JEV adapter.

The documentation maps to existing public application contracts, rerun without
changing execution behavior:

```shell
cargo test --locked --jobs 2 -p octacity-server-application --lib \
  factory_admission_tests -- --test-threads=2
# 8 passed: exact admission, replay, ownership, source resolution and policy.

cargo test --locked --jobs 2 -p octacity-server-application --lib \
  factory_flow_action_tests -- --test-threads=2
# 15 passed: arbitrary configured statuses, original operation identities,
# bounded permissions/deadlines, independent outbox recovery and no reopening.

cargo test --locked --jobs 2 -p octacity-server-application --lib \
  factory_decision_signal_tests -- --test-threads=2
# 17 passed: declared routing/tool-risk consumption, deterministic baselines,
# immutable provider subjects and fail-closed provider observations.
```

Logs and command/exit receipts are under `/tmp/octacity-166-evidence/`:
`factory-source-admission-contracts`, `factory-extension-boundaries`, and
`factory-signal-boundaries`. They verify current shared seams, not a provider
adapter that is intentionally absent. Provider polling, webhook signatures,
native label/status writes and local-plugin isolation retain their own future
Red/Green and real-boundary qualification gates. TDD is not applicable to these
documentation-only changes; no executable behavior, deployment flag or
configuration was added.
