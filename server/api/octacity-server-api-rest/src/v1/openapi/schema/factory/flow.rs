use super::*;

pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let bounds = octacity_server_application::factory_configuration_document_bounds();
  schemas.insert(
    "FactoryFlowNodeKind".to_owned(),
    json!({"type":"string","enum":octacity_server_application::factory_flow_node_kinds().collect::<Vec<_>>()}),
  );
  schemas.insert(
    "FactoryFlowDefinitionReference".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("version", positive_integer()),
        ("digest", schema_ref("FactoryDigest")),
      ],
      &["id", "version", "digest"],
    ),
  );
  schemas.insert(
    "FactoryFlowConfiguration".to_owned(),
    object(
      [
        ("closure", schema_ref("FactoryFlowDefinitionClosure")),
        ("limits", schema_ref("FactoryFlowAdmissionLimits")),
        (
          "data_schemas",
          bounded_array(schema_ref("FactoryFlowDataSchema"), bounds.schema_entries),
        ),
        (
          "pools",
          bounded_map(schema_ref("FactoryFlowPoolSettings"), bounds.schema_entries),
        ),
      ],
      &["closure", "limits", "data_schemas"],
    ),
  );
  schemas.insert(
    "FactoryFlowDefinitionClosure".to_owned(),
    object(
      [
        ("root", schema_ref("FactoryFlowDefinitionReference")),
        (
          "definitions",
          bounded_array(schema_ref("FactoryFlowDefinition"), bounds.definitions),
        ),
      ],
      &["root", "definitions"],
    ),
  );
  schemas.insert(
    "FactoryFlowAdmissionLimits".to_owned(),
    object(
      [
        ("max_depth", bounded_integer(1, bounds.depth.into())),
        ("max_expanded_nodes", bounded_integer(1, bounds.expanded_nodes.into())),
        ("max_repeat_count", bounded_integer(1, bounds.repeats.into())),
        ("max_fan_out", bounded_integer(1, bounds.fan_out.into())),
        ("max_wip", bounded_integer(1, bounds.active_nodes.into())),
        ("budget", schema_ref("FactoryBudget")),
        ("permissions", schema_ref("FactoryPermissions")),
      ],
      &[
        "max_depth",
        "max_expanded_nodes",
        "max_repeat_count",
        "max_fan_out",
        "max_wip",
        "budget",
        "permissions",
      ],
    ),
  );
  schemas.insert(
    "FactoryFlowExecutionPolicy".to_owned(),
    object(
      [
        ("budget", schema_ref("FactoryBudget")),
        ("permissions", schema_ref("FactoryPermissions")),
        ("max_active_nodes", bounded_integer(1, bounds.active_nodes.into())),
        ("verification", nullable(schema_ref("FactoryVerificationPolicy"))),
      ],
      &["budget", "permissions", "max_active_nodes"],
    ),
  );
  schemas.insert(
    "FactoryFlowOutcome".to_owned(),
    object(
      [
        ("key", key()),
        ("kind", json!({"type":"string","enum":["success","failure"]})),
        ("schema", schema_ref("FactoryImmutableReferenceResource")),
        ("public_result", json!({"type":"boolean",
          "description":"Defaults to false when omitted. Explicitly disclose this outcome's schema-checked result to authorized Run readers; inputs, Context and evidence stay private."})),
      ],
      &["key", "kind", "schema"],
    ),
  );
  schemas.insert(
    "FactoryFlowTerminal".to_owned(),
    object(
      [
        ("key", key()),
        ("schema", schema_ref("FactoryImmutableReferenceResource")),
      ],
      &["key", "schema"],
    ),
  );
  schemas.insert(
    "FactoryFlowNodeDefinition".to_owned(),
    object(
      [
        ("key", key()),
        ("kind", schema_ref("FactoryFlowNodeKind")),
        (
          "input_schema",
          nullable(schema_ref("FactoryImmutableReferenceResource")),
        ),
        (
          "outcomes",
          bounded_array(schema_ref("FactoryFlowOutcome"), bounds.schema_entries),
        ),
        ("budget", schema_ref("FactoryBudget")),
        ("permissions", schema_ref("FactoryPermissions")),
        ("required", boolean()),
        ("subflow", nullable(schema_ref("FactoryFlowDefinitionReference"))),
        ("action", nullable(schema_ref("FactoryFlowAction"))),
        ("phase_pool", nullable(key())),
        ("gate", nullable(schema_ref("FactoryFlowGate"))),
        ("operator_requests", nullable(schema_ref("FactoryFlowHumanGateBinding"))),
        ("input_binding", nullable(schema_ref("FactoryFlowInputBinding"))),
        ("context_binding", nullable(schema_ref("FactoryFlowContextBinding"))),
        ("build", nullable(schema_ref("FactoryFlowBuildBinding"))),
        (
          "profile_selection",
          nullable(schema_ref("FactoryProfileSelectionBinding")),
        ),
        ("accepted_work_outcomes", bounded_array(key(), bounds.schema_entries)),
        ("max_execution_retries", non_negative_integer()),
      ],
      &[
        "key",
        "kind",
        "input_schema",
        "outcomes",
        "budget",
        "permissions",
        "required",
        "subflow",
      ],
    ),
  );
  schemas.insert(
    "FactoryFlowDefinition".to_owned(),
    object(
      [
        ("reference", schema_ref("FactoryFlowDefinitionReference")),
        (
          "input_schema",
          nullable(schema_ref("FactoryImmutableReferenceResource")),
        ),
        ("entry", key()),
        (
          "nodes",
          bounded_array(schema_ref("FactoryFlowNodeDefinition"), bounds.nodes),
        ),
        (
          "transitions",
          bounded_array(schema_ref("FactoryFlowTransition"), bounds.transitions),
        ),
        (
          "terminals",
          bounded_array(schema_ref("FactoryFlowTerminal"), bounds.schema_entries),
        ),
        (
          "context_projections",
          bounded_array(schema_ref("FactoryFlowContextProjection"), bounds.transitions),
        ),
        (
          "data_projections",
          bounded_array(schema_ref("FactoryFlowDataProjection"), bounds.transitions),
        ),
        ("execution", schema_ref("FactoryFlowExecutionPolicy")),
      ],
      &[
        "reference",
        "input_schema",
        "entry",
        "nodes",
        "transitions",
        "terminals",
        "context_projections",
        "data_projections",
        "execution",
      ],
    ),
  );
  schemas.insert(
    "FactoryFlowTransitionTarget".to_owned(),
    json!({"oneOf":[
      tagged("kind","node",[("key",key())],&["key"]),tagged("kind","terminal",[("key",key())],&["key"]),
    ]}),
  );
  schemas.insert(
    "FactoryFlowTransition".to_owned(),
    object(
      [
        ("predecessor", key()),
        ("outcome", key()),
        ("target", schema_ref("FactoryFlowTransitionTarget")),
        ("max_repeats", bounded_integer(0, bounds.repeats.into())),
        ("exhausted_terminal", nullable(key())),
        ("starts_cycle", boolean()),
      ],
      &["predecessor", "outcome", "target", "max_repeats", "exhausted_terminal"],
    ),
  );
  schemas.insert(
    "FactoryFlowContextProjection".to_owned(),
    object(
      [
        ("predecessor", key()),
        ("outcome", key()),
        ("successor", key()),
        ("selections", bounded_array(key(), bounds.schema_entries)),
      ],
      &["predecessor", "outcome", "successor", "selections"],
    ),
  );
  schemas.insert(
    "FactoryFlowDataProjection".to_owned(),
    object(
      [
        ("predecessor", key()),
        ("outcome", key()),
        ("successor", key()),
        ("schema", schema_ref("FactoryImmutableReferenceResource")),
      ],
      &["predecessor", "outcome", "successor", "schema"],
    ),
  );
  // This describes the installed built-in action without closing the generic plugin envelope.
  schemas.insert(
    "FactoryStageSourceUpdate".to_owned(),
    object(
      [
        ("source", schema_ref("WorkSourceBinding")),
        ("reporter", schema_ref("WorkSourceBinding")),
        ("connection", nullable(schema_ref("WorkSourceConnectionReference"))),
        ("effects", schema_ref("WorkSourceStatusEffects")),
        (
          "ownership_scopes",
          nullable(bounded_array(
            key(),
            octacity_server_application::MAX_SOURCE_REPORTING_CHOICES
              * (2 * octacity_server_application::MAX_SOURCE_REPORTING_EFFECT_LABELS + 1),
          )),
        ),
      ],
      &["source", "reporter", "connection", "effects"],
    ),
  );
  schemas.insert(
    "FactoryWorkStatusActionParameters".to_owned(),
    object([
      ("status", json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_FACTORY_SAFE_TEXT_BYTES,
        "x-max-utf8-bytes":octacity_server_application::MAX_FACTORY_SAFE_TEXT_BYTES,
        "description":"Arbitrary configured status resolved by the original source's pinned Reporter mapping; no provider or outcome vocabulary is imposed."})),
      ("source_update", schema_ref("FactoryStageSourceUpdate")),
      ("outbox", nullable(object([
        ("max_attempts", bounded_integer(1, octacity_server_application::FactoryReporterRetryPolicy::MAX_ATTEMPTS.into())),
        ("retry_delay_millis", bounded_integer(1, octacity_server_application::FactoryReporterRetryPolicy::MAX_RETRY_DELAY_MILLIS)),
      ], &["max_attempts", "retry_delay_millis"]))),
    ], &["status"]),
  );
  schemas.insert(
    "FactoryIssueStatusActionParameters".to_owned(),
    object(
      [
        ("workflow_id", json!({"type":"string","format":"uuid"})),
        ("workflow_version", positive_integer()),
        ("transition_id", json!({"type":"string","format":"uuid"})),
        ("outcome", key()),
      ],
      &["workflow_id", "workflow_version", "transition_id", "outcome"],
    ),
  );
  schemas.insert(
    "FactoryFlowAction".to_owned(),
    object(
      [
        ("plugin", schema_ref("FactoryImmutableReferenceResource")),
        ("action", key()),
        ("parameters", json!({"description":"Plugin-owned parameters. The installed set_status action uses FactoryWorkStatusActionParameters; transition_issue uses FactoryIssueStatusActionParameters and the existing visible Issue workflow contracts. Its typed input selects candidate and retained evidence digests; an absent Issue association is unavailable. Other installed actions retain their own contracts."})),
      ],
      &["plugin", "action", "parameters"],
    ),
  );
  schemas.insert(
    "FactoryFlowGate".to_owned(),
    object(
      [
        ("policy", schema_ref("FactoryImmutableReferenceResource")),
        ("parameters", json!({})),
      ],
      &["policy", "parameters"],
    ),
  );
}
