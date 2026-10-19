//! Provider-independent Project text preparation and immutable input ownership.
use crate::*;
use octacity_server_domain::{ArtifactId, ArtifactUploadId, ImmutableRevision, ProjectId, Timestamp};
use octacity_server_factory::{
  ExternalWorkIdentity, FactoryArtifactReference, FactoryDigest, FactoryKey, WorkArtifacts, WorkClassification,
};
use serde::{Deserialize, Serialize};

const PREPARATION_LEASE_MILLIS: i64 = 30_000;
const PREPARATION_DEADLINE_MILLIS: i64 = 300_000;
const MAX_PREPARATION_ATTEMPTS: u16 = 16;
/// Only explicitly mapped Work inputs are published; no implicit specification fetch.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryInputRole {
  /// Untrusted task document.
  Task,
  /// Explicit required acceptance document.
  Acceptance,
}
/// Immutable private byte identity reserved before storage I/O.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryInputObject {
  /// Declared role, independent of content or labels.
  pub role: FactoryInputRole,
  /// Exact logical identity, SHA-256 and encoded byte length.
  pub artifact: FactoryArtifactReference,
  /// Exact immutable byte publication identity.
  pub upload_id: ArtifactUploadId,
  /// Logical input format.
  pub media_type: crate::ArtifactMediaType,
}

/// Private exact UTF-8 pair; never printed in operation diagnostics.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTextDocuments {
  task: String,
  acceptance: String,
}
impl std::fmt::Debug for ProjectTextDocuments {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("ProjectTextDocuments").finish_non_exhaustive()
  }
}
impl ProjectTextDocuments {
  /// Constructs the exact submitted pair without normalization.
  pub fn try_new(task: impl Into<String>, acceptance: impl Into<String>) -> Result<Self, StoreError> {
    let result = Self {
      task: task.into(),
      acceptance: acceptance.into(),
    };
    result.validate()?;
    Ok(result)
  }
  /// Checks common byte/control bounds; ingress-specific schemas may be stricter.
  pub fn validate(&self) -> Result<(), StoreError> {
    if [&self.task, &self.acceptance].iter().any(|text| {
      text.trim().is_empty()
        || text.len() > MAX_SOURCE_INPUT_BYTES as usize
        || text.chars().any(|c| c.is_control() && !matches!(c, '\t' | '\r' | '\n'))
    }) || (self.task.len() as u64 + self.acceptance.len() as u64)
      > octacity_server_factory::MAX_CONTEXT_MANIFEST_BYTES
    {
      return Err(StoreError::invalid(
        StoreOperation::ReserveProjectTextInput,
        StoreInputError::InvalidProjectTextInput,
      ));
    }
    Ok(())
  }
  /// Exact private task bytes.
  pub fn task(&self) -> &[u8] {
    self.task.as_bytes()
  }
  /// Exact private acceptance bytes.
  pub fn acceptance(&self) -> &[u8] {
    self.acceptance.as_bytes()
  }
}
/// Retained reviewed intent; private documents have a separate storage lifetime.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTextIntent {
  /// Exact owning Project and immutable target/repository policy.
  pub target: ReadFactoryAdmissionContext,
  /// Stable ordinary manual Work identity supplied by the application ingress.
  pub external_identity: ExternalWorkIdentity,
  /// Reviewed base policy, resolved only once before publication.
  pub source: RepositorySourceSelection,
  /// Explicit reviewed admission classification.
  pub classification: WorkClassification,
  /// Immutable application mapping version, never executable text.
  pub mapping: FactoryKey,
  /// Project/security-scoped caller replay identity.
  pub idempotency_key: IdempotencyKey,
}
/// Shared immutable submission; Issue and quickstart use the same publication contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTextSubmission {
  /// Reviewed metadata retained after private body deletion.
  pub intent: ProjectTextIntent,
  /// Exact private pair, stored separately from preparation metadata.
  pub documents: ProjectTextDocuments,
}
/// Metadata retained before any byte write; no fabricated Build/Job/Agent provenance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectTextPreparation {
  /// Stable technical publication operation.
  pub id: FactoryDigest,
  /// Original admission security partition; never a client-selected proof.
  #[serde(
    serialize_with = "serialize_retained_scope",
    deserialize_with = "deserialize_retained_scope"
  )]
  pub security_scope: ManagementSecurityScope,
  /// Monotonic preparation ownership fence.
  pub generation: u64,
  /// Current owner's start time.
  pub claimed_at: Timestamp,
  /// Current bounded owner lease.
  pub lease_until: Timestamp,
  /// Original hard preparation deadline, never renewed.
  pub preparation_deadline: Timestamp,
  /// Consumed ownership attempts, never reset.
  pub attempts: u16,
  /// Original immutable intent, without private bodies.
  pub intent: ProjectTextIntent,
  /// Complete reserved pair, including unpublished cleanup identities.
  pub objects: Vec<FactoryInputObject>,
  /// Trusted original reservation time.
  pub reserved_at: Timestamp,
  /// Exclusive private input lifetime.
  pub expires_at: Timestamp,
  /// Trusted resolved base, retained before bytes are published.
  pub frozen_revision: Option<ImmutableRevision>,
  /// Visibility point for the complete verified pair.
  pub published_at: Option<Timestamp>,
  /// Original ordinary admission; absent until admission commits.
  pub admission: Option<PublishedFactoryAdmission>,
}
impl ProjectTextPreparation {
  /// Compares exact intent using retained byte identities even after private body retirement.
  pub fn matches_submission(&self, submitted: &ProjectTextSubmission) -> bool {
    self.intent == submitted.intent
      && [
        (FactoryInputRole::Task, submitted.documents.task()),
        (FactoryInputRole::Acceptance, submitted.documents.acceptance()),
      ]
      .into_iter()
      .all(|(role, bytes)| {
        self.objects.iter().any(|object| {
          object.role == role
            && object.artifact.encoded_size() == bytes.len() as u64
            && object.artifact.content_digest() == FactoryDigest::content_sha256(bytes)
        })
      })
  }
  /// Binds a fresh observation to this exact server-retained owner.
  pub fn guard(&self, now: Timestamp) -> ProjectTextGuard {
    ProjectTextGuard {
      id: self.id,
      project_id: self.intent.target.project_id,
      generation: self.generation,
      now,
    }
  }
  /// Rejects stale owners and elapsed preparation/lease windows before effects.
  pub fn require_owner(&self, guard: &ProjectTextGuard) -> Result<(), StoreError> {
    if guard.id != self.id
      || guard.project_id != self.intent.target.project_id
      || guard.generation != self.generation
      || guard.now < self.claimed_at
      || guard.now >= self.lease_until
      || guard.now >= self.preparation_deadline
      || guard.now >= self.expires_at
    {
      return Err(project_text_conflict());
    }
    Ok(())
  }
  /// Validates the retained ordinary admission rather than accepting a caller's resolved proof.
  pub fn validate_admission(
    &self,
    guard: &ProjectTextGuard,
    request: &ManagementMutation<AdmitFactoryWork>,
  ) -> Result<(), StoreError> {
    self.require_owner(guard)?;
    let intent = self.admission_intent()?;
    let r = request.mutation();
    let work = &r.work;
    if self.published_at.is_none()
      || self.frozen_revision.as_ref() != Some(work.subject().base_revision())
      || request.audit().security_scope() != &self.security_scope
      || r.probe.source_scope.security_scope != self.security_scope
      || r.probe.source_scope.source.as_str() != "manual"
      || r.probe.external_identity != intent.external_identity
      || r.probe.idempotency_key != intent.idempotency_key
      || r.probe.intent_digest != factory_admission_intent_digest(&intent, "octacity.factory.manual-admission.v1")
      || work.external_identity() != &intent.external_identity
      || work.artifacts() != &intent.artifacts
      || work.subject().project_id() != intent.project_id
      || work.subject().repository_id() != intent.repository_id
      || work.configuration().id() != intent.configuration_id
      || work.configuration().version() != intent.configuration_version
      || r.repository_version != intent.repository_version
      || work.priority() != intent.classification.priority()
      || work.risk() != intent.classification.risk()
      || work.metadata() != intent.classification.metadata()
      || r.admitted_at != self.reserved_at
    {
      return Err(project_text_conflict());
    }
    Ok(())
  }
  /// Ordinary input references, independent of an Issue or a provider.
  pub fn work_artifacts(&self) -> Result<WorkArtifacts, StoreError> {
    let role = |role| {
      self
        .objects
        .iter()
        .find(|o| o.role == role)
        .map(|o| o.artifact.clone())
        .ok_or_else(project_text_conflict)
    };
    WorkArtifacts::new(
      role(FactoryInputRole::Task)?,
      role(FactoryInputRole::Acceptance)?,
      vec![],
    )
    .map_err(|_| project_text_conflict())
  }
  /// Original ordinary command; digest/replay retain the reviewed source expression.
  pub fn admission_intent(&self) -> Result<FactoryWorkAdmissionIntent, StoreError> {
    let s = &self.intent;
    let t = s.target;
    Ok(FactoryWorkAdmissionIntent {
      project_id: t.project_id,
      configuration_id: t.configuration_id,
      configuration_version: t.configuration_version,
      repository_id: t.repository_id,
      repository_version: t.repository_version,
      external_identity: s.external_identity.clone(),
      source: s.source.clone(),
      artifacts: self.work_artifacts()?,
      classification: s.classification.clone(),
      idempotency_key: s.idempotency_key.clone(),
      admitted_at: self.reserved_at,
    })
  }
}
/// Trusted preparation ownership; clients cannot submit source-resolution proofs.
#[derive(Clone, Debug)]
pub struct ProjectTextGuard {
  /// Stable original operation.
  pub id: FactoryDigest,
  /// Original owning Project.
  pub project_id: ProjectId,
  /// Current monotonic owner fence.
  pub generation: u64,
  /// Trusted observation time.
  pub now: Timestamp,
}
/// Original private intent and trusted reservation time.
#[derive(Clone, Debug)]
pub struct ReserveProjectText {
  /// Frozen caller review.
  pub submission: ProjectTextSubmission,
  /// Trusted server time.
  pub now: Timestamp,
}
/// Published Project input metadata, distinct from Build output and Source observations.
#[derive(Clone, Debug, Serialize)]
pub struct PublishedProjectInput {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Original technical publication identity.
  pub preparation_id: FactoryDigest,
  /// Original immutable application mapping.
  pub mapping: FactoryKey,
  /// Exact private object.
  pub object: FactoryInputObject,
  /// Original publication time.
  pub published_at: Timestamp,
  /// Exclusive private read deadline.
  pub expires_at: Timestamp,
}
/// Query-only metadata; reads never reserve or publish an input.
#[async_trait::async_trait]
pub trait ProjectInputQueryStore: Send + Sync {
  /// Resolves only complete Project-owned publication in the selected Project.
  async fn project_input_artifact(
    &self,
    project: ProjectId,
    artifact: ArtifactId,
  ) -> Result<Option<PublishedProjectInput>, StoreError>;
}
/// One recoverable publication operation above the existing ordinary admission owner.
#[async_trait::async_trait]
pub trait ProjectTextInputStore: FactoryAdmissionStore + ProjectInputQueryStore {
  /// Original scoped intent/receipt; checked before source resolution.
  async fn project_text_preparation(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<ProjectTextPreparation>, StoreError>;
  /// Reads the original private pair only for scoped recovery; deletion returns None.
  async fn project_text_documents(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<ProjectTextDocuments>, StoreError>;
  /// Reserves stable object identities before I/O.
  async fn reserve_project_text(
    &self,
    request: ManagementMutation<ReserveProjectText>,
  ) -> Result<ProjectTextPreparation, StoreError>;
  /// Revalidates current ownership immediately before external byte effects.
  async fn require_project_text_owner(&self, guard: ProjectTextGuard) -> Result<(), StoreError>;
  /// Releases only the matching owner's lease after a recoverable failure.
  async fn release_project_text(&self, guard: ProjectTextGuard) -> Result<(), StoreError>;
  /// Freezes one trusted base before any object write.
  async fn freeze_project_text(
    &self,
    guard: ProjectTextGuard,
    revision: ImmutableRevision,
  ) -> Result<ProjectTextPreparation, StoreError>;
  /// Makes the complete verified pair visible atomically.
  async fn publish_project_text(&self, guard: ProjectTextGuard) -> Result<ProjectTextPreparation, StoreError>;
  /// Reuses ordinary admission atomically with its Project input receipt.
  async fn admit_project_text(
    &self,
    guard: ProjectTextGuard,
    request: ManagementMutation<AdmitFactoryWork>,
  ) -> Result<FactoryAdmissionMutationOutcome, StoreError>;
}
pub(crate) fn project_text_conflict() -> StoreError {
  StoreError::Conflict {
    entity: octacity_server_domain::EntityKind::FactoryRun,
  }
}

fn deserialize_retained_scope<'de, D: serde::Deserializer<'de>>(
  deserializer: D,
) -> Result<ManagementSecurityScope, D::Error> {
  let value = String::deserialize(deserializer)?;
  ManagementSecurityScope::new(value).map_err(serde::de::Error::custom)
}
fn serialize_retained_scope<S: serde::Serializer>(
  scope: &ManagementSecurityScope,
  serializer: S,
) -> Result<S::Ok, S::Error> {
  serializer.serialize_str(scope.as_str())
}

impl ProjectTextPreparation {
  /// Reserves deterministic private identities and original preparation bounds before I/O.
  pub fn reserve(s: ProjectTextSubmission, scope: ManagementSecurityScope, now: Timestamp) -> Result<Self, StoreError> {
    s.documents.validate()?;
    let id = FactoryDigest::sha256(
      "octacity.project-text.v1",
      &[
        s.intent.target.project_id.to_string().as_bytes(),
        scope.as_str().as_bytes(),
        s.intent.idempotency_key.as_str().as_bytes(),
      ],
    );
    let mut objects = Vec::new();
    for (role, name, bytes) in [
      (FactoryInputRole::Task, "task", s.documents.task()),
      (FactoryInputRole::Acceptance, "acceptance", s.documents.acceptance()),
    ] {
      let namespace = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, &id.as_bytes());
      let artifact =
        ArtifactId::from_uuid(uuid::Uuid::new_v5(&namespace, name.as_bytes())).map_err(|_| project_text_conflict())?;
      objects.push(FactoryInputObject {
        role,
        artifact: FactoryArtifactReference::new(artifact, FactoryDigest::content_sha256(bytes), bytes.len() as u64)
          .map_err(|_| project_text_conflict())?,
        upload_id: ArtifactUploadId::from_uuid(uuid::Uuid::new_v5(&artifact.as_uuid(), b"project-input.v1"))
          .map_err(|_| project_text_conflict())?,
        media_type: ArtifactMediaType::new("text/plain").map_err(|_| project_text_conflict())?,
      });
    }
    let result = Self {
      id,
      security_scope: scope,
      generation: 1,
      attempts: 1,
      claimed_at: now,
      lease_until: advance(now, PREPARATION_LEASE_MILLIS)?,
      preparation_deadline: advance(now, PREPARATION_DEADLINE_MILLIS)?,
      intent: s.intent,
      objects,
      reserved_at: now,
      expires_at: advance(now, MAX_SOURCE_RETENTION_MILLIS as i64)?,
      frozen_revision: None,
      published_at: None,
      admission: None,
    };
    Ok(result)
  }
  /// Claims an elapsed/released lease without renewing original deadlines or identities.
  pub fn reclaim(&self, now: Timestamp) -> Result<Self, StoreError> {
    if now < self.claimed_at
      || now < self.lease_until
      || now >= self.preparation_deadline
      || self.attempts >= MAX_PREPARATION_ATTEMPTS
    {
      return Err(project_text_conflict());
    }
    let mut next = self.clone();
    next.generation = next.generation.checked_add(1).ok_or_else(project_text_conflict)?;
    next.attempts += 1;
    next.claimed_at = now;
    next.lease_until = advance(now, PREPARATION_LEASE_MILLIS)?.min(next.preparation_deadline);
    Ok(next)
  }
}

fn advance(now: Timestamp, millis: i64) -> Result<Timestamp, StoreError> {
  Timestamp::from_unix_millis(
    now
      .unix_millis()
      .checked_add(millis)
      .ok_or_else(project_text_conflict)?,
  )
  .map_err(|_| project_text_conflict())
}
