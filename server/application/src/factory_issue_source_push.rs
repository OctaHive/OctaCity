//! Qualifies one reviewed push producer from the original accepted status-action chain.
use super::*;
use octacity_server_store::{
  FactoryRunSnapshot, FactorySourceStore, IssuePushReview, IssueRevision, IssueSourceMappingStore,
};
impl<S: IssueSourceMappingStore, F: FactoryRunStore + FactorySourceStore> FactoryIssueStatusAction<S, F> {
  pub(super) async fn review_source_push(
    &self,
    snapshot: &FactoryRunSnapshot,
    issue: &IssueRevision,
    selection: &IssuePipelineStatusSelection,
    input: &FlowNodeInput,
  ) -> Result<IssuePushReview, ApplicationError> {
    let policy = self.issues.issue_field_policy(issue.project_id, issue.id).await?;
    let Some(push) = policy.push else {
      return Ok(IssuePushReview { mapping: None });
    };
    let scope = self.context.security_scope().to_store();
    let link = self
      .issues
      .issue_source_link(issue.project_id, issue.id, &scope)
      .await?
      .ok_or_else(ApplicationError::capability_unavailable)?;
    let mapping = self
      .issues
      .issue_source_mapping(issue.project_id, issue.id, &scope, None)
      .await?
      .filter(|m| m.reference() == push.mapping)
      .ok_or_else(ApplicationError::capability_unavailable)?;
    let workflow = self
      .issues
      .issue_workflow(
        issue.project_id,
        selection.workflow_id,
        Some(selection.workflow_version),
      )
      .await?;
    let status = workflow
      .definition
      .transitions
      .iter()
      .find(|e| e.id == selection.transition_id)
      .ok_or_else(ApplicationError::invalid)?
      .to;
    // A direct action can target only the native item frozen by its Source admission.
    // A later card attachment cannot supply an item for an old unlinked Work action.
    if let Some(preparation) = self
      .runs
      .source_preparation_for_work(issue.project_id, snapshot.work.id())
      .await?
      && preparation.item == link.selection.item
      && preparation.input.definition().source.installation.identity() == link.selection.source.installation.identity()
    {
      let definition = snapshot
        .admitted_flow
        .closure()
        .definition(input.definition())
        .ok_or_else(ApplicationError::invalid)?;
      let updates = adjacent_source_updates(
        definition,
        input.node(),
        &selection.outcome,
        preparation.input.definition(),
      )?;
      for update in updates.iter().flatten() {
        update.validate_for_input(preparation.input.definition())?;
      }
      crate::validate_issue_push_producers(&mapping, status, &updates)?;
    }
    Ok(IssuePushReview {
      mapping: Some(push.mapping),
    })
  }
}
// Only consecutive configured status actions share this outcome's producer decision.
// Another Issue transition or execution stage starts its own explicit status decision.
fn adjacent_source_updates(
  definition: &FlowDefinition,
  node: &FactoryKey,
  outcome: &FactoryKey,
  input: &WorkSourceInputDraft,
) -> Result<Vec<Option<crate::FactoryStageSourceUpdate>>, ApplicationError> {
  let mut next = vec![(node.clone(), Some(outcome.clone()))];
  let mut previous = vec![node.clone()];
  let mut seen = std::collections::BTreeSet::new();
  let mut updates = vec![];
  while let Some((key, outcome)) = next.pop() {
    for edge in definition
      .transitions()
      .iter()
      .filter(|e| e.predecessor() == &key && outcome.as_ref().is_none_or(|o| e.outcome() == o))
    {
      if let FlowTransitionTarget::Node(target) = edge.target()
        && let Some(binding) = definition
          .node(target)
          .and_then(|n| n.action())
          .filter(|b| b.action().as_str() == "set_status")
        && seen.insert(target.clone())
      {
        collect_source_update(binding, input, &mut updates)?;
        next.push((target.clone(), None));
      }
    }
  }
  while let Some(key) = previous.pop() {
    for edge in definition
      .transitions()
      .iter()
      .filter(|e| matches!(e.target(),FlowTransitionTarget::Node(k) if k==&key))
    {
      let target = edge.predecessor();
      if let Some(binding) = definition
        .node(target)
        .and_then(|n| n.action())
        .filter(|b| b.action().as_str() == "set_status")
        && seen.insert(target.clone())
      {
        collect_source_update(binding, input, &mut updates)?;
        previous.push(target.clone());
      }
    }
  }
  Ok(updates)
}

fn collect_source_update(
  binding: &FlowActionBinding,
  input: &WorkSourceInputDraft,
  updates: &mut Vec<Option<crate::FactoryStageSourceUpdate>>,
) -> Result<(), ApplicationError> {
  let parameters = crate::factory_flow_action::StatusParameters::from_binding(binding)?;
  if let Some(update) = parameters.source_update {
    updates.push(Some(update));
  } else if let Some(reporter) = input.reporter.as_ref().filter(|r| &r.capability == binding.plugin()) {
    if let Some(mapping) = &input.reporting {
      if let Some(effects) = mapping.effects(parameters.status.as_str()) {
        updates.push(Some(crate::FactoryStageSourceUpdate {
          source: input.source.clone(),
          reporter: reporter.clone(),
          connection: input.connection.clone(),
          effects: effects.clone(),
        }));
      }
    } else {
      updates.push(None);
    }
  }
  Ok(())
}
