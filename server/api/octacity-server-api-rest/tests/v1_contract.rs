use axum::http::HeaderValue;
use octacity_server_api_rest::v1::{
  AcceptManualTriggerRequest, AgentPoolResource, AgentResource, ArtifactDownload, ArtifactOutputType, ArtifactResource,
  BuildLogSearchPage, CacheSessionResource, CacheSessionState, ContractValueError, CreateAgentPoolRequest,
  CreateBuildConfigurationRequest, CreateManagedWebhookRequest, CreatePipelineRequest, CreateProjectRequest,
  CreateScheduledTriggerDefinitionRequest, Cursor, CursorPage, DrainAgentRequest, ErrorCode, ErrorResponse,
  IdempotencyKey, InternalTriggerDefinitionRequest, IssueAgentEnrollmentRequest, IssueAgentEnrollmentResponse,
  MAX_CURSOR_BYTES, MAX_IDEMPOTENCY_KEY_BYTES, OperationalMetadata, ProjectResource, VersionPrecondition,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

const CREATE_PROJECT: &str = include_str!("../fixtures/v1/create-project-request.json");
const PROJECT_PAGE: &str = include_str!("../fixtures/v1/project-page.json");
const CREATE_PIPELINE: &str = include_str!("../fixtures/v1/create-pipeline-request.json");
const CREATE_CONFIGURATION: &str = include_str!("../fixtures/v1/create-build-configuration-request.json");
const ACCEPT_MANUAL_TRIGGER: &str = include_str!("../fixtures/v1/accept-manual-trigger-request.json");
const CREATE_SCHEDULED_TRIGGER: &str = include_str!("../fixtures/v1/create-scheduled-trigger-request.json");
const CREATE_INTERNAL_TRIGGER: &str = include_str!("../fixtures/v1/create-internal-trigger-request.json");
const CREATE_MANAGED_WEBHOOK: &str = include_str!("../fixtures/v1/create-managed-webhook-request.json");
const ERROR_RESPONSE: &str = include_str!("../fixtures/v1/error-response.json");
const CREATE_AGENT_POOL: &str = include_str!("../fixtures/v1/create-agent-pool-request.json");
const AGENT_POOL_PAGE: &str = include_str!("../fixtures/v1/agent-pool-page.json");
const AGENT_PAGE: &str = include_str!("../fixtures/v1/agent-page.json");
const ISSUE_AGENT_ENROLLMENT: &str = include_str!("../fixtures/v1/issue-agent-enrollment-request.json");
const ISSUED_AGENT_ENROLLMENT: &str = include_str!("../fixtures/v1/issue-agent-enrollment-response.json");
const BUILD_LOG_SEARCH_PAGE: &str = include_str!("../fixtures/v1/build-log-search-page.json");

#[test]
fn v1_golden_documents_round_trip_without_application_types() {
  assert_golden::<CreateProjectRequest>(CREATE_PROJECT);
  assert_golden::<CursorPage<ProjectResource>>(PROJECT_PAGE);
  assert_golden::<CreatePipelineRequest>(CREATE_PIPELINE);
  assert_golden::<CreateBuildConfigurationRequest>(CREATE_CONFIGURATION);
  assert_golden::<AcceptManualTriggerRequest>(ACCEPT_MANUAL_TRIGGER);
  assert_golden::<CreateScheduledTriggerDefinitionRequest>(CREATE_SCHEDULED_TRIGGER);
  assert_golden::<InternalTriggerDefinitionRequest>(CREATE_INTERNAL_TRIGGER);
  assert_golden::<CreateManagedWebhookRequest>(CREATE_MANAGED_WEBHOOK);
  assert_golden::<ErrorResponse>(ERROR_RESPONSE);
  assert_golden::<CreateAgentPoolRequest>(CREATE_AGENT_POOL);
  assert_golden::<CursorPage<AgentPoolResource>>(AGENT_POOL_PAGE);
  assert_golden::<CursorPage<AgentResource>>(AGENT_PAGE);
  assert_golden::<IssueAgentEnrollmentRequest>(ISSUE_AGENT_ENROLLMENT);
  assert_golden::<IssueAgentEnrollmentResponse>(ISSUED_AGENT_ENROLLMENT);
  assert_golden::<BuildLogSearchPage>(BUILD_LOG_SEARCH_PAGE);
}

#[test]
fn agent_enrollment_response_debug_output_redacts_the_bearer() {
  let response: IssueAgentEnrollmentResponse = serde_json::from_str(ISSUED_AGENT_ENROLLMENT).unwrap();
  assert!(!format!("{response:?}").contains(&response.credential));
}

#[test]
fn managed_webhook_request_debug_output_redacts_protected_handles() {
  let request: CreateManagedWebhookRequest = serde_json::from_str(CREATE_MANAGED_WEBHOOK).unwrap();
  let debug = format!("{request:?}");
  assert!(!debug.contains(&request.verification_material_handle));
  assert!(!debug.contains(&request.administration_credential_handle));
}

#[test]
fn artifact_download_is_strict_and_redacts_its_short_lived_capability() {
  let download = ArtifactDownload {
    artifact: ArtifactResource {
      id: "artifact-1".to_owned(),
      build_id: "build-1".to_owned(),
      attempt_id: "attempt-1".to_owned(),
      job_id: "job-1".to_owned(),
      name: "report.json".to_owned(),
      output_type: ArtifactOutputType::Report {
        format: "plugin.example/report-v2".to_owned(),
      },
      media_type: "application/json".to_owned(),
      size_bytes: 42,
      sha256: "ab".repeat(32),
      published_at_unix_ms: 1_700_000_000_000,
    },
    get_url: "https://storage.invalid/short-lived-secret".to_owned(),
    expires_at_unix_ms: 1_700_000_060_000,
  };

  assert!(!format!("{download:?}").contains(&download.get_url));
  let mut value = serde_json::to_value(download).unwrap();
  value["object_key"] = json!("physical/provider/key");
  assert!(serde_json::from_value::<ArtifactDownload>(value).is_err());
}

#[test]
fn cache_session_diagnostics_are_strict_and_contain_no_credential_material() {
  let diagnostic = CacheSessionResource {
    id: "cache-session-1".to_owned(),
    project_id: "project-1".to_owned(),
    build_id: "build-1".to_owned(),
    job_id: "job-1".to_owned(),
    agent_id: "agent-1".to_owned(),
    registration_epoch: 1,
    lease_id: "lease-1".to_owned(),
    namespace: "project/main".to_owned(),
    read: true,
    write: false,
    quota_bytes: 1_024,
    created_at_unix_ms: 1_000,
    expires_at_unix_ms: 2_000,
    retention_until_unix_ms: 3_000,
    state: CacheSessionState::Active,
    revoked_at_unix_ms: None,
  };
  let mut value = serde_json::to_value(diagnostic).unwrap();
  for forbidden in [
    "credential",
    "credential_hash",
    "bearer_token",
    "scope_id",
    "fencing_token",
  ] {
    assert!(value.get(forbidden).is_none());
  }
  value["bearer_token"] = json!("secret");
  assert!(serde_json::from_value::<CacheSessionResource>(value).is_err());
}

#[test]
fn build_log_search_results_are_strict_and_backend_neutral() {
  let page: BuildLogSearchPage = serde_json::from_str(BUILD_LOG_SEARCH_PAGE).unwrap();
  assert!(!page.freshness.caught_up);

  let mut value = serde_json::to_value(page).unwrap();
  let hit = value["items"][0].as_object_mut().unwrap();
  for forbidden in ["bucket", "object_key", "tsvector", "postgres_rank"] {
    assert!(hit.get(forbidden).is_none());
  }
  hit.insert("object_key".to_owned(), json!("private/log/chunk"));
  assert!(serde_json::from_value::<BuildLogSearchPage>(value).is_err());
}

#[test]
fn every_v1_command_and_envelope_rejects_unknown_fields() {
  let mut create_project: Value = serde_json::from_str(CREATE_PROJECT).unwrap();
  create_project["database_row_version"] = json!(7);
  assert!(serde_json::from_value::<CreateProjectRequest>(create_project).is_err());

  let mut project_page: Value = serde_json::from_str(PROJECT_PAGE).unwrap();
  project_page["items"][0]["private_transfer_url"] = json!("https://storage.invalid/secret");
  assert!(serde_json::from_value::<CursorPage<ProjectResource>>(project_page).is_err());

  let mut pipeline: Value = serde_json::from_str(CREATE_PIPELINE).unwrap();
  pipeline["dag"]["nodes"][0]["execution"]["secret_profile"] = json!("production");
  assert!(serde_json::from_value::<CreatePipelineRequest>(pipeline).is_err());

  let mut configuration: Value = serde_json::from_str(CREATE_CONFIGURATION).unwrap();
  configuration["definition"]["runtime"]["host_path"] = json!("/private/build");
  assert!(serde_json::from_value::<CreateBuildConfigurationRequest>(configuration).is_err());

  let mut trigger: Value = serde_json::from_str(ACCEPT_MANUAL_TRIGGER).unwrap();
  trigger["accepted_at_unix_ms"] = json!(1_700_000_000_000_i64);
  assert!(serde_json::from_value::<AcceptManualTriggerRequest>(trigger).is_err());

  let mut schedule: Value = serde_json::from_str(CREATE_SCHEDULED_TRIGGER).unwrap();
  schedule["schedule"]["database_timezone"] = json!("UTC");
  assert!(serde_json::from_value::<CreateScheduledTriggerDefinitionRequest>(schedule).is_err());

  let mut internal: Value = serde_json::from_str(CREATE_INTERNAL_TRIGGER).unwrap();
  internal["source"]["fallback_revision"] = json!("must-not-be-accepted");
  assert!(serde_json::from_value::<InternalTriggerDefinitionRequest>(internal).is_err());

  let mut managed_webhook: Value = serde_json::from_str(CREATE_MANAGED_WEBHOOK).unwrap();
  managed_webhook["provider_private_configuration"] = json!({"token": "must-never-be-accepted"});
  assert!(serde_json::from_value::<CreateManagedWebhookRequest>(managed_webhook).is_err());

  let mut error: Value = serde_json::from_str(ERROR_RESPONSE).unwrap();
  error["diagnostic"] = json!("provider-private-details");
  assert!(serde_json::from_value::<ErrorResponse>(error).is_err());

  let mut pool: Value = serde_json::from_str(CREATE_AGENT_POOL).unwrap();
  pool["definition"]["provider_template"] = json!("private-infrastructure-detail");
  assert!(serde_json::from_value::<CreateAgentPoolRequest>(pool).is_err());

  let mut pool_page: Value = serde_json::from_str(AGENT_POOL_PAGE).unwrap();
  pool_page["items"][0]["active_lease_ids"] = json!([]);
  assert!(serde_json::from_value::<CursorPage<AgentPoolResource>>(pool_page).is_err());

  let mut agent_page: Value = serde_json::from_str(AGENT_PAGE).unwrap();
  agent_page["items"][0]["registration_credential"] = json!("must-never-leak");
  assert!(serde_json::from_value::<CursorPage<AgentResource>>(agent_page).is_err());

  assert!(serde_json::from_value::<DrainAgentRequest>(json!({"mode": "graceful", "command": "shell"})).is_err());

  let mut enrollment: Value = serde_json::from_str(ISSUE_AGENT_ENROLLMENT).unwrap();
  enrollment["registration_credential"] = json!("must-never-be-accepted");
  assert!(serde_json::from_value::<IssueAgentEnrollmentRequest>(enrollment).is_err());
}

#[test]
fn optional_pipeline_execution_fields_accept_explicit_null() {
  let mut pipeline: Value = serde_json::from_str(CREATE_PIPELINE).unwrap();
  pipeline["dag"]["nodes"][0]["execution"]["octafile"] = Value::Null;
  pipeline["dag"]["nodes"][0]["execution"]["concurrency"] = Value::Null;
  assert!(serde_json::from_value::<CreatePipelineRequest>(pipeline).is_ok());
}

#[test]
fn operational_metadata_is_strict_and_keeps_ingress_security_independent() {
  let metadata = OperationalMetadata::trusted_network(true, true, true, false);
  let value = serde_json::to_value(&metadata).unwrap();
  assert_eq!(
    value["security"]["management"]["mode"],
    "trusted_network_unauthenticated"
  );
  assert_eq!(value["security"]["management"]["operator_authentication"], false);
  assert_eq!(value["security"]["agent"]["mode"], "registration_credentials");
  assert_eq!(value["security"]["webhook"]["mode"], "provider_verification");
  assert_eq!(value["ingress"]["agent_enabled"], true);
  assert_eq!(value["ingress"]["webhook_enabled"], false);

  let mut invalid = value;
  invalid["security"]["management"]["operator_token"] = json!("must-never-exist");
  assert!(serde_json::from_value::<OperationalMetadata>(invalid).is_err());
}

#[test]
fn stable_error_codes_have_exact_v1_wire_names() {
  let cases = [
    (ErrorCode::InvalidRequest, "invalid_request"),
    (ErrorCode::UnsupportedMediaType, "unsupported_media_type"),
    (ErrorCode::UnsupportedApiVersion, "unsupported_api_version"),
    (ErrorCode::PayloadTooLarge, "payload_too_large"),
    (ErrorCode::InvalidIdempotencyKey, "invalid_idempotency_key"),
    (ErrorCode::IdempotencyConflict, "idempotency_conflict"),
    (ErrorCode::PreconditionRequired, "precondition_required"),
    (ErrorCode::PreconditionFailed, "precondition_failed"),
    (ErrorCode::NotFound, "not_found"),
    (ErrorCode::Conflict, "conflict"),
    (ErrorCode::CapabilityUnavailable, "capability_unavailable"),
    (ErrorCode::Unavailable, "unavailable"),
    (ErrorCode::RateLimited, "rate_limited"),
    (ErrorCode::Internal, "internal"),
  ];
  for (code, expected) in cases {
    assert_eq!(serde_json::to_value(code).unwrap(), json!(expected));
  }
}

#[test]
fn idempotency_header_is_bounded_visible_and_lossless() {
  let key = IdempotencyKey::from_header(&HeaderValue::from_static("create-project-42")).unwrap();
  assert_eq!(key.as_str(), "create-project-42");
  assert_eq!(key.to_header_value(), HeaderValue::from_static("create-project-42"));

  for invalid in ["", " leading", "trailing "] {
    assert_eq!(
      invalid.parse::<IdempotencyKey>(),
      Err(ContractValueError::InvalidIdempotencyKey)
    );
  }
  assert_eq!(
    "x".repeat(MAX_IDEMPOTENCY_KEY_BYTES + 1).parse::<IdempotencyKey>(),
    Err(ContractValueError::InvalidIdempotencyKey)
  );
}

#[test]
fn optimistic_precondition_accepts_only_one_canonical_strong_etag() {
  let precondition = VersionPrecondition::from_header(&HeaderValue::from_static("\"42\"")).unwrap();
  assert_eq!(precondition.version(), 42);
  assert_eq!(precondition.to_header_value(), HeaderValue::from_static("\"42\""));

  for invalid in ["42", "\"0\"", "\"042\"", "W/\"42\"", "*", "\"41\", \"42\""] {
    assert_eq!(
      invalid.parse::<VersionPrecondition>(),
      Err(ContractValueError::InvalidVersionPrecondition)
    );
  }
}

#[test]
fn opaque_cursors_are_bounded_before_query_dispatch() {
  assert_eq!(Cursor::new("next-page").unwrap().as_str(), "next-page");
  assert_eq!(Cursor::new("with space"), Err(ContractValueError::InvalidCursor));
  assert_eq!(
    Cursor::new("x".repeat(MAX_CURSOR_BYTES + 1)),
    Err(ContractValueError::InvalidCursor)
  );
}

fn assert_golden<T>(document: &str)
where
  T: DeserializeOwned + Serialize,
{
  let decoded: T = serde_json::from_str(document).unwrap();
  let actual = serde_json::to_value(decoded).unwrap();
  let expected: Value = serde_json::from_str(document).unwrap();
  assert_eq!(actual, expected);
}
