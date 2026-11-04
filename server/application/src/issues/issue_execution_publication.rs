//! Issue-specific review feeds the single Project text intake; it never publishes bytes itself.
use crate::*;
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactByteReader};
use octacity_server_domain::{IssueExecutionRequestId, IssueId, ProjectId};
use octacity_server_factory::{FactoryDigest, FactoryKey, WorkClassification};
use octacity_server_store::{
  AssociateIssueWork, FactoryRunStore, IssueExecutionStore, IssueStore, ProjectTextInputStore,
  ReadFactoryAdmissionContext,
};
use std::sync::Arc;
/// Processes an already reviewed request with explicit ordinary admission classification.
#[derive(Clone, Debug)]
pub struct PublishIssueExecutionCommand {
  /// Original owning Project.
  pub project_id: ProjectId,
  /// Original visible Issue.
  pub issue_id: IssueId,
  /// Exact retained request; current text is never silently selected.
  pub request_id: IssueExecutionRequestId,
  /// Explicit reviewed classification; must match the request when present.
  pub classification: WorkClassification,
}
impl Command for PublishIssueExecutionCommand {
  type Outcome = IssueExecutionRequest;
}
impl ManagementAuthorizationTarget for PublishIssueExecutionCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = RequestIssueExecutionCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
/// Narrow Issue adapter over shared text publication and original Work association.
pub struct IssueExecutionPublisher<S, F, B> {
  issues: Arc<S>,
  factory: Arc<F>,
  intake: ProjectTextIntake<F, B>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
}
impl<S, F: ProjectTextInputStore + 'static, B: ArtifactBytePublisher + ArtifactByteReader>
  IssueExecutionPublisher<S, F, B>
{
  /// Uses exactly the same publication operation available to quickstart ingress.
  pub fn new(
    issues: Arc<S>,
    factory: Arc<F>,
    bytes: Arc<B>,
    revisions: Arc<dyn RevisionResolver>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    let intake = ProjectTextIntake::new(factory.clone(), bytes, revisions, policy.clone(), clock);
    Self {
      issues,
      factory,
      intake,
      policy,
    }
  }
  /// Installs Project Repository resolution on the shared intake.
  pub fn with_repositories(mut self, repositories: Arc<dyn octacity_server_store::ConfigurationStore>) -> Self {
    self.intake = self.intake.with_repositories(repositories);
    self
  }
}
#[async_trait::async_trait]
impl<
  S: IssueStore + IssueExecutionStore,
  F: ProjectTextInputStore + FactoryRunStore + 'static,
  B: ArtifactBytePublisher + ArtifactByteReader,
> ManagementCommandUseCase<PublishIssueExecutionCommand> for IssueExecutionPublisher<S, F, B>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: PublishIssueExecutionCommand,
  ) -> Result<IssueExecutionRequest, ApplicationError> {
    crate::issues::issue::visible_issue(grant, command.issue_id)?;
    crate::factory::authoring::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Issue,
      command.issue_id,
    )
    .await?;
    let request = self
      .issues
      .issue_execution_for_admission(
        command.project_id,
        command.issue_id,
        command.request_id,
        &context.security_scope().to_store(),
      )
      .await?;
    if request
      .intent
      .classification
      .as_ref()
      .is_some_and(|reviewed| reviewed != &command.classification)
    {
      return Err(
        octacity_server_store::StoreError::Conflict {
          entity: octacity_server_domain::EntityKind::Issue,
        }
        .into(),
      );
    }
    let target = &request.intent.target;
    let selected = ReadFactoryAdmissionContext {
      project_id: command.project_id,
      configuration_id: target.configuration_id,
      configuration_version: target.configuration_version,
      repository_id: Some(target.repository_id),

      repository_version: Some(target.repository_version),
    };
    crate::management::project_text_intake::authorize_project_text(self.policy.as_ref(), context, &selected).await?;
    let original = self
      .factory
      .project_text_preparation(
        command.project_id,
        &context.security_scope().to_store(),
        &request.admission_key(),
      )
      .await?;
    let admission = if let Some(admission) = original.as_ref().and_then(|p| p.admission.as_ref()) {
      if original
        .as_ref()
        .is_none_or(|p| p.intent.classification != command.classification)
      {
        return Err(
          octacity_server_store::StoreError::Conflict {
            entity: octacity_server_domain::EntityKind::Issue,
          }
          .into(),
        );
      }
      // Both completed replay and interrupted linkage use retained metadata after body retirement.
      admission.clone()
    } else {
      let predecessor = if target.predecessor_run.is_some() {
        let configuration = self.factory.factory_admission_context(selected).await?;
        crate::issues::issue_execution::resolve_issue_predecessor(
          self.issues.as_ref(),
          self.factory.as_ref(),
          self.policy.as_ref(),
          context,
          &request.intent,
          configuration.configuration.configuration.flow(),
        )
        .await?
      } else {
        None
      };
      let predecessor_digest = predecessor
        .as_ref()
        .map(|handoff| handoff.record.digest())
        .transpose()
        .map_err(|_| ApplicationError::invalid())?;
      let documents = if original.is_some() {
        self
          .factory
          .project_text_documents(
            command.project_id,
            &context.security_scope().to_store(),
            &request.admission_key(),
          )
          .await?
          .ok_or_else(ApplicationError::unavailable)?
      } else {
        let revision = self
          .issues
          .issue(
            command.project_id,
            command.issue_id,
            Some(request.intent.expected_version),
          )
          .await?;
        crate::issues::issue_execution::issue_execution_documents(&revision.content, &target.mapping)?
      };
      if FactoryDigest::content_sha256(documents.task()) != request.task_digest
        || documents.task().len() as u64 != request.task_size
        || FactoryDigest::content_sha256(documents.acceptance()) != request.acceptance_digest
        || documents.acceptance().len() as u64 != request.acceptance_size
      {
        return Err(
          octacity_server_store::StoreError::Conflict {
            entity: octacity_server_domain::EntityKind::Issue,
          }
          .into(),
        );
      }
      self
        .intake
        .submit_with_predecessor(
          context,
          ProjectTextSubmission {
            intent: ProjectTextIntent {
              predecessor_digest,
              target: selected,
              external_identity: request.external_identity(),
              source: target.source.clone(),
              classification: command.classification,
              mapping: FactoryKey::new("issue.title-description-acceptance.v1")
                .map_err(|_| ApplicationError::invalid())?,
              idempotency_key: request.admission_key(),
            },
            documents,
          },
          predecessor.as_ref().map(|handoff| &handoff.record),
        )
        .await?
        .admission
    };
    let linked = self
      .issues
      .associate_issue_work(crate::management_security::audited_mutation(
        context,
        AssociateIssueWork {
          project_id: command.project_id,
          issue_id: command.issue_id,
          request_id: Some(request.id),
          admission,
        },
      )?)
      .await?;
    Ok(IssueExecutionRequest {
      association: Some(linked.association),
      ..request
    })
  }
}
