//! In-process coordinator endpoints used by the Phase 6 service contract.
//!
//! This fixture deliberately models only the fenced, idempotent upload
//! protocol. S3 behavior stays behind the production `ArtifactStore` adapter;
//! Vault, runner, and lifecycle setup remain in the parent scenario.

use std::{
  collections::HashMap,
  sync::{Arc, Mutex},
  time::Duration,
};

use axum::{
  Json, Router,
  extract::{Path, State},
  http::{HeaderMap, StatusCode},
  routing::post,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer as _, SigningKey};
use octacity_artifact_store::{ArtifactObject, ArtifactStore};
use octacity_coordinator::Registration;
use octacity_protocol::{
  BeginOutputUploadRequest, BeginOutputUploadResponse, COORDINATOR_PROTOCOL_VERSION, CompleteOutputUploadRequest,
  CompleteOutputUploadResponse, CoordinatorErrorResponse, JobSpecV1, LeaseAssignment, LeaseFence, OutputUploadMetadata,
  SIGNATURE_ALGORITHM, SignedEnvelope,
};
use sha2::{Digest as _, Sha256};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::{TOKEN, unix_now};

#[derive(Clone)]
pub(super) struct UploadServerState {
  pub(super) store: Arc<dyn ArtifactStore>,
  pub(super) registration_id: String,
  pub(super) fence: LeaseFence,
  pub(super) records: Arc<Mutex<UploadRecords>>,
}

#[derive(Default)]
pub(super) struct UploadRecords {
  by_key: HashMap<String, String>,
  pub(super) by_upload_id: HashMap<String, UploadRecord>,
}

#[derive(Clone)]
pub(super) struct UploadRecord {
  pub(super) object: ArtifactObject,
  pub(super) metadata: OutputUploadMetadata,
  registration_id: String,
  fence: LeaseFence,
  pub(super) completed: bool,
}

impl UploadRecord {
  fn belongs_to(&self, registration_id: &str, fence: &LeaseFence) -> bool {
    self.registration_id == registration_id && &self.fence == fence
  }
}

pub(super) async fn start_server(state: UploadServerState) -> (String, CancellationToken) {
  let router = Router::new()
    .route("/api/v1/leases/{lease_id}/artifacts:begin", post(begin_upload))
    .route("/api/v1/leases/{lease_id}/artifacts:complete", post(complete_upload))
    .with_state(state);
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let origin = format!("http://{}", listener.local_addr().unwrap());
  let shutdown = CancellationToken::new();
  let server_shutdown = shutdown.clone();
  tokio::spawn(async move {
    axum::serve(listener, router)
      .with_graceful_shutdown(server_shutdown.cancelled_owned())
      .await
      .unwrap();
  });
  (origin, shutdown)
}

async fn begin_upload(
  State(state): State<UploadServerState>,
  Path(lease_id): Path<String>,
  headers: HeaderMap,
  Json(request): Json<BeginOutputUploadRequest>,
) -> ApiResult<BeginOutputUploadResponse> {
  authorize(
    &state,
    &lease_id,
    &headers,
    &request.registration_id,
    &request.lease,
    &request.request_id,
  )?;
  request
    .validate()
    .map_err(|_| rejection(StatusCode::BAD_REQUEST, &request.request_id, "invalid_request"))?;
  // Reserve the idempotency key before awaiting object-store authorization.
  // Concurrent begin calls can never overwrite the same logical record with
  // different metadata.
  let record = {
    let mut records = state.records.lock().unwrap();
    if let Some(existing) = records
      .by_key
      .get(&request.upload_key)
      .and_then(|upload_id| records.by_upload_id.get(upload_id))
      .cloned()
    {
      if existing.metadata != request.output || !existing.belongs_to(&request.registration_id, &request.lease) {
        return Err(rejection(
          StatusCode::CONFLICT,
          &request.request_id,
          "idempotency_conflict",
        ));
      }
      existing
    } else {
      let identity = format!("{:x}", Sha256::digest(request.upload_key.as_bytes()));
      let record = UploadRecord {
        object: ArtifactObject {
          artifact_id: format!("artifact-{}", &identity[..32]),
          upload_id: format!("upload-{}", &identity[32..]),
          size_bytes: request.output.size_bytes,
          sha256: request.output.sha256.clone(),
          content_type: request.output.transport_content_type.clone(),
        },
        metadata: request.output.clone(),
        registration_id: request.registration_id.clone(),
        fence: request.lease.clone(),
        completed: false,
      };
      records
        .by_key
        .insert(request.upload_key.clone(), record.object.upload_id.clone());
      records
        .by_upload_id
        .insert(record.object.upload_id.clone(), record.clone());
      record
    }
  };
  let authorization = state
    .store
    .authorize_upload(&record.object, Duration::from_secs(60))
    .await
    .map_err(|_| rejection(StatusCode::BAD_GATEWAY, &request.request_id, "storage_failed"))?;
  Ok(Json(BeginOutputUploadResponse {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: request.request_id,
    upload_id: record.object.upload_id,
    put_url: authorization.url,
    required_headers: authorization.required_headers,
    expires_at: unix_now() + authorization.expires_in.as_secs(),
  }))
}

async fn complete_upload(
  State(state): State<UploadServerState>,
  Path(lease_id): Path<String>,
  headers: HeaderMap,
  Json(request): Json<CompleteOutputUploadRequest>,
) -> ApiResult<CompleteOutputUploadResponse> {
  authorize(
    &state,
    &lease_id,
    &headers,
    &request.registration_id,
    &request.lease,
    &request.request_id,
  )?;
  request
    .validate()
    .map_err(|_| rejection(StatusCode::BAD_REQUEST, &request.request_id, "invalid_request"))?;
  let record = state
    .records
    .lock()
    .unwrap()
    .by_upload_id
    .get(&request.upload_id)
    .cloned()
    .ok_or_else(|| rejection(StatusCode::NOT_FOUND, &request.request_id, "unknown_upload"))?;
  if !record.belongs_to(&request.registration_id, &request.lease) {
    return Err(rejection(StatusCode::CONFLICT, &request.request_id, "lease_fenced"));
  }
  state
    .store
    .complete_upload(&record.object)
    .await
    .map_err(|_| rejection(StatusCode::BAD_GATEWAY, &request.request_id, "storage_failed"))?;
  state
    .records
    .lock()
    .unwrap()
    .by_upload_id
    .get_mut(&request.upload_id)
    .expect("the upload record cannot disappear")
    .completed = true;
  Ok(Json(CompleteOutputUploadResponse {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: request.request_id,
    upload_id: request.upload_id,
  }))
}

fn authorize(
  state: &UploadServerState,
  lease_id: &str,
  headers: &HeaderMap,
  registration_id: &str,
  fence: &LeaseFence,
  request_id: &str,
) -> Result<(), ApiError> {
  let bearer = headers.get("authorization").and_then(|value| value.to_str().ok());
  if bearer != Some(&format!("Bearer {TOKEN}")) {
    return Err(rejection(StatusCode::UNAUTHORIZED, request_id, "unauthorized"));
  }
  if registration_id != state.registration_id || lease_id != state.fence.lease_id || fence != &state.fence {
    return Err(rejection(StatusCode::CONFLICT, request_id, "lease_fenced"));
  }
  Ok(())
}

type ApiResult<T> = Result<Json<T>, ApiError>;
type ApiError = (StatusCode, Json<CoordinatorErrorResponse>);

fn rejection(status: StatusCode, request_id: &str, code: &str) -> ApiError {
  (
    status,
    Json(CoordinatorErrorResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: request_id.to_owned(),
      code: code.to_owned(),
      message: "protocol test server rejected the request".to_owned(),
      retryable: false,
      retry_after_ms: None,
    }),
  )
}

pub(super) fn registration() -> Registration {
  Registration {
    agent_id: "agent-1".to_owned(),
    registration_id: "registration-1".to_owned(),
    max_retry_delay: Duration::from_secs(1),
  }
}

pub(super) fn lease(spec: &JobSpecV1) -> LeaseAssignment {
  let signing = SigningKey::from_bytes(&[7_u8; 32]);
  let payload = serde_json::to_vec(spec).unwrap();
  LeaseAssignment {
    lease_id: "lease-1".to_owned(),
    job_id: spec.job_id.clone(),
    attempt: spec.attempt,
    fencing_token: "fence-1".to_owned(),
    issued_at: unix_now().saturating_sub(1),
    expires_at: unix_now() + 300,
    signed_job_spec: SignedEnvelope {
      key_id: "phase6-key".to_owned(),
      algorithm: SIGNATURE_ALGORITHM.to_owned(),
      payload: STANDARD.encode(&payload),
      signature: STANDARD.encode(signing.sign(&payload).to_bytes()),
    },
  }
}

#[test]
fn upload_records_are_bound_to_the_registration_and_lease_fence() {
  let record = UploadRecord {
    object: ArtifactObject {
      artifact_id: "artifact-1".to_owned(),
      upload_id: "upload-1".to_owned(),
      size_bytes: 0,
      sha256: format!("{:x}", Sha256::digest([])),
      content_type: "application/octet-stream".to_owned(),
    },
    metadata: OutputUploadMetadata {
      run_id: 1,
      task_id: 1,
      kind: octacity_protocol::OutputKind::Artifact,
      name: "artifact".to_owned(),
      content_type: Some("application/octet-stream".to_owned()),
      report_format: None,
      transport_content_type: "application/octet-stream".to_owned(),
      size_bytes: 0,
      sha256: format!("{:x}", Sha256::digest([])),
    },
    registration_id: "registration-1".to_owned(),
    fence: LeaseFence {
      lease_id: "lease-1".to_owned(),
      job_id: "job-1".to_owned(),
      attempt: 1,
      fencing_token: "fence-1".to_owned(),
    },
    completed: false,
  };

  assert!(record.belongs_to("registration-1", &record.fence));
  let mut newer_attempt = record.fence.clone();
  newer_attempt.attempt = 2;
  newer_attempt.fencing_token = "fence-2".to_owned();
  assert!(!record.belongs_to("registration-1", &newer_attempt));
  assert!(!record.belongs_to("registration-2", &record.fence));
}
