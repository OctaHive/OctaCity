use axum::http::HeaderValue;
use octacity_server_api_rest::v1::{
  AcceptManualTriggerRequest, ContractValueError, CreateBuildConfigurationRequest, CreatePipelineRequest,
  CreateProjectRequest, Cursor, CursorPage, ErrorCode, ErrorResponse, IdempotencyKey, MAX_CURSOR_BYTES,
  MAX_IDEMPOTENCY_KEY_BYTES, OperationalMetadata, ProjectResource, VersionPrecondition,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

const CREATE_PROJECT: &str = include_str!("../fixtures/v1/create-project-request.json");
const PROJECT_PAGE: &str = include_str!("../fixtures/v1/project-page.json");
const CREATE_PIPELINE: &str = include_str!("../fixtures/v1/create-pipeline-request.json");
const CREATE_CONFIGURATION: &str = include_str!("../fixtures/v1/create-build-configuration-request.json");
const ACCEPT_MANUAL_TRIGGER: &str = include_str!("../fixtures/v1/accept-manual-trigger-request.json");
const ERROR_RESPONSE: &str = include_str!("../fixtures/v1/error-response.json");

#[test]
fn v1_golden_documents_round_trip_without_application_types() {
  assert_golden::<CreateProjectRequest>(CREATE_PROJECT);
  assert_golden::<CursorPage<ProjectResource>>(PROJECT_PAGE);
  assert_golden::<CreatePipelineRequest>(CREATE_PIPELINE);
  assert_golden::<CreateBuildConfigurationRequest>(CREATE_CONFIGURATION);
  assert_golden::<AcceptManualTriggerRequest>(ACCEPT_MANUAL_TRIGGER);
  assert_golden::<ErrorResponse>(ERROR_RESPONSE);
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

  let mut error: Value = serde_json::from_str(ERROR_RESPONSE).unwrap();
  error["diagnostic"] = json!("provider-private-details");
  assert!(serde_json::from_value::<ErrorResponse>(error).is_err());
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
