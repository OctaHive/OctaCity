# OctaCity protocols

This directory contains language-neutral specifications for contracts that
cross process or repository boundaries. Rust API documentation remains next to
the implementing crate; the documents here define wire behavior and trust
semantics independently of a particular implementation language.

| Protocol | Status | Specification | Implementation |
| --- | --- | --- | --- |
| Signed JobSpec v1 | Implemented | [signed-job-spec-v1.md](signed-job-spec-v1.md) | `octacity-protocol` |
| Source-plugin v1 | Implemented | [source-plugin README](../../crates/octacity-source-plugin/README.md) | `octacity-source-plugin`, `octacity-source`, `octacity-source-git` |
| Octa runner v1 | Implemented in Octa | Published by the `octa-runner-protocol` crate | `octa-runner-protocol`, `octacity-agent` |
| Server-agent transport v1 | Phase 5 implemented | [server-agent-v1.md](server-agent-v1.md) | `octacity-protocol`, `octacity-coordinator`, `octacity-lifecycle` |

“Implemented” means the wire types and validation exist. The OctaCity server
is not available yet; the agent daemon implements the operations listed in the
server-agent specification.
