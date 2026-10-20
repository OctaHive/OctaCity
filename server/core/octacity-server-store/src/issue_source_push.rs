//! Original desired Source state; publication owns authority, Reporter owns native effects.
use crate::*;
use octacity_server_domain::IssueStatusId;
use octacity_server_factory::{WorkSourceBinding, WorkSourceStatusEffects};
use serde::{Deserialize, Serialize};

/// Credential-free command attribution retained with an accepted desired state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssuePushCommand {
  /// Original scoped operation; retries cannot select a later revision or mapping.
  pub idempotency_key: IdempotencyKey,
  /// Accepted management partition, never a provider credential.
  pub security_scope: String,
  /// Actual accepted actor, independently of task assignment.
  pub actor: AuditActor,
  /// Original accepted Factory action, absent for a manual metadata command.
  pub pipeline: Option<IssuePipelineActionProvenance>,
}
/// Exact material for the existing finite Reporter effects contract, not a dispatch permit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceDesiredState {
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
      || link.linked_at > issue.updated_at
      || mapping.published_at > issue.updated_at
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
        idempotency_key: key.clone(),
        security_scope: audit.security_scope().as_str().into(),
        actor: audit.actor().clone(),
        pipeline: history.and_then(|h| h.pipeline.clone()),
      },
    })
  }
}
