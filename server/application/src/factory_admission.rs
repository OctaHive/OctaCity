use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{ProjectId, RepositoryId, RepositoryVersion, Timestamp};
use octacity_server_factory::{
  ExternalWorkIdentity, FactoryConfigurationId, FactoryConfigurationVersion, FactoryDigest, FactoryKey, FactoryRun,
  FactoryRunId, WorkArtifacts, WorkClassification, WorkEnvelope, WorkEnvelopeId,
};
use octacity_server_store::{
  AdmitFactoryWork, FactoryAdmissionMutationOutcome, FactoryAdmissionProbe, FactoryAdmissionStore,
  FactoryWorkSourceScope, IdempotencyKey, ReadFactoryAdmissionContext,
};

use crate::{
  ApplicationError, Command, ManagementAction, ManagementAuthorizationMapping, ManagementAuthorizationTarget,
  ManagementResourceKind, ManagementResourceResult, MutationDisposition, RepositorySourceSelection,
  RevisionResolutionError, RevisionResolutionRequest, RevisionResolver,
  management_security::{audited_mutation, owned_collection_resource},
  manual_trigger::select_source,
};

const MANUAL_WORK_SOURCE: &str = "manual";

/// Provider-neutral manual intent to admit one immutable Work Envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmitManualFactoryWorkCommand {
  /// Existing Project that must own every selected definition.
  pub project_id: ProjectId,
  /// Exact immutable Factory Configuration selected for the run.
  pub configuration_id: FactoryConfigurationId,
  /// Exact immutable Factory Configuration version selected for the run.
  pub configuration_version: FactoryConfigurationVersion,
  /// Exact Repository identity selected as the Work subject.
  pub repository_id: RepositoryId,
  /// Exact immutable Repository definition version used for source resolution.
  pub repository_version: RepositoryVersion,
  /// Stable Work Source identity within the accepted management security scope.
  pub external_identity: ExternalWorkIdentity,
  /// Allowed mutable reference, configured default, or permitted exact revision.
  pub source: RepositorySourceSelection,
  /// Immutable task, acceptance, and optional specification Artifact references.
  pub artifacts: WorkArtifacts,
  /// Bounded priority, risk, and source metadata.
  pub classification: WorkClassification,
  /// Stable transport replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative admission time.
  pub admitted_at: Timestamp,
}

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

/// Safe result of applying or replaying one manual Work admission.
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

/// Manual Factory admission over one authoritative store and VCS seam.
pub struct FactoryAdmissionHandlers<S> {
  store: Arc<S>,
  revisions: Arc<dyn RevisionResolver>,
}

impl<S> FactoryAdmissionHandlers<S> {
  /// Creates handlers from backend-neutral admission and revision ports.
  #[must_use]
  pub fn new(store: Arc<S>, revisions: Arc<dyn RevisionResolver>) -> Self {
    Self { store, revisions }
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
    if let Some(outcome) = self.store.replay_factory_admission(&probe).await? {
      return Ok(outcome.into());
    }

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
    if !configuration.is_enabled()
      || configuration.reference().project_id() != command.project_id
      || admission_context.repository.project_id != command.project_id
    {
      return Err(ApplicationError::invalid());
    }
    let source =
      select_source(&command.source, &admission_context.repository).map_err(|_| ApplicationError::invalid())?;
    let base_revision = self
      .revisions
      .resolve(RevisionResolutionRequest {
        repository: admission_context.repository,
        selection: source,
      })
      .await
      .map_err(map_revision_error)?;
    let work = WorkEnvelope::new(
      WorkEnvelopeId::generate(),
      configuration.reference().clone(),
      command.external_identity,
      octacity_server_factory::ExactSubject::new(command.project_id, command.repository_id, base_revision),
      command.artifacts,
      command.classification,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
    self
      .store
      .admit_factory_work(audited_mutation(
        context,
        AdmitFactoryWork {
          probe,
          repository_version: command.repository_version,
          work,
          run,
          admitted_at: command.admitted_at,
        },
      )?)
      .await
      .map(Into::into)
      .map_err(Into::into)
  }
}

fn admission_probe(
  context: &crate::ManagementRequestContext,
  command: &AdmitManualFactoryWorkCommand,
) -> Result<FactoryAdmissionProbe, ApplicationError> {
  Ok(FactoryAdmissionProbe {
    source_scope: FactoryWorkSourceScope {
      source: FactoryKey::new(MANUAL_WORK_SOURCE).map_err(|_| ApplicationError::invalid())?,
      security_scope: context.security_scope().to_store(),
    },
    external_identity: command.external_identity.clone(),
    intent_digest: admission_intent_digest(command),
    idempotency_key: command.idempotency_key.clone(),
  })
}

fn admission_intent_digest(command: &AdmitManualFactoryWorkCommand) -> FactoryDigest {
  let mut fields = vec![
    command.project_id.to_string().into_bytes(),
    command.configuration_id.to_string().into_bytes(),
    command.configuration_version.get().to_string().into_bytes(),
    command.repository_id.to_string().into_bytes(),
    command.repository_version.get().to_string().into_bytes(),
    command.external_identity.as_str().as_bytes().to_vec(),
  ];
  match &command.source {
    RepositorySourceSelection::DefaultReference => fields.push(b"default_reference".to_vec()),
    RepositorySourceSelection::Reference(reference) => {
      fields.push(b"reference".to_vec());
      fields.push(reference.as_str().as_bytes().to_vec());
    }
    RepositorySourceSelection::ExactRevision(revision) => {
      fields.push(b"exact_revision".to_vec());
      fields.push(revision.as_str().as_bytes().to_vec());
    }
  }
  fields.push(command.artifacts.task().to_string().into_bytes());
  fields.push(command.artifacts.acceptance().to_string().into_bytes());
  for specification in command.artifacts.specifications() {
    fields.push(specification.to_string().into_bytes());
  }
  fields.push(command.classification.priority().get().to_string().into_bytes());
  fields.push(command.classification.risk().as_str().as_bytes().to_vec());
  for (key, value) in command.classification.metadata().iter() {
    fields.push(key.as_bytes().to_vec());
    fields.push(value.as_bytes().to_vec());
  }
  let fields = fields.iter().map(Vec::as_slice).collect::<Vec<_>>();
  FactoryDigest::sha256("octacity.factory.manual-admission.v1", &fields)
}

fn map_revision_error(error: RevisionResolutionError) -> ApplicationError {
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
