# OctaCity documentation

This directory contains the durable contracts, operational guides, and design
records for OctaCity. The project [README](../README.md) is the product entry
point; use this index when you need implementation or deployment detail.

## Architecture

Documents that explain ownership, trust boundaries, and extension points:

- [Server architecture and ownership](architecture/server.md)
- [Server extension seams](architecture/extension-seams.md)
- [Server and Agent threat model](architecture/threat-model.md)

## Operations

Runbooks for deploying and maintaining released components:

- [Server operations](operations/server.md)
- [Agent operations](operations/agent.md)
- [Backup, restore, and reconciliation](operations/backup-restore.md)
- [Schema migrations and rollback](operations/schema-migrations.md)

## Reference

Stable interfaces and examples consumed by operators or implementations:

- [Management REST v1 workflow](reference/management-rest-v1.md)
- [Directory artifact format v1](reference/directory-artifact-format-v1.md)
- [Wire protocol catalogue](reference/protocols/README.md)
- [Configuration and policy examples](reference/examples/)

## Testing and release evidence

Contracts that explain how production claims are verified:

- [Execution-backend contracts](testing/backend-contracts.md)
- [Security release gates](testing/security.md)
- [Vault and artifact integration contract](testing/phase6-contract.md)

## Planning

Design proposals and implementation history that are useful for future work but
are not operational contracts:

- [Agent implementation plan](planning/agent/README.md)
- [Dark factory architecture proposal](planning/proposals/dark-factory-architecture.md)

## Where new documents belong

- Put system boundaries and lasting design decisions in `architecture/`.
- Put procedures an operator follows in `operations/`.
- Put versioned APIs, wire formats, and examples in `reference/`.
- Put test strategy and retained evidence requirements in `testing/`.
- Put proposals and delivery roadmaps in `planning/`.

Keep the root of `docs/` limited to this index.
