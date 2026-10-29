//! Read-only preset authoring reuses ordinary immutable management schemas.
use super::*;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "FactoryPresetSummary".into(),
    object(
      [
        ("reference", schema_ref("FactoryImmutableReferenceResource")),
        ("parameters", bounded_array(object([("path", non_empty_string()), ("kind", json!({"type":"string","enum":["identity","role_profile","repository","project_context","execution_policy","configuration_policy","source_input","reporting_effects"]})), ("required", boolean())], &["path","kind","required"]), octacity_server_application::factory_configuration_document_bounds().choice_entries)),
        ("required_node_kinds", bounded_array(schema_ref("FactoryFlowNodeKind"), octacity_server_application::factory_configuration_document_bounds().nodes)),
        (
          "profile_aliases",
          bounded_array(
            key(),
            octacity_server_application::factory_configuration_document_bounds().nodes,
          ),
        ),
      ],
      &["reference", "profile_aliases", "parameters", "required_node_kinds"],
    ),
  );
  let b = octacity_server_application::factory_configuration_document_bounds();
  let uuid = || json!({"type":"string","format":"uuid"});
  let text = || json!({"type":"string","maxLength":b.text_bytes});
  let reference = || schema_ref("FactoryImmutableReferenceResource");
  let digest = || schema_ref("FactoryDigest");
  schemas.insert(
    "FactoryPresetProfileSelection".into(),
    object(
      [
        ("role", schema_ref("FactoryRoleReference")),
        ("profile", schema_ref("FactoryRoleProfileReference")),
        ("build", schema_ref("FactoryFlowBuildBinding")),
      ],
      &["role", "profile", "build"],
    ),
  );
  schemas.insert(
    "FactoryPresetConfigurationParameters".into(),
    object(
      [
        ("admission_policy", key()),
        ("permission_ceiling", key()),
        ("wip_limits", schema_ref("FactoryWipLimits")),
        ("hard_budget", schema_ref("FactoryBudget")),
      ],
      &["admission_policy", "permission_ceiling", "wip_limits", "hard_budget"],
    ),
  );
  schemas.insert(
    "TriageProjectContextDraft".into(),
    object(
      [
        ("project_id", uuid()),
        ("goals", nullable(schema_ref("FactoryArtifactReference"))),
        ("rules", nullable(schema_ref("FactoryArtifactReference"))),
        (
          "documents",
          bounded_map(schema_ref("FactoryArtifactReference"), b.schema_entries),
        ),
      ],
      &["project_id", "goals", "rules", "documents"],
    ),
  );
  schemas.insert(
    "FactoryPresetInputSelection".into(),
    object(
      [
        ("id", uuid()),
        ("connection", schema_ref("WorkSourceConnectionReference")),
        ("source", schema_ref("WorkSourceBinding")),
        ("reporter", nullable(schema_ref("WorkSourceBinding"))),
        ("reporting", nullable(schema_ref("WorkSourceStatusMapping"))),
        ("predicate", nullable(schema_ref("WorkSourcePredicate"))),
      ],
      &["id", "connection", "source"],
    ),
  );
  schemas.insert(
    "FactoryPresetResolutionRequest".into(),
    object(
      [
        ("preset", reference()),
        ("flow_id", nullable(uuid())),
        ("configuration_id", nullable(uuid())),
        ("input", nullable(schema_ref("FactoryPresetInputSelection"))),
        (
          "effects",
          bounded_map(schema_ref("FactoryPresetEffectSelection"), b.nodes),
        ),
        (
          "profiles",
          bounded_map(schema_ref("FactoryPresetProfileSelection"), b.nodes),
        ),
        (
          "repository",
          nullable(object(
            [
              ("project_id", uuid()),
              ("id", uuid()),
              ("version", positive_integer()),
              ("definition_digest", digest()),
            ],
            &["project_id", "id", "version", "definition_digest"],
          )),
        ),
        ("context", bounded_map(schema_ref("TriageProjectContextDraft"), b.nodes)),
        ("execution", nullable(schema_ref("FactoryFlowExecutionPolicy"))),
        (
          "configuration",
          nullable(schema_ref("FactoryPresetConfigurationParameters")),
        ),
      ],
      &["preset"],
    ),
  );
  schemas.insert(
    "FactoryPresetResolution".into(),
    object(
      [
        ("preset", reference()),
        ("completion", schema_ref("SourceCompletionDocument")),
        (
          "diagnostics",
          bounded_array(
            object(
              [
                ("path", text()),
                ("reason", json!({"type":"string","enum":["required","unavailable"]})),
              ],
              &["path", "reason"],
            ),
            b.choice_entries,
          ),
        ),
        ("input", nullable(schema_ref("WorkSourceInputIntent"))),
        (
          "dependencies",
          bounded_array(
            object(
              [("id", uuid()), ("definition", schema_ref("FactoryFlowDefinitionDraft"))],
              &["id", "definition"],
            ),
            b.definitions,
          ),
        ),
        ("flow", nullable(schema_ref("FactoryFlowDefinitionDraft"))),
        ("configuration", nullable(schema_ref("FactoryConfigurationDefinition"))),
      ],
      &[
        "preset",
        "completion",
        "diagnostics",
        "flow",
        "configuration",
        "input",
        "dependencies",
      ],
    ),
  );
  schemas.insert("SourceCompletionDocument".into(),json!({"oneOf":[
    object([("kind",json!({"const":"draft"})),("project_id",uuid()),("preset",reference()),
      ("content",json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_SOURCE_BODY_BYTES,"x-max-utf8-bytes":octacity_server_application::MAX_SOURCE_BODY_BYTES}))],&["kind","project_id","preset","content"]),
    object([("kind",json!({"const":"project_input"})),("project_id",uuid()),("artifact",schema_ref("FactoryArtifactReference"))],&["kind","project_id","artifact"])
  ]}));

  schemas.insert(
    "FactoryPresetPage".into(),
    object(
      [
        ("total", non_negative_integer()),
        (
          "items",
          bounded_array(schema_ref("FactoryPresetSummary"), b.choice_entries),
        ),
        ("next_cursor", nullable(key())),
      ],
      &["total", "items", "next_cursor"],
    ),
  );
  schemas.insert(
    "FactoryPresetEffectSelection".into(),
    object(
      [
        ("id", uuid()),
        ("definition", schema_ref("FactoryResultReportingDraft")),
      ],
      &["id", "definition"],
    ),
  );
  schemas.insert(
    "FactoryReportingTarget".into(),
    object(
      [
        ("source", schema_ref("WorkSourceBinding")),
        ("reporter", schema_ref("WorkSourceBinding")),
        ("connection", schema_ref("WorkSourceConnectionReference")),
        ("policy", schema_ref("WorkSourceReportingPolicy")),
      ],
      &["source", "reporter", "connection", "policy"],
    ),
  );
  schemas.insert("FactoryReportingSelection".into(), json!({"oneOf":[
    tagged("selection","literal",[("value",json!({"type":["string","number","boolean","null"],"maxLength":b.text_bytes}))], &["value"]),
    tagged("selection","integer_range",[("minimum",integer()),("maximum",integer())], &["minimum","maximum"])
  ]}));
  schemas.insert(
    "FactoryReportingCase".into(),
    object(
      [
        ("selection", schema_ref("FactoryReportingSelection")),
        ("effects", nullable(schema_ref("WorkSourceStatusEffects"))),
      ],
      &["selection", "effects"],
    ),
  );
  schemas.insert(
    "FactoryReportingDimension".into(),
    object(
      [
        ("key", key()),
        ("path", bounded_array(key(), b.schema_depth)),
        ("outcomes", bounded_array(key(), b.schema_entries)),
        (
          "cases",
          bounded_array(schema_ref("FactoryReportingCase"), b.schema_entries),
        ),
      ],
      &["key", "path", "outcomes", "cases"],
    ),
  );
  schemas.insert(
    "FactoryResultReportingDraft".into(),
    object(
      [
        ("target", schema_ref("FactoryReportingTarget")),
        (
          "outcomes",
          bounded_map(nullable(schema_ref("WorkSourceStatusEffects")), b.schema_entries),
        ),
        (
          "dimensions",
          bounded_array(schema_ref("FactoryReportingDimension"), b.schema_entries),
        ),
        ("receipt", schema_ref("FactoryFlowDataSchema")),
        ("action_outcome", key()),
        ("budget", schema_ref("FactoryBudget")),
        ("execution", schema_ref("FactoryFlowExecutionPolicy")),
        (
          "retry",
          object(
            [
              (
                "max_attempts",
                bounded_integer(
                  1,
                  octacity_server_application::FactoryReporterRetryPolicy::MAX_ATTEMPTS.into(),
                ),
              ),
              (
                "retry_delay_millis",
                bounded_integer(
                  1,
                  octacity_server_application::FactoryReporterRetryPolicy::MAX_RETRY_DELAY_MILLIS,
                ),
              ),
            ],
            &["max_attempts", "retry_delay_millis"],
          ),
        ),
      ],
      &[
        "target",
        "outcomes",
        "dimensions",
        "receipt",
        "action_outcome",
        "budget",
        "execution",
        "retry",
      ],
    ),
  );
}
