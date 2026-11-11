use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{ProjectId, RepositoryId, Timestamp};
use octacity_server_factory::{
  FactoryConfigurationId, FactoryDigest, FactoryKey, FactoryRun, FactoryRunId, WorkArtifacts, WorkEnvelope,
  WorkEnvelopeId,
};
use octacity_server_store::{
  AdmitFactoryWork, FactoryAdmissionMutationOutcome, FactoryAdmissionProbe, FactoryAdmissionStore,
  FactoryWorkSourceScope, ReadFactoryAdmissionContext,
};

use crate::{
  ApplicationError, Command, ManagementAction, ManagementAuthorizationMapping, ManagementAuthorizationTarget,
  ManagementResourceKind, ManagementResourceResult, MutationDisposition, RevisionResolutionError,
  RevisionResolutionRequest, RevisionResolver,
  management_security::{audited_mutation, owned_collection_resource},
  triggers::manual_trigger::select_source,
};

const MANUAL_WORK_SOURCE: &str = "manual";

pub use octacity_server_store::FactoryWorkAdmissionIntent;

/// Backward-compatible management command for the built-in manual source.
pub type AdmitManualFactoryWorkCommand = FactoryWorkAdmissionIntent;

impl Command for AdmitManualFactoryWorkCommand {
  type Outcome = FactoryAdmissionOutcome;
}

impl ManagementAuthorizationTarget for AdmitManualFactoryWorkCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::Execute,
    ManagementResourceKind::FactoryRun,
    ManagementResourceKind::Project,
  );

  fn management_resource(&self) -> ManagementResourceResult {
    owned_collection_resource(
      ManagementResourceKind::FactoryRun,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}

/// Safe result of applying or replaying one Work admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryAdmissionOutcome {
  /// Whether this invocation admitted Work or observed the original result.
  pub disposition: MutationDisposition,
  /// Immutable Work Envelope identity.
  pub work_envelope_id: WorkEnvelopeId,
  /// Durable Factory Run identity.
  pub factory_run_id: FactoryRunId,
  /// Original resolved revision, absent for repository-free Project/input Work.
  pub base_revision: Option<octacity_server_domain::ImmutableRevision>,
  /// Initial current-state projection.
  pub state: octacity_server_factory::FactoryRunState,
  /// Authoritative original admission time.
  pub admitted_at: Timestamp,
}

/// Common Factory admission over one authoritative store and VCS seam.
pub struct FactoryAdmissionHandlers<S> {
  store: Arc<S>,
  pub(crate) revisions: Arc<dyn RevisionResolver>,
  repositories: Option<Arc<dyn octacity_server_store::ConfigurationStore>>,
  verification_inputs: Option<Arc<dyn octacity_server_store::ProjectInputQueryStore>>,
  policy: Arc<dyn crate::ManagementAuthorizationPolicy>,
}

pub(crate) struct SourceAdmissionSelection<'a> {
  pub input: &'a octacity_server_factory::WorkSourceInput,
  pub handoff: Option<&'a crate::FactorySourceHandoff>,
  pub issue_input: Option<&'a octacity_server_store::ProjectTextDocuments>,
}

impl<S> FactoryAdmissionHandlers<S> {
  /// Uses the same policy to authorize selected immutable dependencies before VCS access.
  #[must_use]
  pub fn with_policy(
    store: Arc<S>,
    revisions: Arc<dyn RevisionResolver>,
    policy: Arc<dyn crate::ManagementAuthorizationPolicy>,
  ) -> Self {
    Self {
      store,
      revisions,
      repositories: None,
      verification_inputs: None,
      policy,
    }
  }

  /// Reuses the Project Repository owner for explicitly declared Flow resources.
  #[must_use]
  pub fn with_repositories(mut self, repositories: Arc<dyn octacity_server_store::ConfigurationStore>) -> Self {
    self.repositories = Some(repositories);
    self
  }

  /// Checks attached private verification files before admitting new Work.
  pub fn with_verification_inputs(mut self, inputs: Arc<dyn octacity_server_store::ProjectInputQueryStore>) -> Self {
    self.verification_inputs = Some(inputs);
    self
  }

  /// Creates handlers from backend-neutral admission and revision ports.
  #[must_use]
  pub fn new(store: Arc<S>, revisions: Arc<dyn RevisionResolver>) -> Self {
    Self::with_policy(store, revisions, Arc::new(crate::TrustedNetworkManagementPolicy))
  }
}

#[async_trait]
impl<S> crate::ManagementCommandUseCase<AdmitManualFactoryWorkCommand> for FactoryAdmissionHandlers<S>
where
  S: FactoryAdmissionStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: AdmitManualFactoryWorkCommand,
  ) -> Result<FactoryAdmissionOutcome, Self::Error> {
    let probe = admission_probe(context, &command)?;
    self
      .admit_intent(
        context,
        command,
        probe,
        None,
        octacity_server_factory::FactoryAdmissionOrigin::Manual,
      )
      .await
  }
}

impl<S: FactoryAdmissionStore + 'static> FactoryAdmissionHandlers<S> {
  /// Gates fresh automatic intake before the worker reserves or prepares per-item inputs.
  pub(crate) async fn require_source_launch(
    &self,
    context: &crate::ManagementRequestContext,
    input: &octacity_server_factory::WorkSourceInput,
  ) -> Result<octacity_server_store::FactoryAdmissionContext, ApplicationError> {
    use crate::factory::authoring::factory_configuration_choices::require_visible_reference;
    let definition = input.definition();
    require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::FactoryConfiguration,
      definition.target.id(),
    )
    .await?;
    require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Repository,
      definition.repository.id,
    )
    .await?;
    let selected = self
      .store
      .factory_admission_context(ReadFactoryAdmissionContext {
        project_id: input.project_id(),
        configuration_id: definition.target.id(),
        configuration_version: definition.target.version(),
        repository_id: Some(definition.repository.id),
        repository_version: Some(definition.repository.version),
      })
      .await?;
    validate_admission_project(&selected, input.project_id())?;
    validate_launch_policy(
      &selected,
      octacity_server_factory::FactoryAdmissionOrigin::AutomaticSource,
    )?;
    Ok(selected)
  }

  pub(crate) async fn authorize_source_preparation(
    &self,
    context: &crate::ManagementRequestContext,
    preparation: &octacity_server_store::WorkSourcePreparation,
  ) -> Result<(), ApplicationError> {
    if let Some(policy) = &preparation.policy {
      let settings: crate::SourcePreparationSettings =
        serde_json::from_slice(policy).map_err(|_| ApplicationError::invalid())?;
      settings
        .authorize(
          self.policy.as_ref(),
          context,
          preparation,
          preparation.issue_input.as_ref(),
        )
        .await?;
    }
    Ok(())
  }
  pub(crate) async fn authorize_source_intent(
    &self,
    context: &crate::ManagementRequestContext,
    command: &FactoryWorkAdmissionIntent,
  ) -> Result<(), ApplicationError> {
    self.authorize_source_project(context, command.project_id).await?;
    self.require_visible_dependencies(context, command).await
  }

  pub(crate) async fn authorize_source_project(
    &self,
    context: &crate::ManagementRequestContext,
    project_id: ProjectId,
  ) -> Result<(), ApplicationError> {
    authorize_project_admission(self.policy.as_ref(), context, project_id).await
  }

  async fn require_visible_dependencies(
    &self,
    context: &crate::ManagementRequestContext,
    command: &FactoryWorkAdmissionIntent,
  ) -> Result<(), ApplicationError> {
    self
      .require_visible_work_references(
        context,
        command.configuration_id,
        command.repository_id,
        &command.artifacts,
      )
      .await
  }

  async fn require_visible_work_references(
    &self,
    context: &crate::ManagementRequestContext,
    configuration_id: FactoryConfigurationId,
    repository_id: Option<RepositoryId>,
    artifacts: &WorkArtifacts,
  ) -> Result<(), ApplicationError> {
    crate::factory::authoring::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::FactoryConfiguration,
      configuration_id,
    )
    .await?;
    if let Some(repository_id) = repository_id {
      crate::factory::authoring::factory_configuration_choices::require_visible_reference(
        self.policy.as_ref(),
        context,
        ManagementResourceKind::Repository,
        repository_id,
      )
      .await?;
    }
    for artifact in std::iter::once(artifacts.task())
      .chain(std::iter::once(artifacts.acceptance()))
      .chain(artifacts.specifications())
    {
      crate::factory::authoring::factory_configuration_choices::require_visible_reference(
        self.policy.as_ref(),
        context,
        ManagementResourceKind::Artifact,
        artifact.artifact_id(),
      )
      .await?;
    }
    Ok(())
  }

  pub(crate) async fn recover_source(
    &self,
    context: &crate::ManagementRequestContext,
    project_id: ProjectId,
    identity: &octacity_server_store::FactoryAdmissionIdentity,
  ) -> Result<Option<octacity_server_store::PublishedFactoryAdmission>, ApplicationError> {
    self.authorize_source_project(context, project_id).await?;
    let Some(receipt) = self.store.recover_factory_admission(identity).await? else {
      return Ok(None);
    };
    if receipt.work.subject().project_id() != project_id || receipt.work.configuration().project_id() != project_id {
      return Err(ApplicationError::invalid());
    }
    self
      .require_visible_work_references(
        context,
        receipt.work.configuration().id(),
        receipt
          .work
          .subject()
          .repository()
          .ok()
          .map(|subject| subject.repository_id()),
        receipt.work.artifacts(),
      )
      .await?;
    crate::factory::execution::factory_code_sources::validate_sources(
      receipt.flow.closure(),
      project_id,
      self.repositories.as_deref(),
      self.policy.as_ref(),
      context,
    )
    .await?;
    Ok(Some(receipt))
  }

  pub(crate) async fn admit_intent(
    &self,
    context: &crate::ManagementRequestContext,
    command: FactoryWorkAdmissionIntent,
    probe: FactoryAdmissionProbe,
    source: Option<SourceAdmissionSelection<'_>>,
    origin: octacity_server_factory::FactoryAdmissionOrigin,
  ) -> Result<FactoryAdmissionOutcome, ApplicationError> {
    if let Some(outcome) = self.store.replay_factory_admission(&probe).await? {
      self.require_visible_dependencies(context, &command).await?;
      crate::factory::execution::factory_code_sources::validate_sources(
        outcome.admission.flow.closure(),
        command.project_id,
        self.repositories.as_deref(),
        self.policy.as_ref(),
        context,
      )
      .await?;
      return Ok(outcome.into());
    }
    let request = self
      .prepare_fresh_intent(context, command, probe, source, origin)
      .await?;
    self
      .store
      .admit_factory_work(audited_mutation(context, request)?)
      .await
      .map(Into::into)
      .map_err(Into::into)
  }

  /// Shared visibility, immutable pin and Repository resolution gates for fresh admission.
  pub(crate) async fn prepare_fresh_intent(
    &self,
    context: &crate::ManagementRequestContext,
    command: FactoryWorkAdmissionIntent,
    probe: FactoryAdmissionProbe,
    source: Option<SourceAdmissionSelection<'_>>,
    origin: octacity_server_factory::FactoryAdmissionOrigin,
  ) -> Result<AdmitFactoryWork, ApplicationError> {
    command.validate_source()?;
    self.require_visible_dependencies(context, &command).await?;
    let admission_context = self
      .store
      .factory_admission_context(ReadFactoryAdmissionContext {
        project_id: command.project_id,
        configuration_id: command.configuration_id,
        configuration_version: command.configuration_version,
        repository_id: command.repository_id,
        repository_version: command.repository_version,
      })
      .await?;
    let configuration = &admission_context.configuration.configuration;
    validate_admission_project(&admission_context, command.project_id)?;
    validate_launch_policy(&admission_context, origin)?;
    crate::factory::validation::factory_verification_supplement::validate_verification_selections(
      configuration.flow(),
      command.project_id,
      self.verification_inputs.as_deref(),
      command.admitted_at,
      None,
    )
    .await?;
    let handoff = source.as_ref().and_then(|selected| selected.handoff);
    let issue_input = source.as_ref().and_then(|selected| selected.issue_input);
    if let Some(documents) = issue_input {
      documents.validate()?;
      if handoff.is_some()
        || configuration.flow().issue_snapshot.as_ref() != documents.issue_snapshot().map(|snapshot| &snapshot.input)
      {
        return Err(ApplicationError::invalid());
      }
    } else if let Some(handoff) = handoff {
      if !handoff.matches_artifacts(&command.artifacts) {
        return Err(crate::FactoryValidationDiagnostics::unavailable_with_code(
          "/incoming_result",
          "incoming_result_stale",
        ));
      }
    } else {
      reject_missing_incoming_result(configuration.flow())?;
    }
    command
      .profile_overrides
      .validate(&configuration.flow().closure, command.project_id)
      .map_err(|_| ApplicationError::invalid())?;
    crate::factory::execution::factory_code_sources::validate_sources(
      &configuration.flow().closure,
      command.project_id,
      self.repositories.as_deref(),
      self.policy.as_ref(),
      context,
    )
    .await?;
    if let Some(selection) = source {
      let input = selection.input;
      let repository = admission_context
        .repository
        .as_ref()
        .ok_or_else(ApplicationError::invalid)?;
      let expected = &input.definition().repository;
      let encoded = serde_json::to_vec(&repository.definition).map_err(|_| ApplicationError::invalid())?;
      if input.definition().target != *configuration.reference()
        || input.root_flow() != configuration.flow().closure.root()
        || expected.project_id != repository.project_id
        || expected.id != repository.id
        || expected.version != repository.version
        || expected.definition_digest != FactoryDigest::content_sha256(&encoded)
      {
        return Err(ApplicationError::invalid());
      }
    }
    let subject = if matches!(
      command.source,
      octacity_server_store::FactoryAdmissionSource::ProjectInput(_)
    ) {
      octacity_server_factory::WorkSubject::project_input(command.project_id, &command.artifacts)
        .map_err(|_| ApplicationError::invalid())?
    } else if let Some(subject) = handoff.and_then(|handoff| handoff.resolved_subject.clone()) {
      subject.into()
    } else {
      let repository = admission_context
        .repository
        .as_ref()
        .ok_or_else(ApplicationError::invalid)?;
      resolve_repository_subject(self.revisions.as_ref(), command.source.repository()?, repository)
        .await?
        .into()
    };
    let pins = crate::factory::execution::factory_code_sources::resolve_used_sources(
      configuration.flow(),
      self.repositories.as_deref(),
      self.revisions.as_ref(),
    )
    .await?;
    let project = command.project_id;
    let mut admission = materialize_subject_admission(command, probe, admission_context, subject)?;
    admission.flow = admission
      .flow
      .with_code_pins(pins, project)
      .map_err(|_| ApplicationError::invalid())?;
    admission.incoming = if let Some(documents) = issue_input {
      let snapshot = documents.issue_snapshot().ok_or_else(ApplicationError::invalid)?;
      let schema = admission
        .flow
        .data_schema(&snapshot.input.schema)
        .ok_or_else(ApplicationError::invalid)?;
      Some(snapshot.incoming(&admission.work, &admission.flow, schema)?)
    } else {
      handoff
        .map(|handoff| {
          octacity_server_factory::FlowIncomingData::from_predecessor(&admission.work, &admission.flow, &handoff.record)
        })
        .transpose()
        .map_err(|_| {
          crate::FactoryValidationDiagnostics::unavailable_with_code(
            "/incoming_result",
            "incoming_result_wrong_subject",
          )
        })?
    };
    Ok(admission)
  }
}

pub(crate) async fn resolve_repository_subject(
  revisions: &dyn RevisionResolver,
  selection: &octacity_server_domain::RepositorySourceSelection,
  repository: &octacity_server_store::PublishedRepository,
) -> Result<octacity_server_factory::ExactSubject, ApplicationError> {
  let selection = select_source(selection, repository).map_err(|_| ApplicationError::invalid())?;
  let revision = revisions
    .resolve(RevisionResolutionRequest {
      repository: repository.clone(),
      selection,
    })
    .await
    .map_err(map_revision_error)?;
  Ok(octacity_server_factory::ExactSubject::new(
    repository.project_id,
    repository.id,
    revision,
  ))
}

pub(crate) fn admission_probe(
  context: &crate::ManagementRequestContext,
  command: &AdmitManualFactoryWorkCommand,
) -> Result<FactoryAdmissionProbe, ApplicationError> {
  Ok(FactoryAdmissionProbe {
    source_scope: FactoryWorkSourceScope {
      source: FactoryKey::new(MANUAL_WORK_SOURCE).map_err(|_| ApplicationError::invalid())?,
      security_scope: context.security_scope().to_store(),
    },
    external_identity: command.external_identity.clone(),
    intent_digest: octacity_server_store::factory_admission_intent_digest(
      command,
      "octacity.factory.manual-admission.v1",
    ),
    idempotency_key: command.idempotency_key.clone(),
  })
}

pub(crate) fn source_admission_probe(
  context: &crate::ManagementRequestContext,
  command: &FactoryWorkAdmissionIntent,
  installation: &octacity_server_factory::ImmutableReference,
) -> Result<FactoryAdmissionProbe, ApplicationError> {
  let source_scope = installed_source_scope(context, command.project_id, installation)?;
  octacity_server_store::factory_source_admission_probe(
    octacity_server_store::FactoryAdmissionIdentity {
      source_scope,
      external_identity: command.external_identity.clone(),
    },
    command,
  )
  .map_err(Into::into)
}

pub(crate) fn installed_source_scope(
  context: &crate::ManagementRequestContext,
  project_id: ProjectId,
  installation: &octacity_server_factory::ImmutableReference,
) -> Result<FactoryWorkSourceScope, ApplicationError> {
  let source = octacity_server_store::installed_source_namespace(project_id, installation.identity());
  Ok(FactoryWorkSourceScope {
    source,
    security_scope: context.security_scope().to_store(),
  })
}

pub(crate) fn map_revision_error(error: RevisionResolutionError) -> ApplicationError {
  match error {
    RevisionResolutionError::NotFound | RevisionResolutionError::Invalid => ApplicationError::invalid(),
    // Missing resolver/integration support is an installation prerequisite, not
    // a transient provider failure that another identical scan can repair.
    RevisionResolutionError::Unavailable => ApplicationError::SourceDependency {
      class: octacity_server_store::WorkSourceFailure::InvalidConfiguration,
      retry_after_millis: None,
    },
    RevisionResolutionError::Transient | RevisionResolutionError::Cancelled => ApplicationError::unavailable(),
  }
}

impl From<FactoryAdmissionMutationOutcome> for FactoryAdmissionOutcome {
  fn from(value: FactoryAdmissionMutationOutcome) -> Self {
    Self {
      disposition: value.disposition.into(),
      work_envelope_id: value.admission.work.id(),
      factory_run_id: value.admission.run.id(),
      base_revision: value
        .admission
        .work
        .subject()
        .repository()
        .ok()
        .map(|subject| subject.base_revision().clone()),
      state: value.admission.run.state(),
      admitted_at: value.admission.admitted_at,
    }
  }
}

// Shared ordinary admission permission gate for prepared and direct intake.
pub(crate) async fn authorize_project_admission(
  policy: &dyn crate::ManagementAuthorizationPolicy,
  context: &crate::ManagementRequestContext,
  project_id: ProjectId,
) -> Result<(), ApplicationError> {
  crate::factory::authoring::factory_configuration_choices::require_visible_reference(
    policy,
    context,
    ManagementResourceKind::Project,
    project_id,
  )
  .await?;
  let resource = owned_collection_resource(
    ManagementResourceKind::FactoryRun,
    ManagementResourceKind::Project,
    project_id,
  )
  .map_err(|_| ApplicationError::invalid())?;
  let grant = policy
    .authorize(context, ManagementAction::Execute, &resource)
    .await
    .map_err(|_| ApplicationError::invalid())?;
  if matches!(grant.visibility().view(), crate::ManagementVisibilityView::None) {
    return Err(ApplicationError::invalid());
  }
  Ok(())
}

/// Uses exact retained contents. Callers assign origin from their trusted ingress, never task metadata.
pub(crate) fn validate_launch_policy(
  selected: &octacity_server_store::FactoryAdmissionContext,
  origin: octacity_server_factory::FactoryAdmissionOrigin,
) -> Result<(), ApplicationError> {
  let policies = selected
    .project_policies
    .as_ref()
    .map_err(|missing| crate::factory::authoring::factory_configuration_policies::prerequisite(*missing))?;
  if !policies.launch_policy.definition().allows(origin) {
    return Err(ApplicationError::FactoryValidation(Box::new(
      crate::FactoryValidationDiagnostics::single_issue("/admission_policy", "launch_policy_denied", None),
    )));
  }
  Ok(())
}

// Shared ordinary Work/Run construction after trusted source resolution.
pub(crate) fn materialize_resolved_admission(
  command: FactoryWorkAdmissionIntent,
  probe: FactoryAdmissionProbe,
  admission_context: octacity_server_store::FactoryAdmissionContext,
  base_revision: octacity_server_domain::ImmutableRevision,
) -> Result<AdmitFactoryWork, ApplicationError> {
  let repository_id = command.repository_id.ok_or_else(ApplicationError::invalid)?;
  let subject = octacity_server_factory::ExactSubject::new(command.project_id, repository_id, base_revision).into();
  materialize_subject_admission(command, probe, admission_context, subject)
}

fn materialize_subject_admission(
  command: FactoryWorkAdmissionIntent,
  probe: FactoryAdmissionProbe,
  admission_context: octacity_server_store::FactoryAdmissionContext,
  subject: octacity_server_factory::WorkSubject,
) -> Result<AdmitFactoryWork, ApplicationError> {
  let configuration = &admission_context.configuration.configuration;
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    configuration.reference().clone(),
    command.external_identity,
    subject,
    command.artifacts,
    command.classification,
  )
  .and_then(|work| work.with_profile_overrides(command.profile_overrides, &configuration.flow().closure))
  .map_err(|_| ApplicationError::invalid())?;
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = octacity_server_factory::AdmittedFlow::from_configuration(configuration, &run)
    .and_then(|flow| {
      let policies = admission_context
        .project_policies
        .as_ref()
        .map_err(|_| octacity_server_factory::FactoryError::InconsistentSubject)?;
      flow.with_access_profile(configuration, policies.access_profile.clone())
    })
    .map_err(|_| ApplicationError::invalid())?;
  Ok(AdmitFactoryWork {
    incoming: None,
    probe,
    repository_version: command.repository_version,
    work,
    run,
    flow,
    admitted_at: command.admitted_at,
  })
}

/// Rejects an admission path that has no retained external input for the pinned Flow.
pub(crate) fn reject_missing_incoming_result(
  flow: &octacity_server_factory::FactoryFlowConfiguration,
) -> Result<(), ApplicationError> {
  if flow.incoming_schemas().next().is_some() {
    return Err(crate::FactoryValidationDiagnostics::unavailable_with_code(
      "/incoming_result",
      "missing_incoming_result",
    ));
  }
  Ok(())
}

// Admission project/enabled checks remain identical across prepared and Artifact ingress.
pub(crate) fn validate_admission_project(
  selected: &octacity_server_store::FactoryAdmissionContext,
  project: ProjectId,
) -> Result<(), ApplicationError> {
  if !selected.configuration.configuration.is_enabled()
    || selected.configuration.configuration.reference().project_id() != project
    || selected
      .repository
      .as_ref()
      .is_some_and(|repository| repository.project_id != project)
  {
    return Err(ApplicationError::invalid());
  }
  Ok(())
}
