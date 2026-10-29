use super::super::{
  array, boolean, integer, non_empty_string, non_negative_integer, nullable, object, positive_integer, schema_ref,
};
use serde_json::{Map, Value, json};

mod bindings;
mod data;
mod diagnostics;
mod flow;
mod permissions;
mod presets;
mod roles;
mod runtime_settings;

pub(super) fn key() -> Value {
  json!({"type":"string","minLength":1,"maxLength":octacity_server_application::factory_configuration_document_bounds().key_bytes,"pattern":"^[a-z0-9][a-z0-9._-]*$"})
}

fn bounded_integer(minimum: u64, maximum: u64) -> Value {
  json!({"type":"integer","minimum":minimum,"maximum":maximum})
}

pub(super) fn bounded_array(items: Value, maximum: usize) -> Value {
  json!({"type":"array","items":items,"maxItems":maximum})
}

fn bounded_map(value: Value, maximum: usize) -> Value {
  json!({"type":"object","additionalProperties":value,"maxProperties":maximum,"propertyNames":key()})
}

pub(super) fn tagged<const N: usize>(
  tag: &str,
  variant: &str,
  properties: [(&str, Value); N],
  required: &[&str],
) -> Value {
  let properties = properties
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value))
    .chain([(tag.to_owned(), json!({"const":variant}))])
    .collect::<Map<_, _>>();
  let required = std::iter::once(tag).chain(required.iter().copied()).collect::<Vec<_>>();
  json!({"type":"object","additionalProperties":false,"properties":properties,"required":required})
}

pub(super) fn insert_factory_schemas(schemas: &mut Map<String, Value>) {
  let bounds = octacity_server_application::factory_configuration_document_bounds();
  diagnostics::insert(schemas);
  roles::insert(schemas);
  presets::insert(schemas);
  runtime_settings::insert(schemas);
  schemas.insert(
    "FactoryFlowHumanGateBinding".to_owned(),
    json!({"oneOf":[
      tagged("operation", "escalation", [("outcomes", json!({"type":"object","minProperties":1,"maxProperties":3,
        "propertyNames":{"enum":["acknowledge","resume","abort"]},"additionalProperties":key()}))], &["outcomes"]),
      tagged("operation", "delivery", [("outcomes", object([("confirm", key())], &["confirm"]))], &["outcomes"]),
      tagged("operation", "promotion", [("outcomes", object([("confirm", key())], &["confirm"]))], &["outcomes"]),
    ]}),
  );
  schemas.insert("FactoryNodeOperationRequest".to_owned(), json!({"oneOf":[
    tagged("operation", "escalation", [
      ("disposition", json!({"type":"string","enum":["acknowledge","resume","abort"]})),
      ("reason", json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_FACTORY_OPERATOR_REASON_BYTES})),
    ], &["disposition","reason"]),
    tagged("operation", "delivery", [
      ("disposition", json!({"const":"confirm"})), ("reason", json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_FACTORY_OPERATOR_REASON_BYTES})),
    ], &["disposition","reason"]),
    tagged("operation", "promotion", [
      ("disposition", json!({"const":"confirm"})), ("reason", json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_FACTORY_OPERATOR_REASON_BYTES})),
    ], &["disposition","reason"]),
  ]}));
  schemas.insert("FactoryNodeResponseRequest".to_owned(), object([
    ("outcome", key()), ("value", json!({"description":"Bounded observations validated against the exact declared outcome schema; no lifecycle authority."})),
  ], &["outcome","value"]));
  schemas.insert("FactoryConfirmationRequest".to_owned(), object([], &[]));
  schemas.insert(
    "FactoryRunControlResponse".to_owned(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("run_id", non_empty_string()),
        ("version", positive_integer()),
        ("state", schema_ref("FactoryRunState")),
        ("cancellation_requested", boolean()),
      ],
      &["disposition", "run_id", "version", "state", "cancellation_requested"],
    ),
  );
  schemas.insert(
    "FactoryConfigurationValidation".to_owned(),
    object(
      [
        ("configuration", schema_ref("FactoryConfigurationDocument")),
        ("diagnostics", schema_ref("FactoryValidationDiagnostics")),
      ],
      &["configuration", "diagnostics"],
    ),
  );
  schemas.insert("FactoryValidationDiagnostics".to_owned(), object([
    ("errors", json!({"type":"array","maxItems":1,"items":object([
      ("field",non_empty_string()),("code",json!({"enum":["unavailable_reference","invalid_contract"]})),
      ("definition",schema_ref("FactoryFlowDefinitionReference")),("node",non_empty_string())
    ], &["field","code"])})),
    ("unavailable_references", json!({"type":"array","maxItems":1,"items":non_empty_string()})),
    ("required_node_capabilities",json!({"type":"array","items":schema_ref("FactoryImmutableReferenceResource")})),
    ("required_agent_capabilities",json!({"type":"array","items":schema_ref("FactoryImmutableReferenceResource")})),
    ("required_backend_capabilities",json!({"type":"array","items":non_empty_string()})),
    ("effective_permissions",json!({"type":"array","items":object([
      ("definition",schema_ref("FactoryFlowDefinitionReference")),("node",non_empty_string()),("permissions",schema_ref("FactoryPermissions"))
    ], &["definition","node","permissions"])})),
    ("warnings",json!({"type":"array","items":json!({"enum":["agent_local_permissions_resolved_at_dispatch"]})})),
  ], &["errors","unavailable_references","required_node_capabilities","required_agent_capabilities","required_backend_capabilities","effective_permissions","warnings"]));
  schemas.insert(
    "FactoryDecisionOutcome".to_owned(),
    json!({"type":"string","enum":octacity_server_application::factory_decision_outcomes().collect::<Vec<_>>()}),
  );
  schemas.insert(
    "FactoryDeliveryState".to_owned(),
    json!({"type":"string","enum":octacity_server_application::factory_delivery_states().collect::<Vec<_>>()}),
  );
  schemas.insert(
    "FactoryArtifactReference".to_owned(),
    object(
      [
        ("artifact_id", non_empty_string()),
        ("content_digest", schema_ref("FactoryDigest")),
        ("encoded_size", bounded_integer(1, 16 * 1024 * 1024)),
      ],
      &["artifact_id", "content_digest", "encoded_size"],
    ),
  );
  schemas.insert(
    "FactoryWorkArtifacts".to_owned(),
    object(
      [
        ("task", schema_ref("FactoryArtifactReference")),
        ("acceptance", schema_ref("FactoryArtifactReference")),
        (
          "specifications",
          bounded_array(schema_ref("FactoryArtifactReference"), 64),
        ),
      ],
      &["task", "acceptance", "specifications"],
    ),
  );
  schemas.insert(
    "FactoryWorkClassification".to_owned(),
    object(
      [
        ("priority", bounded_integer(0, 100)),
        (
          "risk",
          json!({"type":"string","enum":["low","medium","high","critical"]}),
        ),
        (
          "metadata",
          bounded_map(
            json!({"type":"string","minLength":1,"maxLength":bounds.work_metadata_value_bytes}),
            bounds.work_metadata_entries,
          ),
        ),
      ],
      &["priority", "risk", "metadata"],
    ),
  );
  schemas.get_mut("FactoryWorkClassification").unwrap()["properties"]["metadata"]["propertyNames"]["maxLength"] =
    json!(bounds.work_metadata_key_bytes);
  schemas.insert(
    "AdmitManualFactoryWorkRequest".to_owned(),
    object(
      [
        ("configuration_id", non_empty_string()),
        ("configuration_version", positive_integer()),
        ("repository_id", non_empty_string()),
        ("repository_version", positive_integer()),
        (
          "external_identity",
          json!({"type":"string","minLength":1,"maxLength":bounds.external_work_identity_bytes}),
        ),
        ("source", schema_ref("ManualSource")),
        ("artifacts", schema_ref("FactoryWorkArtifacts")),
        ("classification", schema_ref("FactoryWorkClassification")),
        (
          "profile_overrides",
          bounded_array(schema_ref("FactoryProfileAdmissionOverride"), bounds.profile_overrides),
        ),
      ],
      &[
        "configuration_id",
        "configuration_version",
        "repository_id",
        "repository_version",
        "external_identity",
        "source",
        "artifacts",
        "classification",
      ],
    ),
  );
  schemas.insert(
    "FactoryRunState".to_owned(),
    json!({"type":"string","enum":["admitted","running","waiting","cancelled","completed"]}),
  );
  schemas.insert(
    "FactoryAdmissionResponse".to_owned(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("work_envelope_id", non_empty_string()),
        ("factory_run_id", non_empty_string()),
        ("base_revision", non_empty_string()),
        ("state", schema_ref("FactoryRunState")),
        ("admitted_at_unix_ms", integer()),
      ],
      &[
        "disposition",
        "work_envelope_id",
        "factory_run_id",
        "base_revision",
        "state",
        "admitted_at_unix_ms",
      ],
    ),
  );
  schemas.insert(
    "FactoryExactSubject".to_owned(),
    object(
      [
        ("project_id", non_empty_string()),
        ("repository_id", non_empty_string()),
        ("base_revision", non_empty_string()),
      ],
      &["project_id", "repository_id", "base_revision"],
    ),
  );
  schemas.insert(
    "FactoryCandidateSubject".to_owned(),
    object(
      [
        ("exact", schema_ref("FactoryExactSubject")),
        ("candidate_revision", non_empty_string()),
        ("changeset_digest", schema_ref("FactoryDigest")),
      ],
      &["exact", "candidate_revision", "changeset_digest"],
    ),
  );
  schemas.insert(
    "FactoryBudgetUsage".to_owned(),
    object(
      [
        ("attempts", bounded_integer(0, u64::from(bounds.attempts))),
        ("elapsed_millis", bounded_integer(0, bounds.elapsed_millis)),
        ("tokens", bounded_integer(0, bounds.tokens)),
        ("cost_micro_units", bounded_integer(0, bounds.cost_micro_units)),
        ("output_bytes", bounded_integer(0, bounds.output_bytes)),
      ],
      &[
        "attempts",
        "elapsed_millis",
        "tokens",
        "cost_micro_units",
        "output_bytes",
      ],
    ),
  );
  schemas.insert(
    "FactoryRunSummaryDiagnostics".to_owned(),
    object(
      [
        ("base_revision", non_empty_string()),
        ("candidate", nullable(schema_ref("FactoryCandidateSubject"))),
        (
          "current_nodes",
          bounded_array(
            object(
              [
                ("id", non_empty_string()),
                ("flow_run_id", non_empty_string()),
                ("cycle_id", non_empty_string()),
                ("node", key()),
                ("attempt", positive_integer()),
              ],
              &["id", "flow_run_id", "cycle_id", "node", "attempt"],
            ),
            octacity_server_application::MAX_FACTORY_RUN_SUMMARY_NODES,
          ),
        ),
        ("active_node_count", non_negative_integer()),
        (
          "budget",
          object(
            [
              ("usage", schema_ref("FactoryBudgetUsage")),
              ("limit", schema_ref("FactoryBudget")),
            ],
            &["usage", "limit"],
          ),
        ),
        (
          "latest_decision",
          nullable(object(
            [
              ("id", non_empty_string()),
              ("subject", schema_ref("FactoryCandidateSubject")),
              ("outcome", schema_ref("FactoryDecisionOutcome")),
            ],
            &["id", "subject", "outcome"],
          )),
        ),
        (
          "delivery",
          nullable(object(
            [
              ("id", non_empty_string()),
              ("decision_id", non_empty_string()),
              ("subject", schema_ref("FactoryCandidateSubject")),
              ("state", schema_ref("FactoryDeliveryState")),
            ],
            &["id", "decision_id", "subject", "state"],
          )),
        ),
        ("escalation", nullable(non_empty_string())),
      ],
      &[
        "base_revision",
        "candidate",
        "current_nodes",
        "active_node_count",
        "budget",
        "latest_decision",
        "delivery",
        "escalation",
      ],
    ),
  );
  schemas.insert(
    "FactoryRunSummaryResource".to_owned(),
    object(
      [
        ("issue_association", nullable(schema_ref("IssueWorkAssociation"))),
        ("diagnostics", schema_ref("FactoryRunSummaryDiagnostics")),
        ("id", non_empty_string()),
        ("project_id", non_empty_string()),
        ("work_id", non_empty_string()),
        ("configuration_id", non_empty_string()),
        ("configuration_version", positive_integer()),
        ("source", key()),
        ("external_identity", non_empty_string()),
        ("state", schema_ref("FactoryRunState")),
        ("version", positive_integer()),
        ("admitted_at_unix_ms", integer()),
        ("updated_at_unix_ms", integer()),
      ],
      &[
        "diagnostics",
        "id",
        "project_id",
        "work_id",
        "configuration_id",
        "configuration_version",
        "source",
        "external_identity",
        "state",
        "version",
        "admitted_at_unix_ms",
        "updated_at_unix_ms",
      ],
    ),
  );
  schemas.insert(
    "FactoryRunSummaryPage".to_owned(),
    object(
      [
        ("total", non_negative_integer()),
        ("items", bounded_array(schema_ref("FactoryRunSummaryResource"), 200)),
        (
          "next_cursor",
          nullable(json!({"type":"string","minLength":1,"maxLength":128})),
        ),
      ],
      &["total", "items", "next_cursor"],
    ),
  );
  schemas.insert(
    "FactoryPhaseReadyResource".to_owned(),
    object(
      [
        ("id", schema_ref("FactoryDigest")),
        ("policy_digest", schema_ref("FactoryDigest")),
        ("work_id", non_empty_string()),
        ("project_id", non_empty_string()),
        ("run_id", non_empty_string()),
        ("run_version", positive_integer()),
        ("flow_run_id", non_empty_string()),
        ("cycle_id", non_empty_string()),
        ("node", key()),
        ("generation", non_negative_integer()),
        (
          "severity",
          json!({"type":"string","enum":octacity_server_application::factory_finding_severities().collect::<Vec<_>>()}),
        ),
        ("project_priority", integer()),
        ("admitted_at_unix_ms", integer()),
      ],
      &[
        "id",
        "policy_digest",
        "work_id",
        "project_id",
        "run_id",
        "run_version",
        "flow_run_id",
        "cycle_id",
        "node",
        "generation",
        "severity",
        "project_priority",
        "admitted_at_unix_ms",
      ],
    ),
  );
  schemas.insert(
    "FactoryPhaseReadyPage".to_owned(),
    object(
      [
        ("total", non_negative_integer()),
        ("items", bounded_array(schema_ref("FactoryPhaseReadyResource"), 200)),
        (
          "next_cursor",
          nullable(json!({"type":"string","minLength":1,"maxLength":512})),
        ),
      ],
      &["total", "items", "next_cursor"],
    ),
  );
  flow::insert(schemas);
  let mut validation = schemas["FactoryFlowConfiguration"].clone();
  validation["properties"]["diagnostics"] = schema_ref("FactoryValidationDiagnostics");
  schemas.insert("FactoryFlowDefinitionValidation".to_owned(), validation);
  let mut root = schemas["FactoryFlowDefinition"].clone();
  root["properties"].as_object_mut().unwrap().remove("reference");
  root["required"]
    .as_array_mut()
    .unwrap()
    .retain(|field| field != "reference");
  schemas.insert("FactoryFlowRootDraft".to_owned(), root);
  schemas.insert(
    "FactoryFlowDefinitionDraft".to_owned(),
    object(
      [
        ("root", schema_ref("FactoryFlowRootDraft")),
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
      &["root", "limits", "data_schemas"],
    ),
  );
  schemas.insert(
    "CreateFactoryFlowDefinitionRequest".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("project_id", non_empty_string()),
        ("definition", schema_ref("FactoryFlowDefinitionDraft")),
      ],
      &["id", "project_id", "definition"],
    ),
  );
  schemas.insert(
    "ReplaceFactoryFlowDefinitionRequest".to_owned(),
    object(
      [("definition", schema_ref("FactoryFlowDefinitionDraft"))],
      &["definition"],
    ),
  );
  schemas.insert(
    "FactoryFlowDefinitionDetails".to_owned(),
    object(
      [
        ("project_id", non_empty_string()),
        ("flow", schema_ref("FactoryFlowConfiguration")),
        ("published_at_unix_ms", integer()),
        (
          "native_gate_policies",
          bounded_array(
            schema_ref("FactoryImmutableReferenceResource"),
            bounds.nodes * bounds.definitions,
          ),
        ),
      ],
      &["project_id", "flow", "published_at_unix_ms"],
    ),
  );
  schemas.insert(
    "FactoryFlowDefinitionMutationResponse".to_owned(),
    super::super::mutation_response("FactoryFlowDefinitionDetails"),
  );
  schemas.insert(
    "FactoryFlowDefinitionPage".to_owned(),
    object(
      [
        ("total", non_negative_integer()),
        ("items", bounded_array(schema_ref("FactoryFlowDefinitionDetails"), 200)),
        (
          "next_cursor",
          nullable(json!({"type":"string","minLength":1,"maxLength":128})),
        ),
      ],
      &["total", "items", "next_cursor"],
    ),
  );
  data::insert(schemas);
  bindings::insert(schemas);
  permissions::insert(schemas);
  schemas.insert(
    "FactoryDigest".to_owned(),
    json!({"type":"string","minLength":64,"maxLength":64,"pattern":"^[a-f0-9]{64}$"}),
  );
  schemas.insert(
    "FactoryBudget".to_owned(),
    object(
      [
        ("max_attempts", bounded_integer(1, bounds.attempts.into())),
        ("max_elapsed_millis", bounded_integer(1, bounds.elapsed_millis)),
        ("max_tokens", bounded_integer(0, bounds.tokens)),
        ("max_cost_micro_units", bounded_integer(0, bounds.cost_micro_units)),
        ("max_output_bytes", bounded_integer(0, bounds.output_bytes)),
      ],
      &[
        "max_attempts",
        "max_elapsed_millis",
        "max_tokens",
        "max_cost_micro_units",
        "max_output_bytes",
      ],
    ),
  );
  schemas.insert(
    "FactoryWipLimits".to_owned(),
    object(
      [
        ("max_active_runs", bounded_integer(1, bounds.active_runs.into())),
        ("max_active_nodes", bounded_integer(1, bounds.active_nodes.into())),
      ],
      &["max_active_runs", "max_active_nodes"],
    ),
  );
  schemas.insert(
    "FactoryConfigurationReference".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("version", positive_integer()),
        ("project_id", non_empty_string()),
        ("definition_digest", schema_ref("FactoryDigest")),
      ],
      &["id", "version", "project_id", "definition_digest"],
    ),
  );
  schemas.insert(
    "ReplaceFactoryConfigurationRequest".to_owned(),
    object(
      [("definition", schema_ref("FactoryConfigurationDefinition"))],
      &["definition"],
    ),
  );
  schemas.insert(
    "CreateFactoryConfigurationRequest".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("project_id", non_empty_string()),
        ("definition", schema_ref("FactoryConfigurationDefinition")),
      ],
      &["id", "project_id", "definition"],
    ),
  );
  schemas.insert(
    "FactoryConfigurationDefinition".to_owned(),
    object(
      [
        ("admission_policy", key()),
        ("flow", schema_ref("FactoryFlowConfiguration")),
        ("wip_limits", schema_ref("FactoryWipLimits")),
        ("hard_budget", schema_ref("FactoryBudget")),
        ("permission_ceiling", key()),
        ("enabled", boolean()),
      ],
      &[
        "admission_policy",
        "flow",
        "wip_limits",
        "hard_budget",
        "permission_ceiling",
        "enabled",
      ],
    ),
  );
  schemas.insert(
    "FactoryConfigurationDocument".to_owned(),
    object(
      [
        ("reference", schema_ref("FactoryConfigurationReference")),
        ("admission_policy", schema_ref("FactoryImmutableReferenceResource")),
        ("flow", schema_ref("FactoryFlowConfiguration")),
        ("wip_limits", schema_ref("FactoryWipLimits")),
        ("hard_budget", schema_ref("FactoryBudget")),
        ("permission_ceiling", schema_ref("FactoryImmutableReferenceResource")),
        ("enabled", boolean()),
      ],
      &[
        "reference",
        "admission_policy",
        "flow",
        "wip_limits",
        "hard_budget",
        "permission_ceiling",
        "enabled",
      ],
    ),
  );
  schemas.insert(
    "FactoryConfigurationMutationResponse".to_owned(),
    super::super::mutation_response("FactoryConfigurationDetails"),
  );
  let kinds = octacity_server_application::factory_configuration_choice_kinds().collect::<Vec<_>>();
  schemas.insert("FactoryChoiceKind".to_owned(), json!({"type":"string","enum":kinds}));
  schemas.insert(
    "FactoryImmutableReferenceResource".to_owned(),
    object(
      [
        ("identity", non_empty_string()),
        ("version", non_empty_string()),
        ("digest", non_empty_string()),
      ],
      &["identity", "version", "digest"],
    ),
  );
  schemas.insert(
    "FactoryBuildConfigurationReferenceResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("version", positive_integer()),
        ("project_id", non_empty_string()),
        ("definition_digest", non_empty_string()),
      ],
      &["id", "version", "project_id", "definition_digest"],
    ),
  );
  schemas.insert("FactoryChoiceReferenceResource".to_owned(),json!({"oneOf":[
    object([("kind",json!({"const":"immutable"})),("reference",schema_ref("FactoryImmutableReferenceResource"))],&["kind","reference"]),
    object([("kind",json!({"const":"build_configuration"})),("reference",schema_ref("FactoryBuildConfigurationReferenceResource"))],&["kind","reference"]),
  ]}));
  schemas.insert(
    "FactoryConfigurationChoiceResource".to_owned(),
    object(
      [
        ("identity", non_empty_string()),
        ("kind", schema_ref("FactoryChoiceKind")),
        ("alias", non_empty_string()),
        ("reference", schema_ref("FactoryChoiceReferenceResource")),
        ("metadata", schema_ref("FactoryChoiceMetadata")),
      ],
      &["identity", "kind", "alias", "reference"],
    ),
  );
  schemas.insert("FactoryChoiceMetadata".to_owned(), object([
    ("permission_declarations", schema_ref("FactoryPermissions")),
    ("profile_selection", schema_ref("FactoryProfileSelectorMetadata")),
    ("node_kinds", bounded_array(schema_ref("FactoryFlowNodeKind"),octacity_server_application::factory_flow_node_kinds().count())),
    ("signal_purposes", bounded_array(json!({"type":"string","enum":octacity_server_application::factory_signal_purposes().collect::<Vec<_>>()}),octacity_server_application::factory_signal_purposes().count())),
    ("enforcement", bounded_array(json!({"type":"string","enum":octacity_server_application::factory_permission_categories().collect::<Vec<_>>()}),octacity_server_application::factory_permission_categories().count())),
    ("compatible_choices", bounded_array(object([
      ("kind",schema_ref("FactoryChoiceKind")),("alias",non_empty_string())
    ], &["kind","alias"]), bounds.choice_entries)),
  ], &["node_kinds","signal_purposes","enforcement","compatible_choices"]));
  schemas.insert(
    "FactoryProfileSelectorMetadata".to_owned(),
    object(
      [
        ("provider", schema_ref("FactoryImmutableReferenceResource")),
        ("adapter", schema_ref("FactoryImmutableReferenceResource")),
        ("model", schema_ref("FactoryImmutableReferenceResource")),
        ("question_set", schema_ref("FactoryImmutableReferenceResource")),
        ("calibration", schema_ref("FactoryImmutableReferenceResource")),
        ("probability_semantics", schema_ref("FactoryImmutableReferenceResource")),
        (
          "mode",
          json!({"type":"string","enum":octacity_server_application::factory_signal_modes().collect::<Vec<_>>()}),
        ),
        ("baseline", non_empty_string()),
        (
          "candidates",
          json!({"type":"array","items":key(),"minItems":2,"maxItems":bounds.signal_choices,"uniqueItems":true}),
        ),
        (
          "minimum_probability",
          bounded_integer(0, u64::from(bounds.signal_probability_scale)),
        ),
        (
          "minimum_margin",
          bounded_integer(0, u64::from(bounds.signal_probability_scale)),
        ),
      ],
      &[
        "provider",
        "adapter",
        "model",
        "question_set",
        "calibration",
        "probability_semantics",
        "mode",
        "baseline",
        "candidates",
        "minimum_probability",
        "minimum_margin",
      ],
    ),
  );
  schemas.insert(
    "FactoryConfigurationChoicePage".to_owned(),
    object(
      [
        ("total", non_negative_integer()),
        ("items", array(schema_ref("FactoryConfigurationChoiceResource"))),
        (
          "next_cursor",
          nullable(json!({"type":"string","minLength":1,"maxLength":super::super::MAX_CURSOR_BYTES})),
        ),
      ],
      &["total", "items", "next_cursor"],
    ),
  );
  schemas.insert(
    "FactoryConfigurationDetails".to_owned(),
    object(
      [
        ("configuration", schema_ref("FactoryConfigurationDocument")),
        ("published_at_unix_ms", integer()),
        (
          "pool_policy_digests",
          json!({"type":"object", "additionalProperties":schema_ref("FactoryDigest")}),
        ),
      ],
      &["configuration", "published_at_unix_ms"],
    ),
  );
  schemas.insert(
    "ValidateFactoryFlowDataSchemaRequest".to_owned(),
    object(
      [
        ("project_id", non_empty_string()),
        ("identity", non_empty_string()),
        ("version", non_empty_string()),
        ("shape", schema_ref("FactoryFlowValueSchema")),
      ],
      &["project_id", "identity", "version", "shape"],
    ),
  );
  schemas.insert(
    "FactoryConfigurationSummaryResource".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("project_id", non_empty_string()),
        ("version", positive_integer()),
        ("enabled", boolean()),
        ("created_at_unix_ms", integer()),
        ("published_at_unix_ms", integer()),
      ],
      &[
        "id",
        "project_id",
        "version",
        "enabled",
        "created_at_unix_ms",
        "published_at_unix_ms",
      ],
    ),
  );
  schemas.insert(
    "FactoryConfigurationSummaryPage".to_owned(),
    object(
      [
        ("total", non_negative_integer()),
        ("items", array(schema_ref("FactoryConfigurationSummaryResource"))),
        (
          "next_cursor",
          nullable(json!({"type":"string", "minLength":1, "maxLength":super::super::MAX_CURSOR_BYTES})),
        ),
      ],
      &["total", "items", "next_cursor"],
    ),
  );
}
