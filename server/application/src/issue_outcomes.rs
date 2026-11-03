//! Issue-scoped read of current accepted outcomes with existing Run visibility.
use super::*;
use octacity_server_store::{IssueCodeReviewStore, IssueOutcomeStore};
use std::{collections::BTreeSet, sync::Arc};

pub use octacity_server_store::{IssueOutcomeContent, MAX_ISSUE_OUTCOME_CARDS};

/// Safe result facts and independently visible resource links.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueOutcomeCard {
  /// Accepted outcome and original execution.
  pub summary: octacity_server_store::IssueOutcomeCard,
  /// Visible resources only.
  pub links: Vec<IssueOutcomeLink>,
}

/// Reads the latest applicable accepted result per presentation slot.
#[derive(Clone, Debug)]
pub struct GetIssueOutcomesQuery {
  /// Exact owning Project.
  pub project_id: ProjectId,
  /// Exact visible Issue.
  pub issue_id: IssueId,
}
impl Query for GetIssueOutcomesQuery {
  type Outcome = Vec<IssueOutcomeCard>;
}
impl ManagementAuthorizationTarget for GetIssueOutcomesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}

/// Read-only composition of current Issue outcomes and existing linked resource owners.
pub struct IssueOutcomeHandlers<S> {
  store: Arc<S>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  links: IssueOutcomeLinkReaders<S>,
}
impl<S> IssueOutcomeHandlers<S> {
  /// Shares the existing Issue and Run visibility policy and retained metadata readers.
  pub fn new(store: Arc<S>, policy: Arc<dyn ManagementAuthorizationPolicy>, links: IssueOutcomeLinkReaders<S>) -> Self {
    Self { store, policy, links }
  }
}

#[async_trait::async_trait]
impl<S: IssueCodeReviewStore + IssueOutcomeStore> ManagementQueryUseCase<GetIssueOutcomesQuery>
  for IssueOutcomeHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueOutcomesQuery,
  ) -> Result<Vec<IssueOutcomeCard>, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue_id)?;
    let issue = self.store.issue(query.project_id, query.issue_id, None).await?;
    if crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Project,
      query.project_id,
    )
    .await
    .is_err()
    {
      return Ok(Vec::new());
    }
    let resource = crate::management_security::owned_collection_resource(
      ManagementResourceKind::FactoryRun,
      ManagementResourceKind::Project,
      query.project_id,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let Ok(run_grant) = self.policy.authorize(context, ManagementAction::View, &resource).await else {
      return Ok(Vec::new());
    };
    let visibility = run_grant
      .visibility_for::<ListFactoryRunsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let mut after = None;
    let mut current_run = None;
    let mut run_visible = false;
    let mut slots = BTreeSet::new();
    let mut cards = Vec::new();
    let limit = usize::from(MAX_ISSUE_OUTCOME_CARDS);
    // A denied or superseded occurrence never claims a slot. Walk the ordered
    // metadata until every remaining candidate is exhausted or the response is full.
    while cards.len() < limit {
      let candidates = self.store.issue_outcome_candidates(&issue, &visibility, after).await?;
      let count = candidates.len();
      for candidate in candidates {
        after = Some(candidate.position);
        if slots.contains(&candidate.slot) {
          continue;
        }
        let run_id = candidate.position.run.run_id;
        if current_run != Some(run_id) {
          current_run = Some(run_id);
          run_visible = crate::factory_configuration_choices::require_visible_reference(
            self.policy.as_ref(),
            context,
            ManagementResourceKind::FactoryRun,
            run_id,
          )
          .await
          .is_ok();
        }
        if !run_visible {
          continue;
        }
        let Some(record) = self.store.issue_outcome_record(&issue, &candidate).await? else {
          continue;
        };
        let references =
          octacity_server_store::IssueOutcomeReference::from_accepted(&candidate.presentation, &record.value);
        let (conclusion, content) = IssueOutcomeContent::project(candidate.presentation, &record.value)?;
        let links = self
          .links
          .resolve(
            context,
            self.policy.as_ref(),
            crate::issue_outcome_links::IssueOutcomeLinkScope {
              project: query.project_id,
              issue: query.issue_id,
              work_id: candidate.work_id,
              inputs: &record.inputs,
            },
            &references,
          )
          .await?;
        slots.insert(candidate.slot.clone());
        cards.push(IssueOutcomeCard {
          summary: octacity_server_store::IssueOutcomeCard {
            slot: candidate.slot,
            conclusion,
            content,
            work_id: candidate.work_id,
            run_id,
            node_attempt_id: candidate.position.attempt,
          },
          links,
        });
        if cards.len() == limit {
          break;
        }
      }
      if count < limit {
        break;
      }
    }
    Ok(cards)
  }
}
