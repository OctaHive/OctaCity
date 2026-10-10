# Configurable Factory Flows

`base-intake.json` is a supplied `FlowDefinitionTemplate`, not a runtime-owned
business flow. Edit or clone it to create another flow. Node keys and outcome
keys are operator-defined. Their names never select Rust execution behavior.

The editable document contains local bounded schemas, nodes, explicit input
bindings, finite output contracts, deterministic gates, transitions, bounded
repeats and terminal schemas. `instantiate` resolves schema choices and exact
trusted Build profiles into an immutable `FlowDefinition` and schema catalogue.
Publish them with the complete definition closure, limits and named pool
settings in `FactoryFlowConfiguration`; admission freezes that version.

## Inputs and outputs

A node's `input_binding.sources` selects immutable Work fields, incoming data
under an exact schema, accepted records or a declared predecessor. Choose the
payload view for reported data or the metadata view for trusted identities and
verified Build output facts. The mapping copies only selected values into the
node's closed input schema. Context access is separately explicit. A nested node may select `caller_input` to project the exact frozen payload of its parent call; context is never inherited implicitly. The parent retains the exact child terminal result as provenance.

Build nodes bind one exact command or reasoning profile. Model observations
use the configured result schema; independently verified evidence has separate
logical output names, schemas and tool/plugin identities. Terminal research routes require a separate `terminal_resolution` verifier Report under the configured `terminal_facts` schema, including acceptance, exact outcome, input digest and freshness. Build profiles serving the supplied research gates must select that output as independent evidence. Reproduction facts alone cannot resolve Work.

Gates use bounded
predicates over those facts, finite outcomes and explicit fallback. A shared
`default_output` mapping serves outcomes without an explicit `outputs` override;
the selected outcome still checks its exact result schema. Gates can
compare freshness to `observation_time`; incoming data cannot replace it.

A `trusted_action` node binds a plugin action and bounded parameters. A status
action can set any allowed status string; its connector translates that value
to a label or native status. Notifications and other actions use the same port.
No research outcome automatically invokes an external action.

## Extending the example

1. Add a schema choice or reuse a compatible existing one.
2. Add a node with any new name, input selection, capability/profile, budget,
   narrowed permissions and declared typed outcomes.
3. Connect accepted predecessor outcomes to that node and its outcomes to
   successor nodes or terminals. Declare limits and exhaustion for repeats.
4. For a queued node, add a named pool binding and its immutable policy/settings
   to the journey configuration. Project severity and exact dependencies from
   the frozen node input; selection reserves the full node budget for each node generation. Repeats freeze a new input identity and acquire a new selection even when the payload is unchanged.
5. Validate and publish a new version. Previously admitted runs keep their old
   schemas, profiles, graph, policy parameters and input provenance.

The application contract inserts `accessibility_audit` by these configuration
operations and runs it through the same owner in memory and PostgreSQL.
Permissions in the supplied example default to deny-all: publication must
select qualified profile and authority choices for real execution.

`requirements_needed.accepted_work_outcomes` marks the configured bypass result
as an accepting gate. Accepted Work freezes the original Work and complete
accepted ancestry. The configured bypass still enters every declared downstream
protected-test, implementation, review and verification node. These example
nodes provide control-path bindings; their production capability, protected-test
isolation and independent review work is completed in later plan tasks.
