use super::*;

pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let bounds = octacity_server_application::factory_configuration_document_bounds();
  schemas.insert("FactoryFlowContextSelection".to_owned(),object([
    ("when_present",nullable(json!({"type":"string"}))),
    ("source_kind",json!({"type":"string","enum":["task","specification","typed_result","bounded_summary","evidence","policy","repository_range","repository_fragment"]})),
    ("source",schema_ref("FactoryFlowDataMapping")),
  ], &["source_kind","source"]));
  schemas.insert(
    "FactoryFlowContextBinding".to_owned(),
    object(
      [
        ("candidate_revision", nullable(schema_ref("FactoryFlowDataMapping"))),
        ("candidate_when_present", nullable(json!({"type":"string"}))),
        (
          "entries",
          bounded_map(schema_ref("FactoryFlowContextSelection"), bounds.schema_entries),
        ),
        (
          "artifact_collections",
          bounded_map(schema_ref("FactoryFlowContextSelection"), bounds.schema_entries),
        ),
      ],
      &["entries"],
    ),
  );
  schemas.insert(
    "FactoryEvidenceRequirement".to_owned(),
    object(
      [
        ("kind", key()),
        ("output_kind", json!({"type":"string","enum":["report","artifact"]})),
        ("schema", schema_ref("FactoryImmutableReferenceResource")),
        ("tool", schema_ref("FactoryImmutableReferenceResource")),
        ("plugin", schema_ref("FactoryImmutableReferenceResource")),
      ],
      &["kind", "output_kind", "schema", "tool", "plugin"],
    ),
  );
  schemas.insert(
    "FactoryDeliverableRequirement".to_owned(),
    object(
      [
        ("kind", key()),
        ("paths", bounded_array(non_empty_string(), bounds.schema_items.into())),
        ("min_files", bounded_integer(1, bounds.schema_items.into())),
      ],
      &["kind", "paths", "min_files"],
    ),
  );
  schemas.insert(
    "FactoryDeliverablePolicy".to_owned(),
    object(
      [
        (
          "permitted_paths",
          bounded_array(non_empty_string(), bounds.schema_items.into()),
        ),
        (
          "forbidden_paths",
          bounded_array(non_empty_string(), bounds.schema_items.into()),
        ),
        (
          "required",
          bounded_array(schema_ref("FactoryDeliverableRequirement"), bounds.schema_entries),
        ),
      ],
      &["permitted_paths", "forbidden_paths", "required"],
    ),
  );
  schemas.insert(
    "FactoryDeliverableBinding".to_owned(),
    object(
      [
        ("accepted_contract", schema_ref("FactoryFlowDataMapping")),
        ("policy", schema_ref("FactoryDeliverablePolicy")),
      ],
      &["accepted_contract", "policy"],
    ),
  );
  schemas.insert(
    "FactoryFlowBuildBinding".to_owned(),
    object(
      [
        ("tool", schema_ref("FactoryImmutableReferenceResource")),
        ("result_tool", nullable(schema_ref("FactoryImmutableReferenceResource"))),
        (
          "capture_write_paths",
          nullable(bounded_array(non_empty_string(), bounds.permission_entries)),
        ),
        (
          "capture_deliverables",
          nullable(schema_ref("FactoryDeliverableBinding")),
        ),
        ("plugin", schema_ref("FactoryImmutableReferenceResource")),
        ("model_or_tool", schema_ref("FactoryImmutableReferenceResource")),
        ("task_digest", schema_ref("FactoryDigest")),
        (
          "build_configuration",
          schema_ref("FactoryBuildConfigurationReferenceResource"),
        ),
        ("result_output", key()),
        (
          "result_output_kind",
          json!({"type":"string","enum":["report","artifact"]}),
        ),
        ("result_outcome", key()),
        (
          "evidence",
          bounded_array(schema_ref("FactoryEvidenceRequirement"), bounds.evidence_items),
        ),
      ],
      &[
        "tool",
        "plugin",
        "model_or_tool",
        "task_digest",
        "build_configuration",
        "result_output",
        "result_output_kind",
        "result_outcome",
      ],
    ),
  );
  schemas.insert(
    "FactoryVerificationCheck".to_owned(),
    object(
      [
        ("class", key()),
        ("target", schema_ref("FactoryFlowDataMapping")),
        (
          "source_visibility",
          json!({"type":"string","enum":["black_box","read_only_source"]}),
        ),
        (
          "endpoints",
          bounded_array(non_empty_string(), bounds.permission_entries),
        ),
        ("secret_profiles", bounded_array(key(), bounds.permission_entries)),
        ("identity_profiles", bounded_array(key(), bounds.permission_entries)),
        ("required", boolean()),
      ],
      &[
        "class",
        "target",
        "source_visibility",
        "endpoints",
        "secret_profiles",
        "identity_profiles",
        "required",
      ],
    ),
  );
  schemas.insert(
    "FactoryVerificationPolicy".to_owned(),
    object(
      [
        ("data_handling", schema_ref("FactoryImmutableReferenceResource")),
        ("required_quorum", positive_integer()),
        (
          "checks",
          bounded_map(schema_ref("FactoryVerificationCheck"), bounds.nodes),
        ),
      ],
      &["data_handling", "required_quorum", "checks"],
    ),
  );
  schemas.insert("FactoryPhasePoolPolicy".to_owned(),object([
    ("phase",key()),("order",json!({"type":"array","items":{"type":"string","enum":["severity","project_priority","age"]},"minItems":3,"maxItems":3,"uniqueItems":true})),
    ("max_wip",bounded_integer(1,bounds.active_nodes.into())),("max_project_wip",bounded_integer(1,bounds.active_nodes.into())),("budget",schema_ref("FactoryBudget")),
  ], &["phase","order","max_wip","max_project_wip","budget"]));
  schemas.insert(
    "FactoryFlowPoolSettings".to_owned(),
    object(
      [
        ("policy", schema_ref("FactoryPhasePoolPolicy")),
        ("project_priority", integer()),
        (
          "capabilities",
          bounded_array(schema_ref("FactoryImmutableReferenceResource"), bounds.schema_entries),
        ),
        ("selection", schema_ref("FactoryFlowDataMapping")),
      ],
      &["policy", "project_priority", "capabilities", "selection"],
    ),
  );
}
