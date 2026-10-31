//! Reuses the existing exact Issue/Work/Run journal instead of accepting caller lineage claims.
use super::*;
#[async_trait::async_trait]
impl<S: IssueExecutionStore + IssueStore, F: FactoryDiscoveryStore + octacity_server_store::FactoryRunStore + 'static>
  crate::IssueCodeReviewExecutionReader for IssueExecutionHandlers<S, F>
{
  async fn verify(
    &self,
    context: &ManagementRequestContext,
    binding: &crate::IssueCodeReviewBinding,
    selection: &crate::IssueCodeReviewSelection,
    delivery: bool,
  ) -> Result<Option<crate::IssueCodeReviewPublication>, ApplicationError> {
    let expected = &binding.association;
    let resource = crate::management_security::instance_resource(ManagementResourceKind::Issue, expected.issue_id)
      .map_err(|_| ApplicationError::invalid())?;
    let grant = self
      .policy
      .authorize(context, ManagementAction::View, &resource)
      .await
      .map_err(|_| ApplicationError::invalid())?;
    let association = self
      .visible_issue_work(
        &grant,
        &GetIssueWorkExecutionQuery {
          project_id: expected.project_id,
          issue_id: expected.issue_id,
          work_id: expected.work_id,
        },
      )
      .await?;
    if association != *expected {
      return Err(ApplicationError::invalid());
    }
    self.visible_execution_run(context, &association).await?;
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Repository,
      binding.candidate.exact().repository_id(),
    )
    .await?;
    let snapshot = self.factory.factory_run_snapshot(association.run_id).await?;
    let record = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|r| r.digest().ok() == Some(binding.record))
      .ok_or_else(ApplicationError::invalid)?;
    if record.input().subject() != snapshot.work.subject()
      || record.input().exact_resource().ok() != Some(binding.candidate.exact())
      || record.discovery().and_then(|d| d.published_candidate()) != Some(&binding.candidate)
    {
      return Err(ApplicationError::invalid());
    }
    if !delivery {
      return Ok(None);
    }
    let node = snapshot
      .admitted_flow
      .closure()
      .definition(record.input().definition())
      .and_then(|definition| definition.node(record.input().node()))
      .ok_or_else(ApplicationError::invalid)?;
    let action = node
      .action()
      .filter(|action| {
        node.kind() == octacity_server_factory::FlowNodeKind::TrustedAction
          && action.action().as_str() == crate::factory_candidate_publication::PUBLISH_CANDIDATE_ACTION
      })
      .ok_or_else(ApplicationError::invalid)?;
    let target = crate::factory_candidate_publication::publication_target(action)?;
    // Only the existing verified publication shape can establish Delivery provenance.
    let payload = record.payload().value();
    if payload.get("candidate_revision") != Some(&serde_json::json!(binding.candidate.candidate_revision()))
      || payload.get("changeset_digest") != Some(&serde_json::json!(binding.candidate.changeset_digest()))
      || payload.pointer("/external/remote_head") != Some(&serde_json::json!(binding.candidate.candidate_revision()))
      || payload.pointer("/external/review_identity").and_then(|v| v.as_str()) != Some(selection.item.item.as_str())
    {
      return Err(ApplicationError::invalid());
    }
    let receipt = payload
      .get("receipt")
      .and_then(|v| v.as_str())
      .ok_or_else(ApplicationError::invalid)?
      .parse()
      .map_err(|_| ApplicationError::invalid())?;
    Ok(Some(crate::IssueCodeReviewPublication { receipt, target }))
  }
}
