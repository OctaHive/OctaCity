//! Original desired Source state; publication owns authority, Reporter owns native effects.
use crate::*;
use octacity_server_domain::IssueStatusId;
use octacity_server_factory::{WorkSourceBinding, WorkSourceStatusEffects};
use serde::{Deserialize, Serialize};

/// Immutable local revision owning a Reporter effect, independently of execution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueProjectionSubject {
  /// Owning Project.
  pub project_id: octacity_server_domain::ProjectId,
  /// Original local task.
  pub issue_id: octacity_server_domain::IssueId,
  /// Accepted revision; dispatch never substitutes the current head.
  pub version: octacity_server_domain::IssueVersion,
}

/// Credential-free command attribution retained with an accepted desired state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssuePushCommand {
  /// Original failed effect resolved by this new reviewed policy command.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub resolves: Option<octacity_server_factory::FactoryDigest>,
  /// Original scoped operation; retries cannot select a later revision or mapping.
  pub idempotency_key: IdempotencyKey,
  /// Accepted management partition, never a provider credential.
  pub security_scope: String,
  /// Actual accepted actor, independently of task assignment.
  pub actor: AuditActor,
  /// Original accepted Factory action, absent for a manual metadata command.
  pub pipeline: Option<IssuePipelineActionProvenance>,
}
/// Credential-free origin of managed writer authority, retained only in private publication material.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceWriterAccess {
  /// Original independently managed connection.
  pub connection_id: octacity_server_domain::IntegrationId,
  /// Reviewed metadata version; current grants and enablement are still checked at dispatch.
  pub connection_version: octacity_server_domain::IntegrationVersion,
  /// Underlying managed capability configuration when the accepted binding uses an input alias.
  pub configuration: Option<octacity_server_factory::ImmutableReference>,
}
/// Exact material for the existing finite Reporter effects contract, not a dispatch permit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceDesiredState {
  /// Original managed authority, absent only for trusted legacy writer publications.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub writer_access: Option<IssueSourceWriterAccess>,
  /// Original primary native association; later attachment cannot populate this value.
  pub link: IssueSourceLink,
  /// Exact independently authorized writer from the accepted mapping.
  pub reporter: WorkSourceBinding,
  /// Original business vocabulary, separate from the adapter's native state.
  pub workflow: IssueWorkflowPin,
  /// Accepted internal status; its spelling never selects a native operation.
  pub status_id: IssueStatusId,
  /// Exact configured effects; absence explicitly means the status is unmapped.
  pub effects: Option<WorkSourceStatusEffects>,
  /// Original metadata or configured-action producer.
  pub command: IssuePushCommand,
}
impl IssuePushIntent {
  /// Stable operation and immutable input for the existing deferred-effect journal.
  pub fn projection_record(&self, accepted_at: octacity_server_domain::Timestamp) -> Option<FactoryOutboxRecord> {
    self.desired.as_ref()?;
    let bytes = serde_json::to_vec(self).ok()?;
    let digest = octacity_server_factory::FactoryDigest::sha256("octacity.issue.push.v1", &[&bytes]);
    Some(FactoryOutboxRecord::pending(
      digest,
      crate::FactoryOutboxSubject::Issue(IssueProjectionSubject {
        project_id: self.project_id,
        issue_id: self.issue_id,
        version: self.version,
      }),
      octacity_server_factory::FactoryKey::new("issue.status.projection").expect("static key"),
      digest,
      accepted_at,
      accepted_at,
    ))
  }
  /// Resolves only original publication facts; a later mapping/link cannot retrofit an intent.
  pub fn desired_state(
    &self,
    issue: &IssueRevision,
    link: &IssueSourceLink,
    mapping: &PublishedIssueSourceMapping,
    audit: &MutationAuditContext,
    key: &IdempotencyKey,
    history: Option<&IssueStatusHistoryEntry>,
  ) -> Result<IssueSourceDesiredState, StoreError> {
    self.desired_state_at(issue, link, mapping, audit, key, history, issue.updated_at)
  }
  /// Freezes a separately accepted policy-resolution time without changing the Issue revision.
  #[allow(clippy::too_many_arguments)]
  pub fn desired_state_at(
    &self,
    issue: &IssueRevision,
    link: &IssueSourceLink,
    mapping: &PublishedIssueSourceMapping,
    audit: &MutationAuditContext,
    key: &IdempotencyKey,
    history: Option<&IssueStatusHistoryEntry>,
    accepted_at: octacity_server_domain::Timestamp,
  ) -> Result<IssueSourceDesiredState, StoreError> {
    let invalid = crate::issue_source_mapping::mapping_conflict;
    let status = mapping.definition.status.as_ref().ok_or_else(invalid)?;
    if self.project_id != issue.project_id
      || self.issue_id != issue.id
      || self.version != issue.version
      || self.mapping != mapping.reference()
      || mapping.issue_id != issue.id
      || mapping.project_id != issue.project_id
      || link.issue_id != issue.id
      || link.selection.project_id != issue.project_id
      || accepted_at < issue.updated_at
      || link.linked_at > accepted_at
      || mapping.published_at > accepted_at
      || !mapping.definition.push_enabled
      || self.fields != [IssueField::Status].into()
      || mapping.definition.fields.get(&IssueField::Status) != Some(&IssueSourceFieldAuthority::OctacityOwnedPush)
      || status.workflow
        != (IssueWorkflowPin {
          id: issue.workflow_id,
          version: issue.workflow_version,
        })
      || history.is_some_and(|h| {
        h.source.is_some() || h.issue_id != issue.id || h.version != issue.version || h.to != issue.status_id
      })
    {
      return Err(invalid());
    }
    Ok(IssueSourceDesiredState {
      writer_access: None,
      link: link.clone(),
      reporter: mapping.definition.reporter.clone().ok_or_else(invalid)?,
      workflow: status.workflow.clone(),
      status_id: issue.status_id,
      effects: status
        .push
        .as_ref()
        .and_then(|p| p.effects(&issue.status_id.to_string()))
        .cloned(),
      command: IssuePushCommand {
        resolves: None,
        idempotency_key: key.clone(),
        security_scope: audit.security_scope().as_str().into(),
        actor: audit.actor().clone(),
        pipeline: history.and_then(|h| h.pipeline.clone()),
      },
    })
  }
}

/// Original desired material and current independently fenced delivery record.
#[derive(Clone, Debug)]
pub struct IssuePushProjection {
  /// Immutable accepted desired state, never reconstructed from the current mapping.
  pub intent: IssuePushIntent,
  /// Immutable original publication time.
  pub accepted_at: octacity_server_domain::Timestamp,
  /// Latest existing Reporter outbox state.
  pub record: FactoryOutboxRecord,
}
/// Narrow Reporter read over already committed Issue effects; no metadata mutation or admission.
#[async_trait::async_trait]
pub trait IssueSourceProjectionStore: Send + Sync {
  /// Private current producer policy for the canonical native ticket of an admitted Source Work.
  /// Derive native identity/security partition from retained admission, never a caller-selected card.
  /// This read grants no write authority and must not import, attach or refresh an Issue.
  async fn work_source_issue_push_mapping(
    &self,
    _project: octacity_server_domain::ProjectId,
    _work: octacity_server_factory::WorkEnvelopeId,
  ) -> Result<Option<crate::PublishedIssueSourceMapping>, StoreError> {
    Err(StoreError::Unavailable)
  }
  /// Bounded live membership in operation-identity order, restricted to the original native partition.
  /// Returned material is private; public readers must project an explicit safe allowlist.
  async fn issue_push_projections(
    &self,
    _project: octacity_server_domain::ProjectId,
    _issue: octacity_server_domain::IssueId,
    _scope: &ManagementSecurityScope,
    _after: Option<octacity_server_factory::FactoryDigest>,
    _limit: u16,
  ) -> Result<Vec<IssuePushProjection>, StoreError> {
    Err(StoreError::Unavailable)
  }
  /// Reads original material and latest delivery state by stable operation identity.
  async fn issue_push_projection(
    &self,
    _operation: octacity_server_factory::FactoryDigest,
  ) -> Result<IssuePushProjection, StoreError> {
    Err(StoreError::Unavailable)
  }
}
// This fixture backend owns Factory records only; it cannot invent an Issue publication.
#[cfg(any(test, feature = "test-support"))]
#[async_trait::async_trait]
impl IssueSourceProjectionStore for crate::factory_configuration_testing::InMemoryFactoryConfigurationStore {
  async fn work_source_issue_push_mapping(
    &self,
    _: octacity_server_domain::ProjectId,
    _: octacity_server_factory::WorkEnvelopeId,
  ) -> Result<Option<crate::PublishedIssueSourceMapping>, StoreError> {
    Ok(None)
  }
}
