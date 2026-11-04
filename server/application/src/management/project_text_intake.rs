//! One Project-owned text publication path for Issue execution and quickstart ingress.
use crate::*;
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactByteReader, ArtifactObject};
/// Existing logical Artifact name ceiling; named Project inputs use the same value.
pub use octacity_server_domain::MAX_ARTIFACT_NAME_BYTES as MAX_PROJECT_DOCUMENT_NAME_BYTES;
use octacity_server_factory::{FlowIncomingData, FlowNodeRecord};
/// Existing private input byte ceiling, shared with named document transports.
pub use octacity_server_store::MAX_SOURCE_INPUT_BYTES as MAX_PROJECT_DOCUMENT_BYTES;
use octacity_server_store::{
  FactoryAdmissionMutationOutcome, FactoryInputRole, ProjectTextInputStore, ReserveProjectText,
};
pub use octacity_server_store::{
  ProjectDocumentResource, ProjectDocumentSubmission, ProjectTextDocuments, ProjectTextIntent, ProjectTextSubmission,
};
use std::sync::Arc;
/// Shared recoverable publication; no Issue, installed Source or Build prerequisite.
pub struct ProjectTextIntake<S, B> {
  store: Arc<S>,
  bytes: Arc<B>,
  revisions: Arc<dyn RevisionResolver>,
  repositories: Option<Arc<dyn octacity_server_store::ConfigurationStore>>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S: ProjectTextInputStore + 'static, B: ArtifactBytePublisher + ArtifactByteReader> ProjectTextIntake<S, B> {
  /// Reuses ordinary admission, immutable byte ports and trusted server time.
  pub fn new(
    store: Arc<S>,
    bytes: Arc<B>,
    revisions: Arc<dyn RevisionResolver>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    Self {
      store,
      bytes,
      revisions,
      repositories: None,
      policy,
      clock,
    }
  }
  /// Uses the existing Project Repository catalog for declared step sources.
  pub fn with_repositories(mut self, repositories: Arc<dyn octacity_server_store::ConfigurationStore>) -> Self {
    self.repositories = Some(repositories);
    self
  }
  /// Publishes one exact pair and admits one original ordinary Work/Run.
  pub async fn submit(
    &self,
    context: &ManagementRequestContext,
    submission: ProjectTextSubmission,
  ) -> Result<FactoryAdmissionMutationOutcome, ApplicationError> {
    self.submit_with_predecessor(context, submission, None).await
  }
  pub(crate) async fn submit_with_predecessor(
    &self,
    context: &ManagementRequestContext,
    submission: ProjectTextSubmission,
    predecessor: Option<&FlowNodeRecord>,
  ) -> Result<FactoryAdmissionMutationOutcome, ApplicationError> {
    let digest = predecessor
      .map(FlowNodeRecord::digest)
      .transpose()
      .map_err(|_| ApplicationError::invalid())?;
    if submission.intent.predecessor_digest != digest {
      return Err(ApplicationError::invalid());
    }
    submission.documents.validate()?;
    authorize_project_text(self.policy.as_ref(), context, &submission.intent.target).await?;
    let original = self
      .store
      .project_text_preparation(
        submission.intent.target.project_id,
        &context.security_scope().to_store(),
        &submission.intent.idempotency_key,
      )
      .await?;
    if let Some(original) = &original {
      if !original.matches_submission(&submission) {
        return Err(
          octacity_server_store::StoreError::Conflict {
            entity: octacity_server_domain::EntityKind::FactoryRun,
          }
          .into(),
        );
      }
      if let Some(admission) = &original.admission {
        return Ok(FactoryAdmissionMutationOutcome {
          disposition: octacity_server_store::MutationDisposition::Replayed,
          admission: admission.clone(),
        });
      }
    }
    let selected = self.store.factory_admission_context(submission.intent.target).await?;
    crate::factory::admission::factory_admission::validate_admission_project(
      &selected,
      submission.intent.target.project_id,
    )?;
    crate::factory::admission::factory_admission::validate_launch_policy(
      &selected,
      octacity_server_factory::FactoryAdmissionOrigin::Manual,
    )?;
    if predecessor.is_none() {
      crate::factory::admission::factory_admission::reject_missing_incoming_result(
        selected.configuration.configuration.flow(),
      )?;
    }
    crate::factory::execution::factory_code_sources::validate_sources(
      &selected.configuration.configuration.flow().closure,
      submission.intent.target.project_id,
      self.repositories.as_deref(),
      self.policy.as_ref(),
      context,
    )
    .await?;
    let source = crate::triggers::manual_trigger::select_source(
      &submission.intent.source,
      selected.repository.as_ref().ok_or_else(ApplicationError::invalid)?,
    )
    .map_err(|_| ApplicationError::InvalidInput)?;
    let documents = submission.documents.clone();
    let record = self
      .store
      .reserve_project_text(crate::management_security::audited_mutation(
        context,
        ReserveProjectText {
          submission,
          now: self.clock.now()?,
        },
      )?)
      .await?;
    let guard = record.guard(self.clock.now()?);
    let result = self
      .publish_and_admit(context, record, selected, source, documents, predecessor)
      .await;
    if result.is_err() {
      let _ = self.store.release_project_text(guard).await;
    }
    result
  }
  async fn publish_and_admit(
    &self,
    context: &ManagementRequestContext,
    mut record: octacity_server_store::ProjectTextPreparation,
    selected: octacity_server_store::FactoryAdmissionContext,
    source: octacity_server_store::RepositorySourceSelection,
    documents: ProjectTextDocuments,
    predecessor: Option<&FlowNodeRecord>,
  ) -> Result<FactoryAdmissionMutationOutcome, ApplicationError> {
    if record.frozen_revision.is_none() {
      let revision = self
        .revisions
        .resolve(RevisionResolutionRequest {
          repository: selected.repository.clone().ok_or_else(ApplicationError::invalid)?,
          selection: source,
        })
        .await
        .map_err(crate::factory::admission::factory_admission::map_revision_error)?;
      require_predecessor_revision(predecessor, &revision)?;
      let pins = crate::factory::execution::factory_code_sources::resolve_used_sources(
        selected.configuration.configuration.flow(),
        self.repositories.as_deref(),
        self.revisions.as_ref(),
      )
      .await?;
      record = self
        .store
        .freeze_project_text(record.guard(self.clock.now()?), revision, pins)
        .await?;
    }
    let revision = record.frozen_revision.clone().ok_or_else(ApplicationError::invalid)?;
    require_predecessor_revision(predecessor, &revision)?;
    if record.published_at.is_none() {
      for object in &record.objects {
        let bytes = match object.role {
          FactoryInputRole::Task => documents.task(),
          FactoryInputRole::Acceptance => documents.acceptance(),
          FactoryInputRole::Document => return Err(ApplicationError::invalid()),
        };
        self
          .store
          .require_project_text_owner(record.guard(self.clock.now()?))
          .await?;
        if !publication::is_input_published(self.bytes.as_ref(), object).await? {
          self
            .store
            .require_project_text_owner(record.guard(self.clock.now()?))
            .await?;
          self
            .bytes
            .publish_verified(&publication::input_object(object)?, bytes.to_vec())
            .await
            .map_err(|_| ApplicationError::unavailable())?;
          self
            .store
            .require_project_text_owner(record.guard(self.clock.now()?))
            .await?;
          if !publication::is_input_published(self.bytes.as_ref(), object).await? {
            return Err(ApplicationError::unavailable());
          }
        }
      }
      record = self.store.publish_project_text(record.guard(self.clock.now()?)).await?;
    }
    let intent = record.admission_intent()?;
    let probe = crate::factory::admission::factory_admission::admission_probe(context, &intent)?;
    let mut admission =
      crate::factory::admission::factory_admission::materialize_resolved_admission(intent, probe, selected, revision)?;
    admission.flow = admission
      .flow
      .with_code_pins(record.frozen_code_pins.clone(), record.intent.target.project_id)
      .map_err(|_| ApplicationError::invalid())?;
    admission.incoming = predecessor
      .map(|record| FlowIncomingData::from_predecessor(&admission.work, &admission.flow, record))
      .transpose()
      .map_err(|_| ApplicationError::invalid())?;
    Ok(
      self
        .store
        .admit_project_text(
          record.guard(self.clock.now()?),
          crate::management_security::audited_mutation(context, admission)?,
        )
        .await?,
    )
  }
}

fn require_predecessor_revision(
  predecessor: Option<&FlowNodeRecord>,
  revision: &octacity_server_domain::ImmutableRevision,
) -> Result<(), ApplicationError> {
  if predecessor.is_some_and(|record| {
    record
      .input()
      .subject()
      .repository()
      .map_or(true, |source| source.base_revision() != revision)
  }) {
    return Err(FactoryValidationDiagnostics::unavailable_with_code(
      "/incoming_result",
      "incoming_result_wrong_subject",
    ));
  }
  Ok(())
}

// Shared authorization for both first submission and original scoped recovery.
pub(crate) async fn authorize_project_text(
  policy: &dyn ManagementAuthorizationPolicy,
  context: &ManagementRequestContext,
  target: &octacity_server_store::ReadFactoryAdmissionContext,
) -> Result<(), ApplicationError> {
  crate::factory::admission::factory_admission::authorize_project_admission(policy, context, target.project_id).await?;
  crate::factory::authoring::factory_configuration_choices::require_visible_reference(
    policy,
    context,
    ManagementResourceKind::FactoryConfiguration,
    target.configuration_id,
  )
  .await?;
  crate::factory::authoring::factory_configuration_choices::require_visible_reference(
    policy,
    context,
    ManagementResourceKind::Repository,
    target.repository_id.ok_or_else(ApplicationError::invalid)?,
  )
  .await?;
  Ok(())
}

#[path = "project_input_document.rs"]
mod documents;

pub use documents::{
  ListProjectInputDocumentsQuery, ProjectDocumentContent, ProjectDocumentPage, PublishProjectDocumentCommand,
  ReadProjectInputDocumentQuery,
};

#[path = "project_input_publication.rs"]
mod publication;
