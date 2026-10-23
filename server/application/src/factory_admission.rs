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
  manual_trigger::select_source,
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
  /// Exact immutable base revision resolved by the original admission.
  pub base_revision: octacity_server_domain::ImmutableRevision,
  /// Initial current-state projection.
  pub state: octacity_server_factory::FactoryRunState,
  /// Authoritative original admission time.
  pub admitted_at: Timestamp,
}

/// Common Factory admission over one authoritative store and VCS seam.
pub struct FactoryAdmissionHandlers<S> {
  store: Arc<S>,
  revisions: Arc<dyn RevisionResolver>,
  policy: Arc<dyn crate::ManagementAuthorizationPolicy>,
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
      policy,
    }
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
    self.admit_intent(context, command, probe, None).await
  }
}

impl<S: FactoryAdmissionStore + 'static> FactoryAdmissionHandlers<S> {
  pub(super) async fn authorize_source_intent(
    &self,
    context: &crate::ManagementRequestContext,
    command: &FactoryWorkAdmissionIntent,
  ) -> Result<(), ApplicationError> {
    self.authorize_source_project(context, command.project_id).await?;
    self.require_visible_dependencies(context, command).await
  }

  pub(super) async fn authorize_source_project(
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
    repository_id: RepositoryId,
    artifacts: &WorkArtifacts,
  ) -> Result<(), ApplicationError> {
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::FactoryConfiguration,
      configuration_id,
    )
    .await?;
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Repository,
      repository_id,
    )
    .await?;
    for artifact in std::iter::once(artifacts.task())
      .chain(std::iter::once(artifacts.acceptance()))
      .chain(artifacts.specifications())
    {
      crate::factory_configuration_choices::require_visible_reference(
        self.policy.as_ref(),
        context,
        ManagementResourceKind::Artifact,
        artifact.artifact_id(),
      )
      .await?;
    }
    Ok(())
  }

  pub(super) async fn recover_source(
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
        receipt.work.subject().repository_id(),
        receipt.work.artifacts(),
      )
      .await?;
    Ok(Some(receipt))
  }

  pub(super) async fn admit_intent(
    &self,
    context: &crate::ManagementRequestContext,
    command: FactoryWorkAdmissionIntent,
    probe: FactoryAdmissionProbe,
    pinned_input: Option<&octacity_server_factory::WorkSourceInput>,
  ) -> Result<FactoryAdmissionOutcome, ApplicationError> {
    if let Some(outcome) = self.store.replay_factory_admission(&probe).await? {
      return Ok(outcome.into());
    }
    let request = self.prepare_fresh_intent(context, command, probe, pinned_input).await?;
    self
      .store
      .admit_factory_work(audited_mutation(context, request)?)
      .await
      .map(Into::into)
      .map_err(Into::into)
  }

  /// Shared visibility, immutable pin and Repository resolution gates for fresh admission.
  pub(super) async fn prepare_fresh_intent(
    &self,
    context: &crate::ManagementRequestContext,
    command: FactoryWorkAdmissionIntent,
    probe: FactoryAdmissionProbe,
    pinned_input: Option<&octacity_server_factory::WorkSourceInput>,
  ) -> Result<AdmitFactoryWork, ApplicationError> {
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
    command
      .profile_overrides
      .validate(&configuration.flow().closure, command.project_id)
      .map_err(|_| ApplicationError::invalid())?;
    if let Some(input) = pinned_input {
      let repository = &admission_context.repository;
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
    let source =
      select_source(&command.source, &admission_context.repository).map_err(|_| ApplicationError::invalid())?;
    let base_revision = self
      .revisions
      .resolve(RevisionResolutionRequest {
        repository: admission_context.repository.clone(),
        selection: source,
      })
      .await
      .map_err(map_revision_error)?;
    materialize_resolved_admission(command, probe, admission_context, base_revision)
  }
}

pub(super) fn admission_probe(
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

pub(super) fn source_admission_probe(
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

pub(super) fn installed_source_scope(
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

pub(super) fn map_revision_error(error: RevisionResolutionError) -> ApplicationError {
  match error {
    RevisionResolutionError::NotFound | RevisionResolutionError::Invalid => ApplicationError::invalid(),
    RevisionResolutionError::Transient | RevisionResolutionError::Cancelled | RevisionResolutionError::Unavailable => {
      ApplicationError::unavailable()
    }
  }
}

impl From<FactoryAdmissionMutationOutcome> for FactoryAdmissionOutcome {
  fn from(value: FactoryAdmissionMutationOutcome) -> Self {
    Self {
      disposition: value.disposition.into(),
      work_envelope_id: value.admission.work.id(),
      factory_run_id: value.admission.run.id(),
      base_revision: value.admission.work.subject().base_revision().clone(),
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
  crate::factory_configuration_choices::require_visible_reference(
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

// Shared ordinary Work/Run construction after trusted source resolution.
pub(super) fn materialize_resolved_admission(
  command: FactoryWorkAdmissionIntent,
  probe: FactoryAdmissionProbe,
  admission_context: octacity_server_store::FactoryAdmissionContext,
  base_revision: octacity_server_domain::ImmutableRevision,
) -> Result<AdmitFactoryWork, ApplicationError> {
  let configuration = &admission_context.configuration.configuration;
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    configuration.reference().clone(),
    command.external_identity,
    octacity_server_factory::ExactSubject::new(command.project_id, command.repository_id, base_revision),
    command.artifacts,
    command.classification,
  )
  .and_then(|work| work.with_profile_overrides(command.profile_overrides, &configuration.flow().closure))
  .map_err(|_| ApplicationError::invalid())?;
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = octacity_server_factory::AdmittedFlow::from_configuration(configuration, &run)
    .map_err(|_| ApplicationError::invalid())?;
  Ok(AdmitFactoryWork {
    probe,
    repository_version: command.repository_version,
    work,
    run,
    flow,
    admitted_at: command.admitted_at,
  })
}

// Admission project/enabled checks remain identical across prepared and Artifact ingress.
pub(super) fn validate_admission_project(
  selected: &octacity_server_store::FactoryAdmissionContext,
  project: ProjectId,
) -> Result<(), ApplicationError> {
  if !selected.configuration.configuration.is_enabled()
    || selected.configuration.configuration.reference().project_id() != project
    || selected.repository.project_id != project
  {
    return Err(ApplicationError::invalid());
  }
  Ok(())
}
