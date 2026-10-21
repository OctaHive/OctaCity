//! Exact common operator contracts, with no private inputs or command response bodies.
use super::super::*;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let id = || json!({"type":"string","format":"uuid"});
  schemas.insert(
    "FactoryNodeActionObservation".into(),
    object(
      [
        ("run_id", id()),
        ("node_attempt_id", id()),
        ("run_version", positive_integer()),
        ("state", string_enum(&["waiting", "recorded", "consumed", "historical"])),
        ("description", nullable(schema_ref("FactoryNodeActionDescription"))),
        ("accepted_candidate", boolean()),
        ("recorded_version", nullable(positive_integer())),
        ("recorded_at", nullable(integer())),
        ("observed_at", integer()),
      ],
      &[
        "run_id",
        "node_attempt_id",
        "run_version",
        "state",
        "description",
        "accepted_candidate",
        "recorded_version",
        "recorded_at",
        "observed_at",
      ],
    ),
  );
  let bounds = octacity_server_application::factory_configuration_document_bounds();
  schemas.insert("FactoryActionCoverage".into(), string_enum(&["complete", "incomplete"]));
  let respond = object(
    [
      ("kind", json!({"const":"respond","type":"string"})),
      (
        "outcomes",
        json!({"type":"array","maxItems":bounds.schema_entries,"items":schema_ref("FactoryFlowOutcome")}),
      ),
      ("binding", nullable(schema_ref("FactoryFlowHumanGateBinding"))),
      (
        "schemas",
        json!({"type":"array","maxItems":bounds.schema_entries,"items":schema_ref("FactoryFlowDataSchema")}),
      ),
    ],
    &["kind", "outcomes", "binding", "schemas"],
  );
  let retry = object([("kind", json!({"const":"retry","type":"string"}))], &["kind"]);
  schemas.insert("FactoryNodeActionKind".into(), json!({"oneOf":[respond,retry]}));
  let fields = [
    ("work_id", id()),
    ("run_id", id()),
    ("run_version", positive_integer()),
    ("flow_run_id", id()),
    ("cycle_id", id()),
    ("node_attempt_id", id()),
    ("contract", schema_ref("FactoryFlowDefinitionReference")),
    ("node_key", non_empty_string()),
    ("input_digest", schema_ref("FactoryDigest")),
    ("candidate", nullable(schema_ref("FactoryCandidateSubject"))),
    ("actionable_at", integer()),
    ("deadline", nullable(integer())),
    ("kind", schema_ref("FactoryNodeActionKind")),
  ];
  let required = fields.iter().map(|(name, _)| *name).collect::<Vec<_>>();
  schemas.insert("FactoryNodeActionDescription".into(), object(fields, &required));
  schemas.insert(
    "FactoryActionQueueItem".into(),
    object(
      [
        ("project_id", id()),
        ("association", nullable(schema_ref("IssueWorkAssociation"))),
        ("action", schema_ref("FactoryNodeActionDescription")),
      ],
      &["project_id", "association", "action"],
    ),
  );
  schemas.insert(
    "FactoryActionsPage".into(),
    object(
      [
        ("total", non_negative_integer()),
        ("coverage", schema_ref("FactoryActionCoverage")),
        ("observed_at", integer()),
        ("items", json!({"type":"array","maxItems":octacity_server_application::MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE,"items":schema_ref("FactoryActionQueueItem")})),
        ("next_cursor", nullable(json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_ISSUE_OBSERVATION_CURSOR_BYTES}))),
      ],
      &["total", "coverage", "observed_at", "items", "next_cursor"],
    ),
  );
}
