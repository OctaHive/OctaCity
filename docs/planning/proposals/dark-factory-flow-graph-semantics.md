# Dark Factory Flow Graph Semantics

## Status and scope

This document records the graph semantics observed before implementing Factory
Flow Definitions. It compares three domains without claiming that they should
share an implementation:

- the Octa task DAG at revision
  `e0a3c65fe010220c5a8162c52368b7c8f624ddb5`;
- the existing OctaCity persisted Job DAG;
- the target immutable Factory Flow Definition.

The accompanying provider-neutral corpus is
`server/core/octacity-server-factory/tests/fixtures/flow-graph-conformance-v1.json`.
It defines the structural facts consumed by the private Factory graph module
through its domain adapter. It is not an Octa compatibility suite and does not
change the Job reconciler contract.

## Common structural vocabulary

The corpus uses one normalized edge orientation: `predecessor -> successor`.
An edge means that the predecessor must reach an applicable outcome before the
successor can become eligible. This is a structural convention only. It does
not transfer context, artifacts, credentials, permissions, or execution
authority.

Node identities are opaque, non-empty strings. Canonical ordering is ascending
UTF-8 byte order. Edges are ordered by `(predecessor, successor)`. Strongly
connected component members are ordered by node identity, and components are
ordered by their first member. These rules deliberately avoid locale, provider,
database, and process-runtime behavior.

For a structurally valid graph, the corpus defines:

- reachability as the sorted set reachable from the declared query roots,
  including those roots;
- a cyclic component as a component with more than one node or a singleton
  containing a self-edge;
- canonical topological order as Kahn's algorithm with the smallest ready node
  selected first; it is unavailable when any cycle exists;
- node and edge counts after successful duplicate validation;
- maximum fan-out as the largest number of direct successors;
- maximum depth as the largest number of edges on a path in an acyclic graph,
  with an isolated node at depth zero; it is unavailable for a cyclic graph.

Duplicate nodes, duplicate edges, and missing endpoints are structural errors.
Derived facts are not authoritative for malformed input. Product adapters may
impose stricter identity, entry, terminal, reachability, schema, or budget
rules after structural validation.

## Semantic matrix

| Concern | Octa task DAG (current) | OctaCity Job DAG (current) | Factory Flow Definition (target) |
| --- | --- | --- | --- |
| Purpose and lifetime | Plans and executes one bounded Octafile invocation in process. | Reconciles one persisted Attempt's immutable Jobs from authoritative state. | Defines a versioned macro workflow interpreted durably across restarts and nested flows. |
| Node identity | `T: Eq + Hash + Identifiable`; the node set uses value equality while adjacency uses `id()` strings. | `JobId`, unique within one materialized Attempt graph. | `FlowNodeId`, unique within one immutable Flow Definition version. |
| Stored edge form | Adjacency from parent/predecessor to dependant/successor. | Each child Job stores a sorted list of prerequisite Job IDs. | Canonical control edges use predecessor to successor; context and data projections are separate declared structures. |
| Normalized orientation | Predecessor -> successor. | Predecessor -> successor after reversing the stored child-to-prerequisite relation. | Predecessor outcome -> eligible successor route. |
| Duplicate node | `HashSet` silently deduplicates equal values; an `id()` collision is not independently diagnosed. | Rejected as `DuplicateJob`. | Rejected before publication. |
| Duplicate edge | `HashSet` silently deduplicates it. | Repeated prerequisites are rejected as `DuplicateDependency`. | Rejected before publication. |
| Missing reference | `add_dependency` rejects either endpoint if its node value is absent. | Complete-graph validation rejects an unknown prerequisite. | Definition validation rejects every missing node, route, entry, terminal, or subflow reference. |
| Canonical ordering | No public canonical node, edge, or topological projection; storage uses hash collections. | Nodes use `BTreeMap` order and each prerequisite list is sorted. | Canonical nodes, edges, issues, components, reachability, and acyclic topological projection are required. |
| Reachability | No public reachability fact; planner construction determines the executed graph. | No single-root requirement; disconnected Jobs are valid when otherwise consistent. | Structural reachability is reported; the Factory adapter rejects unreachable required nodes and incomplete terminal paths. |
| Components and cycles | Exposes only a whole-graph cycle boolean through topological elimination. | Validation reports only `CyclicGraph`. | Reports strongly connected components and cyclic components; the adapter validates explicit bounded-repeat semantics. |
| Topological projection | Internal traversal is used for cycle/linearity checks but is not a canonical public order. | Deterministic fixed-point reconciliation is not exposed as a topological projection. | Exposes a canonical order only for an acyclic structural projection. |
| Structural measurements | Exposes node count; no depth, edge-count, or fan-out contract. | No public structural measurement contract. | Reports bounded node count, edge count, maximum depth, and maximum fan-out for admission checks. |
| Cycle policy | Every cycle, including a self-edge, is invalid for planned execution. | Every persisted Job cycle is corrupt and rejected. | Raw cycle facts are neutral; only declared statically bounded logical repeats may pass Factory policy. |
| Execution semantics | Short-lived in-memory planner/executor owns runnable task scheduling. | Durable Job state, dependency policy, cancellation, and reconciliation remain server authority. | Durable Flow Runs, Node Attempts, fences, budgets, and transitions remain Factory application/store authority. Realized history is append-only and acyclic even when a definition declares a bounded repeat. |
| Data authority | Task dependencies may inform execution but do not define Factory context isolation. | Job dependencies control readiness; Job inputs and outputs remain existing Build contracts. | Control ordering grants no data. Context, artifacts, mounts, credentials, and typed handoffs require explicit projections and policy. |

## Evidence locations

The matrix is grounded in these current implementations:

- Octa structure and cycle behavior: `crates/octa-dag/src/dag.rs` in the Octa
  repository at the revision recorded above;
- Octa planner edge construction and cycle rejection:
  `crates/octa-executor/src/planner/` in that repository;
- OctaCity Job graph collection, validation, and reconciliation:
  `server/core/octacity-server-orchestrator/src/reconcile.rs`;
- Factory target semantics:
  `openspec/changes/add-dark-factory-mode/design.md` and
  `openspec/changes/add-dark-factory-mode/specs/server/dark-factory/spec.md`.

The Octa source references identify external review evidence. They are not
OctaCity build inputs or runtime dependencies.

## Provider-neutral conformance corpus

The version-one corpus covers:

- deliberately unordered diamond input and deterministic canonical output;
- reachability with a disconnected node;
- a multi-node strongly connected component and a self-loop;
- duplicate node and edge rejection;
- missing predecessor and successor rejection;
- node, edge, depth, and fan-out measurements.

The corpus contains only string identities, directed edges, roots, structural
issues, and expected facts. It contains no Build, Job, Flow node kind, provider,
plugin, command, permission, persistence, scheduling, retry, or budget concept.
The internal module conforms to these fixtures through the Factory adapter;
Factory-specific policy remains outside the structural implementation.

## No shared-kernel commitment

This change does not add `octa-dag`, an `octa-graph` package, or another shared
graph dependency to OctaCity. It does not modify Octa, and it does not route Job
reconciliation through the new Factory module. Similar algorithms are not by
themselves a sufficient abstraction boundary.

A later separately reviewed change may extract a shared pure kernel only if:

1. at least two working consumers need the same non-trivial structural behavior;
2. the common interface is smaller and more stable than either domain adapter;
3. migration replaces existing logic instead of adding a second validation layer;
4. conformance proves no execution or policy drift; and
5. cross-repository release coupling costs less than the locality it removes.

Until those conditions are demonstrated, Factory graph code stays private to
the OctaCity Factory core, Octa keeps its task DAG, and the Job reconciler keeps
its persisted execution semantics.
