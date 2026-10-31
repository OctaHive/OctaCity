//! Closed allowlists for diagnostic projections; execution documents are never REST resources.
use super::*;

pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let bounds = octacity_server_application::factory_configuration_document_bounds();
  schemas.insert(
    "FactorySignalAnswer".to_owned(),
    object(
      [
        ("question", schema_ref("FactoryImmutableReferenceResource")),
        (
          "value",
          json!({"oneOf":[
            object([("FiniteChoice", object([
              ("selected", non_empty_string()),
              ("selected_probability", bounded_integer(0, u64::from(bounds.signal_probability_scale))),
              ("runner_up_probability", bounded_integer(0, u64::from(bounds.signal_probability_scale)))
            ], &["selected","selected_probability","runner_up_probability"]))], &["FiniteChoice"]),
            object([("BoundedScore", object([
              ("score", json!({"type":"integer","minimum":i32::MIN,"maximum":i32::MAX})),
              ("confidence", bounded_integer(0, u64::from(bounds.signal_probability_scale)))
            ], &["score","confidence"]))], &["BoundedScore"])
          ]}),
        ),
      ],
      &["question", "value"],
    ),
  );
  schemas.insert(
    "FactoryWorkDiagnostic".to_owned(),
    object(
      [
        ("id", non_empty_string()),
        ("configuration", schema_ref("FactoryConfigurationReference")),
        ("external_identity", non_empty_string()),
        ("subject", schema_ref("FactoryWorkSubject")),
        ("priority", json!({"type":"integer","minimum":0,"maximum":100})),
        (
          "risk",
          json!({"type":"string","enum":["low","medium","high","critical"]}),
        ),
      ],
      &[
        "id",
        "configuration",
        "external_identity",
        "subject",
        "priority",
        "risk",
      ],
    ),
  );
  schemas.insert(
    "FactoryNodeCompletionDiagnostic".to_owned(),
    object(
      [
        ("id", schema_ref("FactoryDigest")),
        ("node_attempt_id", non_empty_string()),
        ("flow_run_id", non_empty_string()),
        ("cycle_id", non_empty_string()),
        ("node_key", key()),
        ("outcome", key()),
        ("output_schema", schema_ref("FactoryImmutableReferenceResource")),
        ("output_digest", schema_ref("FactoryDigest")),
        ("result_provenance", json!({"type":"array","description":"Exact retained executing or explicitly selected producer calls, without Context bytes.","items":schema_ref("FactoryCallDiagnostic")})),
        ("public_result", json!({
          "description":"Original result value, validated by output_schema, present only for an explicitly public admitted outcome.",
          "x-max-encoded-bytes":octacity_server_application::factory_configuration_document_bounds().value_bytes
        })),
        ("usage", schema_ref("FactoryBudgetUsage")),
        ("observed_at", integer()),
      ],
      &[
        "id",
        "node_attempt_id",
        "flow_run_id",
        "cycle_id",
        "node_key",
        "outcome",
        "output_schema",
        "output_digest",
        "usage",
        "observed_at",
      ],
    ),
  );
  schemas.insert(
    "FactoryEffectDeliveryDiagnostic".to_owned(),
    object(
      [
        ("id", schema_ref("FactoryDigest")),
        ("record_id", schema_ref("FactoryDigest")),
        ("kind", key()),
        ("input_digest", schema_ref("FactoryDigest")),
        (
          "state",
          json!({"type":"string","enum":["pending","claimed","delivered","failed"]}),
        ),
        (
          "failure",
          nullable(json!({"type":"string","enum":["forbidden","unsupported","exhausted","conflict"]})),
        ),
        ("attempt", bounded_integer(0, u64::from(u16::MAX))),
        ("available_at", integer()),
        ("recorded_at", integer()),
      ],
      &[
        "id",
        "record_id",
        "kind",
        "input_digest",
        "state",
        "failure",
        "attempt",
        "available_at",
        "recorded_at",
      ],
    ),
  );
  schemas.insert(
    "FactoryNodeStopDiagnostic".to_owned(),
    object(
      [
        ("id", schema_ref("FactoryDigest")),
        ("node_attempt_id", non_empty_string()),
        (
          "reason",
          json!({"oneOf":[tagged("kind","execution_failed", [], &[]),
      tagged("kind","cancelled", [("requested_at", integer())], &["requested_at"])]}),
        ),
        ("usage", schema_ref("FactoryBudgetUsage")),
        ("execution", nullable(schema_ref("FactoryDigest"))),
        ("observed_at", integer()),
      ],
      &["id", "node_attempt_id", "reason", "usage", "execution", "observed_at"],
    ),
  );
  let node_fields = [
    ("id", non_empty_string()),
    ("factory_run_id", non_empty_string()),
    ("flow_run_id", non_empty_string()),
    ("cycle_id", non_empty_string()),
    ("node_key", key()),
    ("node_kind", schema_ref("FactoryFlowNodeKind")),
    ("number", positive_integer()),
    ("input_digest", non_empty_string()),
    (
      "execution",
      json!({"oneOf":[tagged("kind","built_in", [], &[]),
      tagged("kind","external",[("identity",schema_ref("FactoryImmutableReferenceResource"))], &["identity"])]}),
    ),
    ("started_at", integer()),
    ("deadline", integer()),
    ("retry_of", nullable(non_empty_string())),
  ];
  let node_required = [
    "id",
    "factory_run_id",
    "flow_run_id",
    "cycle_id",
    "node_key",
    "node_kind",
    "number",
    "input_digest",
    "execution",
    "started_at",
    "deadline",
    "retry_of",
  ];
  schemas.insert(
    "FactoryNodeIdentityDiagnostic".to_owned(),
    object(node_fields.clone(), &node_required),
  );
  schemas.insert(
    "FactoryNodeAttemptDiagnostic".to_owned(),
    object(
      node_fields.into_iter().chain([
        ("retryable", json!({"type":"boolean"})),
        ("active", json!({"type":"boolean"})),
      ]),
      &node_required
        .into_iter()
        .chain(["retryable", "active"])
        .collect::<Vec<_>>(),
    ),
  );
  schemas.insert(
    "FactoryDomainDiagnostic".to_owned(),
    object(
      [
        ("id", schema_ref("FactoryDigest")),
        ("node_attempt_id", non_empty_string()),
        ("flow_run_id", non_empty_string()),
        ("cycle_id", non_empty_string()),
        ("input_digest", schema_ref("FactoryDigest")),
        ("observed_at", integer()),
        ("facts", schema_ref("FactoryDomainFacts")),
      ],
      &[
        "id",
        "node_attempt_id",
        "flow_run_id",
        "cycle_id",
        "input_digest",
        "observed_at",
        "facts",
      ],
    ),
  );
  schemas.insert(
    "FactoryCallDiagnostic".to_owned(),
    object(
      [
        ("node", schema_ref("FactoryNodeIdentityDiagnostic")),
        (
          "profile",
          nullable(object(
            [
              ("selected_profile", nullable(schema_ref("FactoryRoleProfileReference"))),
              ("operation_id", schema_ref("FactoryDigest")),
              ("context_id", non_empty_string()),
              ("context_digest", schema_ref("FactoryDigest")),
              ("tool", schema_ref("FactoryImmutableReferenceResource")),
              ("plugin", schema_ref("FactoryImmutableReferenceResource")),
              ("model_or_tool", schema_ref("FactoryImmutableReferenceResource")),
              ("task_digest", schema_ref("FactoryDigest")),
              ("permission_digest", schema_ref("FactoryDigest")),
              (
                "build_configuration",
                schema_ref("FactoryBuildConfigurationReferenceResource"),
              ),
            ],
            &[
              "operation_id",
              "context_id",
              "context_digest",
              "tool",
              "plugin",
              "model_or_tool",
              "task_digest",
              "permission_digest",
              "build_configuration",
            ],
          )),
        ),
      ],
      &["node", "profile"],
    ),
  );
  schemas.insert(
    "FactoryBuildExecutionDiagnostic".into(),
    object(
      [
        ("id", non_empty_string()),
        ("operation_id", non_empty_string()),
        ("node_attempt_id", non_empty_string()),
        ("build_id", non_empty_string()),
        ("attempt_id", non_empty_string()),
        (
          "jobs",
          json!({"type":"array","minItems":1,"maxItems":64,"uniqueItems":true,"items":non_empty_string()}),
        ),
      ],
      &[
        "id",
        "operation_id",
        "node_attempt_id",
        "build_id",
        "attempt_id",
        "jobs",
      ],
    ),
  );
  let candidate = || schema_ref("FactoryCandidateSubject");
  let exact = || schema_ref("FactoryExactSubject");
  let id = non_empty_string;
  let digest = || schema_ref("FactoryDigest");
  let reference = || schema_ref("FactoryImmutableReferenceResource");
  let disposition = || json!({"oneOf":[{"type":"string","enum":["Allow","Deny","Escalate"]}, object([("Route",key())], &["Route"]),object([("Profile",key())], &["Profile"])]});
  let consumption = object(
    [
      ("baseline", disposition()),
      ("recommendation", nullable(disposition())),
      ("final_disposition", disposition()),
      ("kind", schema_ref("FactorySignalConsumptionKind")),
    ],
    &["baseline", "recommendation", "final_disposition", "kind"],
  );
  schemas.insert("FactoryDecisionReason".to_owned(), json!({"oneOf":[
    {"type":"string","const":"PolicySatisfied"},
    object([("RequiredEvidenceMissing",key())], &["RequiredEvidenceMissing"]),
    object([("RequiredEvidenceFailed",key())], &["RequiredEvidenceFailed"]),
    object([("RequiredEvidenceIndeterminate",key())], &["RequiredEvidenceIndeterminate"]),
    object([("RequiredAssessmentMissing",key())], &["RequiredAssessmentMissing"]),
    object([("AssessmentIndeterminate",key())], &["AssessmentIndeterminate"]),
    object([("SeverityThresholdExceeded",object([
      ("evaluator",key()),("severity",json!({"type":"string","enum":["advisory","low","medium","high","critical"]}))
    ], &["evaluator","severity"]))], &["SeverityThresholdExceeded"]),
    object([("QuorumNotMet",object([("required",positive_integer()),("observed",non_negative_integer())], &["required","observed"]))], &["QuorumNotMet"])
  ]}));
  schemas.insert("FactoryDecisionExplanation".to_owned(), object([
    ("plan_id",id()),
    ("assessment_ids",bounded_array(id(),octacity_server_application::MAX_DECISION_ASSESSMENTS)),
    ("signal_receipt_ids",bounded_array(id(),octacity_server_application::MAX_DECISION_SIGNAL_RECEIPTS)),
    ("reasons",json!({"type":"array","minItems":1,"maxItems":octacity_server_application::MAX_DECISION_REASONS,"items":schema_ref("FactoryDecisionReason")})),
    ("input_digest",digest()),("policy_version",positive_integer()),("policy_digest",digest()),
  ], &["plan_id","assessment_ids","signal_receipt_ids","reasons","input_digest","policy_version","policy_digest"]));
  schemas.insert("FactoryDomainFacts".to_owned(), json!({"oneOf":[
    tagged("kind","candidate",[("subject",candidate())], &["subject"]),
    tagged("kind","changeset",[("subject",candidate()),("id",id()),("node_attempt_id",id())], &["subject","id","node_attempt_id"]),
    tagged("kind","evidence",[("subject",candidate()),("id",id()),("changeset_id",id()),("digest",digest()),("item_count",non_negative_integer())], &["subject","id","changeset_id","digest","item_count"]),
    tagged("kind","review_plan",[("subject",candidate()),("id",id()),("evidence_id",id()),("digest",digest()),
      ("purpose",json!({"type":"string","enum":["requirements","implementation","code_review","verification","deployment"]})),
      ("required_quorum",positive_integer()),("deadline",integer())], &["subject","id","evidence_id","digest","purpose","required_quorum","deadline"]),
    tagged("kind","assessment",[("subject",candidate()),("id",id()),("plan_id",id()),("digest",digest()),
      ("evaluator",reference()),("model",reference()),("prompt_digest",digest()),
      ("outcome",json!({"type":"string","enum":["satisfied","violated","indeterminate"]}))], &["subject","id","plan_id","digest","evaluator","model","prompt_digest","outcome"]),
    tagged("kind","decision",[("subject",candidate()),("id",id()),("outcome",json!({"type":"string","enum":octacity_server_application::factory_decision_outcomes().collect::<Vec<_>>()})),
      ("explanation",schema_ref("FactoryDecisionExplanation"))], &["subject","id","outcome"]),
    tagged("kind","delivery",[("subject",candidate()),("id",id()),("decision_id",id()),("state",json!({"type":"string","enum":octacity_server_application::factory_delivery_states().collect::<Vec<_>>()}))], &["subject","id","decision_id","state"]),
    tagged("kind","escalation",[("subject",schema_ref("FactoryWorkSubject")),("id",id())], &["subject","id"]),
    tagged("kind","reporting",[("subject",schema_ref("FactoryWorkSubject")),("id",id()),("number",positive_integer()),("state",json!({"type":"string","enum":["succeeded","failed","unknown"]}))], &["subject","id","number","state"]),
    tagged("kind","promotion",[("subject",candidate()),("plan_digest",digest()),("input_digest",digest()),
      ("environment_contract",reference()),("deployment",reference()),("outcome",json!({"type":"string","enum":["passed","failed","indeterminate"]})),
      ("started_at",integer()),("finished_at",integer())], &["subject","plan_digest","input_digest","environment_contract","deployment","outcome","started_at","finished_at"]),
    tagged("kind","decision_signal",[("subject",exact()),("id",id()),("request_id",id()),("receipt_digest",digest()),("input_digest",digest()),
      ("provider",reference()),("adapter",reference()),("model",reference()),("policy",reference()),
      ("frozen_input_digest",digest()),("question_set",reference()),("probability_semantics",reference()),
      ("purpose",json!({"type":"string","enum":octacity_server_application::factory_signal_purposes().collect::<Vec<_>>()})),
      ("mode",json!({"type":"string","enum":octacity_server_application::factory_signal_modes().collect::<Vec<_>>()})),
      ("fallback",json!({"type":"string","enum":octacity_server_application::factory_signal_fallbacks().collect::<Vec<_>>()})),
      ("answers",bounded_array(schema_ref("FactorySignalAnswer"),bounds.signal_questions)),
      ("state",json!({"type":"string","enum":["completed","timed_out","invalid","unavailable","cancelled"]})),
      ("consumption",consumption)], &["subject","id","request_id","receipt_digest","input_digest","provider","adapter","model","policy","frozen_input_digest","question_set","probability_semantics","state","consumption"]),
  ]}));
  schemas.insert(
    "FactoryDiagnosticPage".to_owned(),
    object(
      [
        (
          "items",
          bounded_array(
            json!({"oneOf": [tagged("kind","profile_selection",[("record",schema_ref("FactoryProfileSelectionProjection"))], &["record"]),object(
              [
                ("kind", json!({"type":"string","const":"flow_run"})),
                (
                  "record",
                  object(
                    [
                      ("id", non_empty_string()),
                      ("factory_run_id", non_empty_string()),
                      ("definition", schema_ref("FactoryFlowDefinitionReference")),
                      (
                        "parent",
                        nullable(object(
                          [
                            ("flow_run_id", non_empty_string()),
                            ("node_attempt_id", non_empty_string()),
                          ],
                          &["flow_run_id", "node_attempt_id"],
                        )),
                      ),
                    ],
                    &["id", "factory_run_id", "definition", "parent"],
                  ),
                ),
              ],
              &["kind", "record"],
            ), tagged("kind", "workflow_cycle", [("record", object([
              ("id",non_empty_string()), ("flow_run_id",non_empty_string()),
              ("number",positive_integer()), ("predecessor",nullable(non_empty_string())),
              ("source_attempt_id",non_empty_string()),
            ], &["id","flow_run_id","number","predecessor"]))], &["record"]),
            tagged("kind", "node_attempt", [("record", schema_ref("FactoryNodeAttemptDiagnostic"))], &["record"]),
            tagged("kind", "build", [("record", schema_ref("FactoryBuildExecutionDiagnostic"))], &["record"]),
            tagged("kind", "budget", [("record", object([
              ("id",non_empty_string()), ("run_version",positive_integer()), ("usage",schema_ref("FactoryBudgetUsage")), ("recorded_at",integer()),
            ], &["id","run_version","usage","recorded_at"]))], &["record"]),
            tagged("kind", "audit", [("record", object([
              ("id",non_empty_string()), ("run_id",non_empty_string()),
              ("actor_kind",json!({"type":"string","enum":["unauthenticated_management","authenticated_management","agent","trigger","orchestrator","adapter","worker"]})),
              ("actor_identity_digest",nullable(non_empty_string())), ("operation",key()), ("request_identity_digest",non_empty_string()),
              ("outcome",key()), ("recorded_at",integer()),
            ], &["id","run_id","actor_kind","actor_identity_digest","operation","request_identity_digest","outcome","recorded_at"])) ], &["record"]),
            tagged("kind", "work", [("record", schema_ref("FactoryWorkDiagnostic"))], &["record"]),
            tagged("kind", "node_completion", [("record", schema_ref("FactoryNodeCompletionDiagnostic"))], &["record"]),
            tagged("kind", "effect_delivery", [("record", schema_ref("FactoryEffectDeliveryDiagnostic"))], &["record"]),
            tagged("kind", "node_stop", [("record", schema_ref("FactoryNodeStopDiagnostic"))], &["record"]),
            tagged("kind", "call", [("record", schema_ref("FactoryCallDiagnostic"))], &["record"]),
            tagged("kind", "domain_fact", [("record", schema_ref("FactoryDomainDiagnostic"))], &["record"])]}),
            usize::from(octacity_server_application::MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE),
          ),
        ),
        (
          "next_cursor",
          nullable(json!({"type":"string","minLength":1,"maxLength":256})),
        ),
      ],
      &["items", "next_cursor"],
    ),
  );
}
