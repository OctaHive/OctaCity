//! Journal contracts reuse common diagnostic, request and Job schemas; bodies are dedicated reads.
use super::super::*;
use super::factory::{bounded_array, key, tagged};

fn projection(source: &Value, fields: &[(&str, &str)]) -> Value {
  object(
    fields
      .iter()
      .map(|(to, from)| (*to, source["properties"][*from].clone())),
    &fields.iter().map(|(to, _)| *to).collect::<Vec<_>>(),
  )
}
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let id = non_empty_string;
  let cursor = || json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_ISSUE_OBSERVATION_CURSOR_BYTES});
  let maximum = usize::from(octacity_server_application::MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE);
  let mut request = schemas["IssueExecutionResource"].clone();
  for field in ["state", "run_url"] {
    request["properties"].as_object_mut().unwrap().remove(field);
    request["required"].as_array_mut().unwrap().retain(|name| name != field);
  }
  schemas.insert("IssueJournalExecutionRequest".into(), request);
  schemas.insert(
    "IssueRunProjection".into(),
    projection(
      &schemas["FactoryRunSummaryResource"],
      &[
        ("id", "id"),
        ("work_id", "work_id"),
        ("state", "state"),
        ("version", "version"),
        ("diagnostics", "diagnostics"),
        ("admitted_at", "admitted_at_unix_ms"),
        ("updated_at", "updated_at_unix_ms"),
      ],
    ),
  );
  schemas.insert(
    "IssueRunObservation".into(),
    json!({"oneOf":[
      tagged("availability","preparing",[],&[]),
      tagged("availability","unavailable",[],&[]),
      tagged("availability","available",[("resource",schema_ref("IssueRunProjection"))],&["resource"])
    ]}),
  );
  schemas.insert(
    "IssueExecutionProjection".into(),
    object(
      [
        ("request", schema_ref("IssueJournalExecutionRequest")),
        ("issue_version", positive_integer()),
        ("requirements", string_enum(&["current", "historical"])),
        ("run", schema_ref("IssueRunObservation")),
      ],
      &["request", "issue_version", "requirements", "run"],
    ),
  );
  schemas.insert(
    "IssueExecutionJournalPage".into(),
    object(
      [
        ("execution", schema_ref("IssueExecutionProjection")),
        ("records", nullable(schema_ref("IssueResultPage"))),
      ],
      &["execution", "records"],
    ),
  );
  let terminal = projection(
    &schemas["JobTerminalResource"],
    &[
      ("state", "state"),
      ("failure", "failure_classification"),
      ("completed_at", "completed_at_unix_ms"),
    ],
  );
  let mut job = projection(
    &schemas["JobResource"],
    &[
      ("id", "id"),
      ("state", "state"),
      ("version", "version"),
      ("created_at", "created_at_unix_ms"),
      ("updated_at", "updated_at_unix_ms"),
      ("assignment", "assignment"),
      ("terminal", "terminal"),
    ],
  );
  job["properties"]["terminal"] = nullable(terminal);
  schemas.insert("IssueJobProjection".into(), job);
  let profile = schemas["FactoryCallDiagnostic"]["properties"]["profile"].clone();
  schemas.insert(
    "IssueExecutionJobProjection".into(),
    object(
      [
        ("association", schema_ref("IssueWorkAssociation")),
        ("execution", schema_ref("FactoryBuildExecutionDiagnostic")),
        ("profile", profile),
        ("job", schema_ref("IssueJobProjection")),
        (
          "log",
          object(
            [
              ("build_id", id()),
              ("attempt_id", id()),
              ("job_id", id()),
              ("event_cursor", non_negative_integer()),
              (
                "events_url",
                json!({"type":"string","maxLength":128,"pattern":"^/api/v1/jobs/[0-9a-f-]{36}/events$"}),
              ),
            ],
            &["build_id", "attempt_id", "job_id", "event_cursor", "events_url"],
          ),
        ),
      ],
      &["association", "execution", "profile", "job", "log"],
    ),
  );
  let path_maximum = octacity_server_application::MAX_PUBLIC_OPERATION_PATH_BYTES;
  schemas.insert("PublicOperationObservation".into(),object([
    ("operation_id",schema_ref("FactoryDigest")),("phase",string_enum(&["started","completed"])),
    ("observed_at_unix_ms",non_negative_integer()),("operation",json!({"oneOf":[
      tagged("kind","command",[("exit_code",nullable(integer())),("failed",nullable(boolean()))],&["exit_code","failed"]),
      tagged("kind","file",[("path",json!({"type":"string","minLength":1,"maxLength":path_maximum,"x-max-utf8-bytes":path_maximum})),
        ("change",string_enum(&["created","modified","deleted"])),("failed",nullable(boolean()))],&["path","change","failed"])
    ]}))
  ],&["operation_id","phase","observed_at_unix_ms","operation"]));
  let fields = [
    "observed_at_unix_ms",
    "elapsed_ms",
    "cpu_time_ms",
    "memory_current_bytes",
    "memory_peak_bytes",
    "disk_current_bytes",
    "disk_peak_bytes",
    "io_read_bytes",
    "io_written_bytes",
    "network_received_bytes",
    "network_transmitted_bytes",
  ];
  schemas.insert(
    "IssueJournalResourceUsage".into(),
    object(
      fields.into_iter().map(|field| {
        (
          field,
          if field.starts_with("network_") {
            nullable(non_negative_integer())
          } else {
            non_negative_integer()
          },
        )
      }),
      &fields,
    ),
  );
  schemas.insert("IssueJournalEventFacts".into(), json!({"oneOf":[
    tagged("kind","operation",[("observation",schema_ref("PublicOperationObservation")),("runner_sequence",positive_integer()),
      ("run_id",nullable(non_negative_integer())),("step_id",nullable(non_negative_integer())),("command_id",nullable(key()))],
      &["observation","runner_sequence","run_id","step_id","command_id"]),
    tagged("kind","lifecycle",[("state",string_enum(&["preparing","running","freezing","uploading","cleaning","completing"]))],&["state"]),
    tagged("kind","resource_usage",[("usage",schema_ref("IssueJournalResourceUsage"))],&["usage"]),
    tagged("kind","not_reported",[],&[])
  ]}));
  schemas.insert(
    "IssueJournalEvent".into(),
    object(
      [
        ("sequence", positive_integer()),
        ("occurred_at_unix_ms", non_negative_integer()),
        ("facts", schema_ref("IssueJournalEventFacts")),
        (
          "detail_url",
          json!({"type":"string","maxLength":512,"pattern":"^/api/v1/projects/"}),
        ),
      ],
      &["sequence", "occurred_at_unix_ms", "facts", "detail_url"],
    ),
  );
  schemas.insert(
    "IssueJournalGap".into(),
    object(
      [
        ("from_sequence", positive_integer()),
        ("through_sequence", positive_integer()),
      ],
      &["from_sequence", "through_sequence"],
    ),
  );
  schemas.insert(
    "IssueExecutionJournalEvents".into(),
    object(
      [
        ("job", schema_ref("IssueExecutionJobProjection")),
        (
          "observation",
          json!({"type":"string","const":"issue-job-event-sequence-v1"}),
        ),
        ("coverage", schema_ref("IssueResultCoverage")),
        ("job_stale", boolean()),
        ("cursor", cursor()),
        ("events", bounded_array(schema_ref("IssueJournalEvent"), maximum)),
        ("gaps", bounded_array(schema_ref("IssueJournalGap"), maximum + 1)),
      ],
      &[
        "job",
        "observation",
        "coverage",
        "job_stale",
        "cursor",
        "events",
        "gaps",
      ],
    ),
  );
  let mut detail = schemas["JobEventPage"].clone();
  detail["properties"]["items"]["maxItems"] = json!(1);
  schemas.insert("IssueExecutionJournalEventDetail".into(), detail);
}
