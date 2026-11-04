//! Pure Issue lineage composition over the common retained action descriptor.
use crate::ApplicationError;
use octacity_server_domain::Timestamp;
pub use octacity_server_store::{FactoryNodeActionDescription, FactoryNodeActionKind};
use octacity_server_store::{FactoryRunSnapshot, IssueWorkAssociation};
use serde::Serialize;

/// Original optional collaboration lineage; it is not an alternate execution target.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IssueFactoryActionDescription {
  /// Exact retained Issue/Work association, including nullable legacy requirements provenance.
  pub association: IssueWorkAssociation,
  /// Existing control target and observed Run precondition.
  pub action: FactoryNodeActionDescription,
}

/// Binds pure descriptions only to the supplied original association and snapshot.
/// Callers own visibility and bounded queue discovery; manual Issues without Work need no description.
pub fn describe_issue_factory_actions(
  association: &IssueWorkAssociation,
  snapshot: &FactoryRunSnapshot,
  at: Timestamp,
) -> Result<Vec<IssueFactoryActionDescription>, ApplicationError> {
  if association.project_id != snapshot.work.subject().project_id()
    || association.work_id != snapshot.work.id()
    || association.run_id != snapshot.run.id()
  {
    return Err(ApplicationError::invalid());
  }
  let mut actions = snapshot
    .flow
    .attempts
    .iter()
    .filter_map(|attempt| {
      FactoryNodeActionDescription::from_snapshot(snapshot, attempt.id(), at).map(|action| {
        IssueFactoryActionDescription {
          association: association.clone(),
          action,
        }
      })
    })
    .collect::<Vec<_>>();
  actions.sort_by_key(|row| (row.action.flow_run_id, row.action.cycle_id, row.action.node_attempt_id));
  Ok(actions)
}
