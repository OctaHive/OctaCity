# OctaCity protocols

This directory contains language-neutral specifications for contracts that
cross process or repository boundaries. Rust API documentation remains next to
the implementing crate; the documents here define wire behavior and trust
semantics independently of a particular implementation language.

| Protocol | Status | Specification | Implementation |
| --- | --- | --- | --- |
| Signed JobSpec v1 | Implemented | [signed-job-spec-v1.md](signed-job-spec-v1.md) | `octacity-protocol` |
| Source-plugin v1 | Implemented | [source-plugin README](../../agent/octacity-source-plugin/README.md) | `octacity-source-plugin`, `octacity-source`, `octacity-source-git` |
| Octa runner v3 | Implemented in Octa | Published by the `octa-runner-protocol` crate | `octa-runner-protocol`, `octacity-runner` |
| Server-agent transport v1 | Phases 5–7 implemented | [server-agent-v1.md](server-agent-v1.md) | `octacity-protocol`, `octacity-coordinator`, `octacity-lifecycle` |
| Cache session v1 | Implemented | [cache-session-v1.md](cache-session-v1.md) | `octacity-cache-session`, `octacity-runner` |
| Webhook provider v1 | Wire contract, bounded host, public ingress, and unmanaged and managed lifecycles implemented; production provider adapters intentionally absent | [webhook-provider-v1.md](webhook-provider-v1.md) | `octacity-webhook-provider-protocol`, `octacity-server-webhook`, `octacity-server-api-webhook` |
| VCS provider v1 | Wire contract implemented; host and Git adapter pending | [vcs-provider-v1.md](vcs-provider-v1.md) | `octacity-vcs-protocol` |
| Artifact transfer v1 | Wire contract implemented | [artifact-transfer-v1.md](artifact-transfer-v1.md) | `octacity-protocol` |
| Agent provisioning v1 | Conformance contract only; no production adapter | [agent-provisioning-v1.md](agent-provisioning-v1.md) | `octacity-agent-provisioning-protocol` |

“Implemented” means the wire types and validation exist. A health-only
`octacity-server` composition shell is available, but it intentionally exposes
no management mutations or server-Agent coordination routes yet. The agent
daemon implements the client-side operations listed in the server-Agent
specification.
