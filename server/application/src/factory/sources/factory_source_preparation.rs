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

#[derive(Clone, Copy)]
enum OriginalSourceInputs<'a> {
  Artifacts(&'a octacity_server_factory::WorkArtifacts),
  IssueText(&'a octacity_server_store::ProjectTextDocuments),
}
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
  /// Digest of the accepted predecessor frozen by the Source worker.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub predecessor_digest: Option<FactoryDigest>,
}
impl SourcePreparationSettings {
  pub(crate) async fn authorize(
    &self,
    policy: &dyn crate::ManagementAuthorizationPolicy,
    context: &crate::ManagementRequestContext,
    preparation: &octacity_server_store::WorkSourcePreparation,
  ) -> Result<(), ApplicationError> {
    use crate::{ManagementResourceKind, factory::authoring::factory_configuration_choices::require_visible_reference};
    crate::factory::admission::factory_admission::authorize_project_admission(
      policy,
      context,
      preparation.input.project_id(),
    )
    .await?;
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
    self.prepare_inputs(claim, id, settings, None).await
  }

  /// Materializes only the implementation inputs of the verified handoff.
  pub async fn prepare_handoff(
    &self,
    claim: WorkSourceScanClaim,
    id: FactoryDigest,
    settings: SourcePreparationSettings,
    handoff: &crate::FactorySourceHandoff,
  ) -> Result<SourcePreparationOutcome, ApplicationError> {
    let original = match &handoff.issue_text {
      Some(text) => OriginalSourceInputs::IssueText(text),
      None => OriginalSourceInputs::Artifacts(&handoff.artifacts),
    };
    self
      .prepare_original(claim, id, settings, handoff.record_digest()?, original)
      .await
  }

  async fn prepare_original(
    &self,
    claim: WorkSourceScanClaim,
    id: FactoryDigest,
    mut settings: SourcePreparationSettings,
    predecessor: FactoryDigest,
    inputs: OriginalSourceInputs<'_>,
  ) -> Result<SourcePreparationOutcome, ApplicationError> {
    if settings.predecessor_digest.is_some_and(|digest| digest != predecessor) {
      return Err(crate::factory::sources::factory_source_admission::stale_source_handoff());
    }
    settings.predecessor_digest = Some(predecessor);
    self.prepare_inputs(claim, id, settings, Some(inputs)).await
  }

  async fn prepare_inputs(
    &self,
    claim: WorkSourceScanClaim,
    id: FactoryDigest,
    settings: SourcePreparationSettings,
    original: Option<OriginalSourceInputs<'_>>,
  ) -> Result<SourcePreparationOutcome, ApplicationError> {
    if original.is_some() != settings.predecessor_digest.is_some() {
      return Err(ApplicationError::invalid());
    }
    settings.mapping.validate().map_err(|_| ApplicationError::invalid())?;
    let retained = self.store.source_preparation(claim.project_id, id).await?;
    if let Some((policy, context)) = &self.authorization {
      settings.authorize(policy.as_ref(), context, &retained).await?;
    }
    if let crate::SourceAcceptanceMapping::ProcessCompletion(document) = &settings.mapping.acceptance
      && document.project_id() != claim.project_id
    {
      return Err(ApplicationError::invalid());
    }
    let input = retained.input.definition();
    let selected = self
      .store
      .factory_admission_context(octacity_server_store::ReadFactoryAdmissionContext {
        project_id: retained.input.project_id(),
        configuration_id: input.target.id(),
        configuration_version: input.target.version(),
        repository_id: Some(input.repository.id),
        repository_version: Some(input.repository.version),
      })
      .await?;
    crate::factory::admission::factory_admission::validate_admission_project(&selected, retained.input.project_id())?;
    crate::factory::admission::factory_admission::validate_launch_policy(
      &selected,
      octacity_server_factory::FactoryAdmissionOrigin::AutomaticSource,
    )?;
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
    let required_until = claim.until.min(preparation.snapshot_expires_at);
    let mut retained_until = None;
    let (mapped, media_type) = if let Some(OriginalSourceInputs::IssueText(text)) = original {
      let maximum = settings.mapping.max_artifact_bytes as usize;
      let result = if [text.task(), text.acceptance()]
        .into_iter()
        .any(|bytes| bytes.len() > maximum)
      {
        Err(crate::SourceMappingFailure::TooLarge)
      } else {
        Ok(crate::MappedWorkSource {
          task: text.task().to_vec(),
          acceptance: text.acceptance().to_vec(),
        })
      };
      (result, "text/plain")
    } else if let Some(OriginalSourceInputs::Artifacts(inputs)) = original
      && [inputs.task(), inputs.acceptance()]
        .iter()
        .any(|artifact| artifact.encoded_size() > u64::from(settings.mapping.max_artifact_bytes))
    {
      (Err(crate::SourceMappingFailure::TooLarge), "text/plain")
    } else if let Some(OriginalSourceInputs::Artifacts(inputs)) = original {
      let task = self
        .read_original_input(
          claim.project_id,
          inputs.task(),
          settings.mapping.max_artifact_bytes,
          required_until,
        )
        .await?;
      let acceptance = self
        .read_original_input(
          claim.project_id,
          inputs.acceptance(),
          settings.mapping.max_artifact_bytes,
          required_until,
        )
        .await?;
      retained_until = Some(task.retained_until().min(acceptance.retained_until()));
      (
        Ok(crate::MappedWorkSource {
          task: task.bytes().to_vec(),
          acceptance: acceptance.bytes().to_vec(),
        }),
        "text/plain",
      )
    } else {
      let resolved = if let crate::SourceAcceptanceMapping::ProcessCompletion(document) = &settings.mapping.acceptance
        && let crate::SourceCompletionDocument::ProjectInput { artifact, .. } = document
      {
        document
          .validate(settings.mapping.max_body_bytes)
          .map_err(|_| ApplicationError::invalid())?;
        let resolved = self
          .read_input(
            claim.project_id,
            artifact,
            settings.mapping.max_body_bytes,
            required_until,
          )
          .await?;
        retained_until = Some(resolved.retained_until());
        Some(resolved)
      } else {
        None
      };
      (
        crate::factory::sources::factory_source_mapping::map_work_source_snapshot_with_completion(
          &preparation,
          &settings.mapping,
          resolved.as_ref(),
        ),
        "application/json",
      )
    };
    if retained_until.is_some()
      && let Some((policy, context)) = &self.authorization
    {
      settings.authorize(policy.as_ref(), context, &preparation).await?;
    }
    let mapped = match mapped {
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
          media_type: octacity_server_store::ArtifactMediaType::new(media_type)
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
    if let Some(retained_until) = retained_until {
      let current = self.store.source_materialization(claim.project_id, id).await?;
      let proposed = request.retain(&preparation, current.as_ref())?;
      if proposed.expires_at > retained_until {
        return Err(ApplicationError::invalid());
      }
    }
    let reserved = self.store.reserve_source_artifacts(request.clone()).await?;
    // Recheck the authoritative reservation if a concurrent identical owner won first.
    if retained_until.is_some_and(|until| reserved.expires_at > until) {
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
  async fn read_original_input(
    &self,
    project_id: octacity_server_domain::ProjectId,
    artifact: &FactoryArtifactReference,
    maximum_bytes: u32,
    required_until: octacity_server_domain::Timestamp,
  ) -> Result<crate::FactoryRetainedInput, ApplicationError> {
    self.authorize_original_input(artifact).await?;
    let document = self
      .read_input(project_id, artifact, maximum_bytes, required_until)
      .await?;
    self.authorize_original_input(artifact).await?;
    Ok(document)
  }

  async fn authorize_original_input(&self, artifact: &FactoryArtifactReference) -> Result<(), ApplicationError> {
    if let Some((policy, context)) = &self.authorization {
      crate::factory::authoring::factory_configuration_choices::require_visible_reference(
        policy.as_ref(),
        context,
        crate::ManagementResourceKind::Artifact,
        artifact.artifact_id(),
      )
      .await?;
    }
    Ok(())
  }

  async fn read_input(
    &self,
    project_id: octacity_server_domain::ProjectId,
    artifact: &FactoryArtifactReference,
    maximum_bytes: u32,
    required_until: octacity_server_domain::Timestamp,
  ) -> Result<crate::FactoryRetainedInput, ApplicationError> {
    self
      .project_inputs
      .as_ref()
      .ok_or_else(ApplicationError::capability_unavailable)?
      .read(crate::ReadFactoryArtifact {
        project_id,
        artifact: artifact.clone(),
        maximum_bytes: u64::from(maximum_bytes),
        observed_at: self.clock.now()?,
        required_until,
      })
      .await
  }
}
