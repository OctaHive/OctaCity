use std::{future::Future, sync::Arc, time::Duration};

use async_trait::async_trait;
use octacity_artifact_store::{ArtifactObject, ArtifactStore, ArtifactStoreError};
use octacity_observability::{ErrorClass, Operation, ServerOperationMetric};
use octacity_protocol::{
  AgentCredentialToken, BeginOutputUploadRequest, BeginOutputUploadResponse, COORDINATOR_PROTOCOL_VERSION,
  CompleteOutputUploadRequest, CompleteOutputUploadResponse, OutputKind,
};
use octacity_server_domain::{ArtifactId, ArtifactName, ArtifactUploadId, BuildId, EntityKind, Timestamp};
use octacity_server_store::{
  ArtifactContentDigest, ArtifactMediaType, ArtifactRecordStore, ArtifactReportFormat, ArtifactState, ArtifactType,
  ArtifactUploadRecord, ArtifactVerificationResult, BeginArtifactUpload, IdempotencyKey, ListPublishedArtifacts,
  StoreError, VerifyArtifactUpload,
};
use serde::Serialize;
use thiserror::Error;

use crate::{
  AgentOperation, AgentRegistrationError, AgentRegistrationUseCases, ApplicationError, Query, QueryHandler, agent_lease,
};

/// Complete transport-independent input for one output-upload reservation.
#[derive(Clone, Debug)]
pub struct BeginAgentArtifactUploadInput {
  /// Lease identity selected by the route.
  pub route_lease_id: String,
  /// Shared coordinator protocol request.
  pub request: BeginOutputUploadRequest,
  /// Current registration bearer.
  pub credential: AgentCredentialToken,
  /// Server-observed request arrival time.
  pub observed_at_unix_ms: i64,
}

/// Complete transport-independent input for one output-upload completion.
#[derive(Clone, Debug)]
pub struct CompleteAgentArtifactUploadInput {
  /// Lease identity selected by the route.
  pub route_lease_id: String,
  /// Shared coordinator protocol request.
  pub request: CompleteOutputUploadRequest,
  /// Current registration bearer.
  pub credential: AgentCredentialToken,
  /// Server-observed request arrival time.
  pub observed_at_unix_ms: i64,
}

/// Application boundary used by fenced Agent output routes.
#[async_trait]
pub trait AgentArtifactTransferUseCases: Send + Sync {
  /// Reserves logical metadata before issuing a short-lived upload capability.
  async fn begin_upload(
    &self,
    input: BeginAgentArtifactUploadInput,
  ) -> Result<BeginOutputUploadResponse, AgentArtifactError>;

  /// Independently verifies immutable bytes before publishing logical metadata.
  async fn complete_upload(
    &self,
    input: CompleteAgentArtifactUploadInput,
  ) -> Result<CompleteOutputUploadResponse, AgentArtifactError>;
}

/// Logical output projection safe for management clients.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArtifactProjection {
  /// Stable logical Artifact identity.
  pub id: ArtifactId,
  /// Build that owns the output.
  pub build_id: BuildId,
  /// Attempt that produced the output.
  pub attempt_id: octacity_server_domain::AttemptId,
  /// Job that produced the output.
  pub job_id: octacity_server_domain::JobId,
  /// User-visible logical name.
  pub name: String,
  /// Stable logical kind.
  pub kind: ArtifactProjectionKind,
  /// Logical media type preserved from the output declaration.
  pub media_type: String,
  /// Exact byte length.
  pub size_bytes: u64,
  /// Lowercase SHA-256 identity.
  pub sha256: String,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Logical output kind without storage-provider semantics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArtifactProjectionKind {
  /// User-visible file or archive.
  Artifact,
  /// Machine-readable report with an open format identifier.
  Report {
    /// Plugin-owned open format identifier.
    format: String,
  },
}

/// Short-lived download authority paired with safe logical metadata.
#[derive(Clone, Eq, PartialEq)]
pub struct ArtifactDownloadProjection {
  /// Logical output metadata.
  pub artifact: ArtifactProjection,
  /// Opaque short-lived GET capability.
  pub url: String,
  /// Unix millisecond at which the capability expires.
  pub expires_at_unix_ms: i64,
}

impl std::fmt::Debug for ArtifactDownloadProjection {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("ArtifactDownloadProjection")
      .field("artifact", &self.artifact)
      .field("url", &"<redacted>")
      .field("expires_at_unix_ms", &self.expires_at_unix_ms)
      .finish()
  }
}

/// Typed query for one published logical output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetArtifactQuery {
  /// Logical Artifact identity.
  pub artifact_id: ArtifactId,
}

impl Query for GetArtifactQuery {
  type Outcome = ArtifactProjection;
}

/// Typed bounded query for published outputs of one Build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListBuildArtifactsQuery {
  /// Build whose outputs are requested.
  pub build_id: BuildId,
  /// Positive result ceiling.
  pub limit: u16,
}

impl Query for ListBuildArtifactsQuery {
  type Outcome = Vec<ArtifactProjection>;
}

/// Typed query that mints a short-lived download capability for published bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorizeArtifactDownloadQuery {
  /// Logical Artifact identity.
  pub artifact_id: ArtifactId,
  /// Server-observed query time.
  pub observed_at_unix_ms: i64,
}

impl Query for AuthorizeArtifactDownloadQuery {
  type Outcome = ArtifactDownloadProjection;
}

/// Store-backed logical Artifact service coordinated with a replaceable byte store.
pub struct ArtifactHandlers<S, B> {
  registrations: Arc<dyn AgentRegistrationUseCases>,
  store: Arc<S>,
  bytes: Arc<B>,
  upload_capability_lifetime: Duration,
  download_capability_lifetime: Duration,
}

impl<S, B> ArtifactHandlers<S, B> {
  /// Creates the service with explicit bounded capability lifetimes.
  pub fn new(
    registrations: Arc<dyn AgentRegistrationUseCases>,
    store: Arc<S>,
    bytes: Arc<B>,
    upload_capability_lifetime: Duration,
    download_capability_lifetime: Duration,
  ) -> Result<Self, AgentArtifactError> {
    validate_lifetime(upload_capability_lifetime)?;
    validate_lifetime(download_capability_lifetime)?;
    Ok(Self {
      registrations,
      store,
      bytes,
      upload_capability_lifetime,
      download_capability_lifetime,
    })
  }
}

#[async_trait]
impl<S, B> AgentArtifactTransferUseCases for ArtifactHandlers<S, B>
where
  S: ArtifactRecordStore + 'static,
  B: ArtifactStore + 'static,
{
  async fn begin_upload(
    &self,
    input: BeginAgentArtifactUploadInput,
  ) -> Result<BeginOutputUploadResponse, AgentArtifactError> {
    observe_artifact(Operation::Upload, async {
      input
        .request
        .validate()
        .map_err(|_| AgentArtifactError::InvalidRequest)?;
      let context = agent_lease::authorize_operation(
        self.registrations.as_ref(),
        AgentOperation::Upload,
        &input.route_lease_id,
        &input.request.lease,
        &input.request.registration_id,
        input.credential,
        input.observed_at_unix_ms,
      )
      .await
      .map_err(AgentArtifactError::from)?;
      let observed_at = context.observed_at;
      let lease = context.lease;
      let output = &input.request.output;
      let artifact_type = match output.kind {
        OutputKind::Artifact => ArtifactType::Artifact,
        OutputKind::Report => ArtifactType::Report(
          ArtifactReportFormat::new(output.report_format.clone().ok_or(AgentArtifactError::InvalidRequest)?)
            .map_err(|_| AgentArtifactError::InvalidRequest)?,
        ),
      };
      let capability_expires_at = deadline(observed_at, self.upload_capability_lifetime)?;
      let outcome = self
        .store
        .begin_artifact_upload(BeginArtifactUpload {
          artifact_id: ArtifactId::generate(),
          upload_id: ArtifactUploadId::generate(),
          idempotency_key: IdempotencyKey::new(input.request.upload_key.clone())
            .map_err(|_| AgentArtifactError::InvalidRequest)?,
          lease: lease.access,
          job_id: lease.job_id,
          attempt: lease.attempt,
          logical_name: ArtifactName::new(output.name.clone()).map_err(|_| AgentArtifactError::InvalidRequest)?,
          producer_run_id: output.run_id,
          producer_task_id: output.task_id,
          artifact_type,
          media_type: ArtifactMediaType::new(
            output
              .content_type
              .clone()
              .unwrap_or_else(|| output.transport_content_type.clone()),
          )
          .map_err(|_| AgentArtifactError::InvalidRequest)?,
          transport_media_type: ArtifactMediaType::new(output.transport_content_type.clone())
            .map_err(|_| AgentArtifactError::InvalidRequest)?,
          size_bytes: output.size_bytes,
          digest: ArtifactContentDigest::from_lower_hex(&output.sha256)
            .map_err(|_| AgentArtifactError::InvalidRequest)?,
          reserved_at: observed_at,
          capability_expires_at,
        })
        .await?;
      let object = artifact_object(&outcome.upload)?;
      let authorization = self
        .bytes
        .authorize_upload(&object, self.upload_capability_lifetime)
        .await?;
      let expires_at = deadline(observed_at, authorization.expires_in)?;
      Ok(BeginOutputUploadResponse {
        protocol_version: COORDINATOR_PROTOCOL_VERSION,
        request_id: input.request.request_id,
        upload_id: outcome.upload.upload_id.to_string(),
        put_url: authorization.url,
        required_headers: authorization.required_headers,
        expires_at: u64::try_from(expires_at.unix_millis().div_euclid(1_000))
          .map_err(|_| AgentArtifactError::InvalidRequest)?,
      })
    })
    .await
  }

  async fn complete_upload(
    &self,
    input: CompleteAgentArtifactUploadInput,
  ) -> Result<CompleteOutputUploadResponse, AgentArtifactError> {
    observe_artifact(Operation::Verify, async {
      input
        .request
        .validate()
        .map_err(|_| AgentArtifactError::InvalidRequest)?;
      let context = agent_lease::authorize_operation(
        self.registrations.as_ref(),
        AgentOperation::Upload,
        &input.route_lease_id,
        &input.request.lease,
        &input.request.registration_id,
        input.credential,
        input.observed_at_unix_ms,
      )
      .await
      .map_err(AgentArtifactError::from)?;
      let observed_at = context.observed_at;
      let lease = context.lease;
      let upload_id = input
        .request
        .upload_id
        .parse()
        .map_err(|_| AgentArtifactError::InvalidRequest)?;
      let verification = VerifyArtifactUpload {
        upload_id,
        lease: lease.access,
        job_id: lease.job_id,
        attempt: lease.attempt,
        observed_at,
      };
      let upload = self.store.begin_artifact_verification(verification).await?;
      if upload.artifact.state() != ArtifactState::Published {
        let object = artifact_object(&upload)?;
        if let Err(error) = self.bytes.complete_upload(&object).await {
          if matches!(
            error,
            ArtifactStoreError::Integrity { .. } | ArtifactStoreError::NotFound
          ) {
            self
              .store
              .finish_artifact_verification(verification, ArtifactVerificationResult::Rejected)
              .await?;
          }
          return Err(error.into());
        }
        self
          .store
          .finish_artifact_verification(verification, ArtifactVerificationResult::Verified)
          .await?;
      }
      Ok(CompleteOutputUploadResponse {
        protocol_version: COORDINATOR_PROTOCOL_VERSION,
        request_id: input.request.request_id,
        upload_id: input.request.upload_id,
      })
    })
    .await
  }
}

async fn observe_artifact<T>(
  operation: Operation,
  future: impl Future<Output = Result<T, AgentArtifactError>>,
) -> Result<T, AgentArtifactError> {
  crate::telemetry::observe(
    ServerOperationMetric::Artifact,
    operation,
    future,
    |error| match error {
      AgentArtifactError::InvalidRequest | AgentArtifactError::CredentialRejected | AgentArtifactError::NotFound => {
        crate::telemetry::rejected(ErrorClass::Invalid)
      }
      AgentArtifactError::Fenced => crate::telemetry::rejected(ErrorClass::Fenced),
      AgentArtifactError::Expired => crate::telemetry::rejected(ErrorClass::Expired),
      AgentArtifactError::Integrity => crate::telemetry::rejected(ErrorClass::Protocol),
      AgentArtifactError::Conflict => crate::telemetry::rejected(ErrorClass::Conflict),
      AgentArtifactError::Unavailable => crate::telemetry::failed(ErrorClass::Unavailable),
    },
  )
  .await
}

#[async_trait]
impl<S, B> QueryHandler<GetArtifactQuery> for ArtifactHandlers<S, B>
where
  S: ArtifactRecordStore + 'static,
  B: ArtifactStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: GetArtifactQuery) -> Result<ArtifactProjection, Self::Error> {
    project(self.store.published_artifact(query.artifact_id).await?)
  }
}

#[async_trait]
impl<S, B> QueryHandler<ListBuildArtifactsQuery> for ArtifactHandlers<S, B>
where
  S: ArtifactRecordStore + 'static,
  B: ArtifactStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: ListBuildArtifactsQuery) -> Result<Vec<ArtifactProjection>, Self::Error> {
    self
      .store
      .list_published_artifacts(ListPublishedArtifacts {
        build_id: query.build_id,
        limit: query.limit,
      })
      .await?
      .into_iter()
      .map(project)
      .collect::<Result<_, _>>()
  }
}

#[async_trait]
impl<S, B> QueryHandler<AuthorizeArtifactDownloadQuery> for ArtifactHandlers<S, B>
where
  S: ArtifactRecordStore + 'static,
  B: ArtifactStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(
    &self,
    query: AuthorizeArtifactDownloadQuery,
  ) -> Result<ArtifactDownloadProjection, Self::Error> {
    let upload = self.store.published_artifact(query.artifact_id).await?;
    let object = artifact_object(&upload).map_err(map_artifact_error)?;
    let authorization = self
      .bytes
      .authorize_download(&object, self.download_capability_lifetime)
      .await
      .map_err(map_storage_error)?;
    let observed_at =
      Timestamp::from_unix_millis(query.observed_at_unix_ms).map_err(|_| ApplicationError::invalid())?;
    let expires_at = deadline(observed_at, authorization.expires_in).map_err(map_artifact_error)?;
    Ok(ArtifactDownloadProjection {
      artifact: project(upload)?,
      url: authorization.url,
      expires_at_unix_ms: expires_at.unix_millis(),
    })
  }
}

fn artifact_object(upload: &ArtifactUploadRecord) -> Result<ArtifactObject, AgentArtifactError> {
  let identity = upload.artifact.identity();
  ArtifactObject::new(
    identity.artifact_id,
    upload.upload_id,
    identity.size_bytes,
    identity.digest.to_string(),
    upload.transport_media_type.as_str(),
  )
  .map_err(|_| AgentArtifactError::InvalidRequest)
}

fn project(upload: ArtifactUploadRecord) -> Result<ArtifactProjection, ApplicationError> {
  let identity = upload.artifact.identity();
  let kind = match &identity.artifact_type {
    ArtifactType::Artifact => ArtifactProjectionKind::Artifact,
    ArtifactType::Report(format) => ArtifactProjectionKind::Report {
      format: format.as_str().to_owned(),
    },
  };
  Ok(ArtifactProjection {
    id: identity.artifact_id,
    build_id: identity.build_id,
    attempt_id: identity.attempt_id,
    job_id: identity.job_id,
    name: identity.logical_name.to_string(),
    kind,
    media_type: identity.media_type.as_str().to_owned(),
    size_bytes: identity.size_bytes,
    sha256: identity.digest.to_string(),
    published_at: upload.artifact.published_at().ok_or(ApplicationError::unavailable())?,
  })
}

fn validate_lifetime(value: Duration) -> Result<(), AgentArtifactError> {
  if value.is_zero() || value.as_millis() > i64::MAX as u128 {
    Err(AgentArtifactError::InvalidRequest)
  } else {
    Ok(())
  }
}

fn deadline(start: Timestamp, lifetime: Duration) -> Result<Timestamp, AgentArtifactError> {
  let milliseconds = i64::try_from(lifetime.as_millis()).map_err(|_| AgentArtifactError::InvalidRequest)?;
  let value = start
    .unix_millis()
    .checked_add(milliseconds)
    .ok_or(AgentArtifactError::InvalidRequest)?;
  Timestamp::from_unix_millis(value).map_err(|_| AgentArtifactError::InvalidRequest)
}

impl From<agent_lease::AuthorizationError> for AgentArtifactError {
  fn from(value: agent_lease::AuthorizationError) -> Self {
    match value {
      agent_lease::AuthorizationError::InvalidRequest => Self::InvalidRequest,
      agent_lease::AuthorizationError::Registration(error) => error.into(),
    }
  }
}

fn map_storage_error(error: ArtifactStoreError) -> ApplicationError {
  match error {
    ArtifactStoreError::Invalid { .. } => ApplicationError::invalid(),
    ArtifactStoreError::NotFound => ApplicationError::Store(StoreError::NotFound {
      entity: EntityKind::Artifact,
    }),
    ArtifactStoreError::Integrity { .. } | ArtifactStoreError::TimedOut { .. } | ArtifactStoreError::Backend { .. } => {
      ApplicationError::unavailable()
    }
  }
}

fn map_artifact_error(error: AgentArtifactError) -> ApplicationError {
  match error {
    AgentArtifactError::InvalidRequest => ApplicationError::invalid(),
    AgentArtifactError::NotFound => ApplicationError::Store(StoreError::NotFound {
      entity: EntityKind::Artifact,
    }),
    AgentArtifactError::CredentialRejected
    | AgentArtifactError::Fenced
    | AgentArtifactError::Expired
    | AgentArtifactError::Integrity
    | AgentArtifactError::Conflict
    | AgentArtifactError::Unavailable => ApplicationError::unavailable(),
  }
}

/// Stable failures understood by the Agent Artifact HTTP adapter.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AgentArtifactError {
  /// The route, shared DTO, logical metadata, or timestamp is invalid.
  #[error("invalid Agent Artifact request")]
  InvalidRequest,
  /// The registration bearer was rejected.
  #[error("agent credential rejected")]
  CredentialRejected,
  /// The registration or Lease no longer owns the Job.
  #[error("lease is fenced")]
  Fenced,
  /// The current Lease reached its authoritative deadline.
  #[error("lease is expired")]
  Expired,
  /// Uploaded bytes were absent or differed from their declared immutable identity.
  #[error("uploaded Artifact failed integrity verification")]
  Integrity,
  /// An idempotency key or logical output name conflicts with prior intent.
  #[error("Artifact request conflicts with durable state")]
  Conflict,
  /// The logical Artifact or pending upload does not exist.
  #[error("Artifact was not found")]
  NotFound,
  /// The authoritative operation could not safely complete.
  #[error("Artifact service unavailable")]
  Unavailable,
}

impl From<AgentRegistrationError> for AgentArtifactError {
  fn from(value: AgentRegistrationError) -> Self {
    match value {
      AgentRegistrationError::InvalidRequest => Self::InvalidRequest,
      AgentRegistrationError::CredentialRejected => Self::CredentialRejected,
      AgentRegistrationError::Fenced => Self::Fenced,
      AgentRegistrationError::Unavailable => Self::Unavailable,
    }
  }
}

impl From<StoreError> for AgentArtifactError {
  fn from(value: StoreError) -> Self {
    match value {
      StoreError::InvalidInput { .. } => Self::InvalidRequest,
      StoreError::CredentialRejected => Self::CredentialRejected,
      StoreError::Fenced { .. } => Self::Fenced,
      StoreError::Expired { .. } => Self::Expired,
      StoreError::Conflict {
        entity: EntityKind::Artifact | EntityKind::ArtifactUpload,
      }
      | StoreError::Duplicate {
        entity: EntityKind::Artifact | EntityKind::ArtifactUpload,
      } => Self::Conflict,
      StoreError::NotFound { .. } => Self::NotFound,
      StoreError::Conflict { .. }
      | StoreError::Duplicate { .. }
      | StoreError::EventGap { .. }
      | StoreError::EventsMissing { .. }
      | StoreError::Unavailable => Self::Unavailable,
    }
  }
}

impl From<ArtifactStoreError> for AgentArtifactError {
  fn from(value: ArtifactStoreError) -> Self {
    match value {
      ArtifactStoreError::Invalid { .. } => Self::InvalidRequest,
      ArtifactStoreError::Integrity { .. } | ArtifactStoreError::NotFound => Self::Integrity,
      ArtifactStoreError::TimedOut { .. } | ArtifactStoreError::Backend { .. } => Self::Unavailable,
    }
  }
}
