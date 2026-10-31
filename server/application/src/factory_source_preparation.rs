//! Trusted restart-safe mapping, immutable publication and ordinary admission intent.
use crate::{ApplicationError, FactoryExecutionClock, WorkSourceMapping};
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactObject};
use octacity_server_domain::{ArtifactId, ArtifactUploadId};
use octacity_server_factory::{FactoryArtifactReference, FactoryDigest, WorkClassification};
use octacity_server_store::{
  FactoryWorkAdmissionIntent, IdempotencyKey, PublishSourceArtifacts, PublishSourcePreparationIntent,
  RepositorySourceSelection, ReserveSourceArtifacts, SourceArtifactObject, SourceArtifactRole, WorkSourceArtifactStore,
  WorkSourceScanClaim,
};
use std::sync::Arc;
use uuid::Uuid;

/// Exact installed preparation choices; ticket data cannot deserialize these as authority.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePreparationSettings {
  /// Immutable installed mapping and private limits.
  pub mapping: WorkSourceMapping,
  /// Operator-selected Repository source policy.
  pub source: RepositorySourceSelection,
  /// Operator-selected priority, risk and bounded safe metadata.
  pub classification: WorkClassification,
}
impl SourcePreparationSettings {
  pub(crate) async fn authorize(
    &self,
    policy: &dyn crate::ManagementAuthorizationPolicy,
    context: &crate::ManagementRequestContext,
    preparation: &octacity_server_store::WorkSourcePreparation,
  ) -> Result<(), ApplicationError> {
    use crate::{ManagementResourceKind, factory_configuration_choices::require_visible_reference};
    crate::factory_admission::authorize_project_admission(policy, context, preparation.input.project_id()).await?;
    require_visible_reference(
      policy,
      context,
      ManagementResourceKind::FactoryConfiguration,
      preparation.input.definition().target.id(),
    )
    .await?;
    require_visible_reference(
      policy,
      context,
      ManagementResourceKind::Repository,
      preparation.input.definition().repository.id,
    )
    .await?;
    if let crate::SourceAcceptanceMapping::ProcessCompletion(crate::SourceCompletionDocument::ProjectInput {
      artifact,
      ..
    }) = &self.mapping.acceptance
    {
      require_visible_reference(
        policy,
        context,
        ManagementResourceKind::Artifact,
        artifact.artifact_id(),
      )
      .await?;
    }
    Ok(())
  }
}
/// Safe preparation result; no raw title/body reaches diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourcePreparationOutcome {
  /// Exact normalized original command; VCS is resolved only by ordinary admission.
  Prepared(Box<FactoryWorkAdmissionIntent>),
  /// Retained safe candidate failure, with no fabricated artifacts or Run.
  Rejected(crate::SourceMappingFailure),
}
/// No provider client is installed here; only bounded retained data and verified private bytes.
pub struct FactoryWorkSourcePreparer<S, B> {
  store: Arc<S>,
  bytes: Arc<B>,
  clock: Arc<dyn FactoryExecutionClock>,
  project_inputs: Option<Arc<crate::FactoryProjectInputReader>>,
  authorization: Option<(
    Arc<dyn crate::ManagementAuthorizationPolicy>,
    crate::ManagementRequestContext,
  )>,
}
impl<S: WorkSourceArtifactStore, B: ArtifactBytePublisher> FactoryWorkSourcePreparer<S, B> {
  /// Installs authoritative fenced metadata, immutable bytes and trusted time.
  pub fn new(store: Arc<S>, bytes: Arc<B>, clock: Arc<dyn FactoryExecutionClock>) -> Self {
    Self {
      store,
      bytes,
      clock,
      project_inputs: None,
      authorization: None,
    }
  }
  /// Installs the shared reader for exact private Project completion references.
  /// Preset drafts require no pre-existing publication or Build.
  pub fn with_project_inputs(mut self, reader: impl Into<Arc<crate::FactoryProjectInputReader>>) -> Self {
    self.project_inputs = Some(reader.into());
    self
  }
  /// Requires current Project execution and exact input visibility before private I/O.
  /// Trusted library callers may compose authorization at their own entry boundary.
  pub fn with_authorization(
    mut self,
    policy: Arc<dyn crate::ManagementAuthorizationPolicy>,
    context: crate::ManagementRequestContext,
  ) -> Self {
    self.authorization = Some((policy, context));
    self
  }
  /// Retains original snapshot, byte identities, mapping choices and complete intent across retries.
  pub async fn prepare(
    &self,
    claim: WorkSourceScanClaim,
    id: FactoryDigest,
    settings: SourcePreparationSettings,
  ) -> Result<SourcePreparationOutcome, ApplicationError> {
    settings.mapping.validate().map_err(|_| ApplicationError::invalid())?;
    if let Some((policy, context)) = &self.authorization {
      let preparation = self.store.source_preparation(claim.project_id, id).await?;
      settings.authorize(policy.as_ref(), context, &preparation).await?;
    }
    if let crate::SourceAcceptanceMapping::ProcessCompletion(document) = &settings.mapping.acceptance
      && document.project_id() != claim.project_id
    {
      return Err(ApplicationError::invalid());
    }
    let policy = serde_json::to_vec(&settings).map_err(|_| ApplicationError::invalid())?;
    let digest = FactoryDigest::content_sha256(&policy);
    let preparation = self
      .store
      .freeze_source_preparation_policy(octacity_server_store::FreezeSourcePreparationPolicy {
        claim: claim.clone(),
        preparation_id: id,
        policy,
        now: self.clock.now()?,
      })
      .await?;
    let resolved = if let crate::SourceAcceptanceMapping::ProcessCompletion(document) = &settings.mapping.acceptance
      && let crate::SourceCompletionDocument::ProjectInput { artifact, .. } = document
    {
      document
        .validate(settings.mapping.max_body_bytes)
        .map_err(|_| ApplicationError::invalid())?;
      if document.project_id() != preparation.input.project_id() {
        return Err(ApplicationError::invalid());
      }
      // Reading uses the current fenced operation window; the actual Work input
      // retention is checked against the authoritative reservation before publication.
      let required_until = claim.until.min(preparation.snapshot_expires_at);
      Some(
        self
          .project_inputs
          .as_ref()
          .ok_or_else(ApplicationError::capability_unavailable)?
          .read(crate::ReadFactoryArtifact {
            project_id: preparation.input.project_id(),
            artifact: artifact.clone(),
            maximum_bytes: u64::from(settings.mapping.max_body_bytes),
            observed_at: self.clock.now()?,
            required_until,
          })
          .await?,
      )
    } else {
      None
    };
    // A successful asynchronous read is data, not a continuing authorization grant.
    if resolved.is_some()
      && let Some((policy, context)) = &self.authorization
    {
      settings.authorize(policy.as_ref(), context, &preparation).await?;
    }
    let mapped = match crate::factory_source_mapping::map_work_source_snapshot_with_completion(
      &preparation,
      &settings.mapping,
      resolved.as_ref(),
    ) {
      Ok(mapped) => mapped,
      Err(crate::SourceMappingFailure::InvalidMapping) => return Err(ApplicationError::invalid()),
      Err(reason) => {
        let name = match reason {
          crate::SourceMappingFailure::MissingAcceptance => "source.missing_acceptance",
          crate::SourceMappingFailure::MissingTitle => "source.missing_title",
          crate::SourceMappingFailure::MissingBody => "source.missing_body",
          crate::SourceMappingFailure::TooLarge => "source.too_large",
          crate::SourceMappingFailure::InvalidSnapshot => "source.invalid_snapshot",
          crate::SourceMappingFailure::InvalidMapping => unreachable!(),
        };
        self
          .store
          .reject_source_preparation(octacity_server_store::RejectSourcePreparation {
            claim,
            preparation_id: id,
            mapping_digest: digest,
            reason: octacity_server_factory::FactoryKey::new(name).map_err(|_| ApplicationError::invalid())?,
            retention_millis: settings.mapping.retention_millis,
            now: self.clock.now()?,
          })
          .await?;
        return Ok(SourcePreparationOutcome::Rejected(reason));
      }
    };
    let namespace = Uuid::new_v5(&Uuid::NAMESPACE_OID, &id.as_bytes());
    let mut documents = Vec::new();
    for (role, name, bytes) in [
      (SourceArtifactRole::Task, "task", mapped.task),
      (SourceArtifactRole::Acceptance, "acceptance", mapped.acceptance),
    ] {
      let content = FactoryDigest::content_sha256(&bytes);
      let identity = FactoryDigest::sha256(
        "octacity.factory.source-document.v1",
        &[name.as_bytes(), &content.as_bytes()],
      );
      let artifact = ArtifactId::from_uuid(Uuid::new_v5(&namespace, &identity.as_bytes()))
        .map_err(|_| ApplicationError::invalid())?;
      documents.push((
        SourceArtifactObject {
          role,
          artifact: FactoryArtifactReference::new(artifact, content, bytes.len() as u64)
            .map_err(|_| ApplicationError::invalid())?,
          upload_id: ArtifactUploadId::from_uuid(Uuid::new_v5(&artifact.as_uuid(), b"source-input:v1"))
            .map_err(|_| ApplicationError::invalid())?,
          media_type: octacity_server_store::ArtifactMediaType::new("application/json")
            .map_err(|_| ApplicationError::invalid())?,
        },
        bytes,
      ));
    }
    let mut request = ReserveSourceArtifacts {
      claim: claim.clone(),
      preparation_id: id,
      mapping_digest: digest,
      objects: documents.iter().map(|(object, _)| object.clone()).collect(),
      retention_millis: settings.mapping.retention_millis,
      now: self.clock.now()?,
    };
    if let Some(document) = &resolved {
      let current = self.store.source_materialization(claim.project_id, id).await?;
      let proposed = request.retain(&preparation, current.as_ref())?;
      if proposed.expires_at > document.retained_until() {
        return Err(ApplicationError::invalid());
      }
    }
    let reserved = self.store.reserve_source_artifacts(request.clone()).await?;
    // Recheck the authoritative reservation if a concurrent identical owner won first.
    if resolved
      .as_ref()
      .is_some_and(|document| reserved.expires_at > document.retained_until())
    {
      return Err(ApplicationError::invalid());
    }
    if reserved.published_at.is_none() {
      for (object, bytes) in documents {
        request.now = self.clock.now()?;
        self.store.reserve_source_artifacts(request.clone()).await?;
        let description = ArtifactObject::new(
          object.artifact.artifact_id(),
          object.upload_id,
          object.artifact.encoded_size(),
          object.artifact.content_digest().to_string(),
          object.media_type.as_str(),
        )
        .map_err(|_| ApplicationError::invalid())?;
        self
          .bytes
          .publish_verified(&description, bytes)
          .await
          .map_err(|_| ApplicationError::unavailable())?;
      }
      self
        .store
        .publish_source_artifacts(PublishSourceArtifacts {
          claim: claim.clone(),
          preparation_id: id,
          mapping_digest: digest,
          now: self.clock.now()?,
        })
        .await?;
    }
    let intent = FactoryWorkAdmissionIntent {
      profile_overrides: Default::default(),
      project_id: preparation.input.project_id(),
      configuration_id: preparation.input.definition().target.id(),
      configuration_version: preparation.input.definition().target.version(),
      repository_id: Some(preparation.input.definition().repository.id),

      repository_version: Some(preparation.input.definition().repository.version),

      external_identity: preparation
        .admission_identity(octacity_server_store::ManagementSecurityScope::trusted_network())
        .external_identity,
      source: settings.source.into(),
      artifacts: reserved.work_artifacts()?,
      classification: settings.classification,
      idempotency_key: IdempotencyKey::new(format!("source-preparation.{id}"))
        .map_err(|_| ApplicationError::invalid())?,
      admitted_at: reserved.reserved_at,
    };
    let retained = self
      .store
      .publish_source_preparation_intent(PublishSourcePreparationIntent {
        claim,
        preparation_id: id,
        intent,
        now: self.clock.now()?,
      })
      .await?;
    Ok(SourcePreparationOutcome::Prepared(Box::new(
      retained.intent.ok_or_else(ApplicationError::unavailable)?,
    )))
  }
}
