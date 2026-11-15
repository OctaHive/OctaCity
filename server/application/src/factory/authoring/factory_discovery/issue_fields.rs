//! Safe accepted-analysis/save correlation, projected from the existing local-effect journal.
use crate::*;
use octacity_server_factory::*;
use octacity_server_store::{FactoryOutboxFailure, FactoryOutboxState, FactoryRunSnapshot};

/// A guarded save is independent of analysis acceptance and Flow completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryIssueFieldApplicationStatus {
  /// Accepted analysis is awaiting a confirmed local save receipt.
  Pending,
  /// The existing trusted owner confirmed the atomic save.
  Applied,
  /// A conflicting Issue edit prevented the save.
  Conflict,
  /// Current write authority denied the save.
  Denied,
  /// The local operation is unsupported or its bounded retries ended.
  Failed,
}
/// Scope and safe receipt identities only; no result values, Issue body or worker ownership.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryIssueFieldApplication {
  /// The exact admitted field selection, not an inferred write scope.
  pub form: IssueFieldForm,
  /// Original accepted result's idempotent local operation.
  pub operation_id: FactoryDigest,
  /// Latest observed guarded application state.
  pub status: FactoryIssueFieldApplicationStatus,
  /// Exact latest immutable outbox receipt, when already recorded.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub delivery_record: Option<FactoryDigest>,
}
pub(super) fn project(
  record: &FlowNodeRecord,
  write: &FlowIssueFieldWrite,
  snapshot: &FactoryRunSnapshot,
) -> Result<FactoryIssueFieldApplication, ApplicationError> {
  let expected =
    crate::factory::reporting::factory_issue_classification_action::automatic::accepted_effect(record, write)?;
  let digest = record.digest().map_err(|_| ApplicationError::unavailable())?;
  let latest = snapshot
    .outbox
    .iter()
    .filter(|row| {
      row.subject.run_id() == Some(snapshot.run.id())
        && row.operation_id == expected.operation_id
        && row.kind == expected.kind
        && row.input_digest == digest
    })
    .max_by_key(|row| row.order_key());
  use FactoryIssueFieldApplicationStatus as Status;
  let status = match latest.map(|row| (row.state, row.failure)) {
    Some((FactoryOutboxState::Delivered, _)) => Status::Applied,
    Some((FactoryOutboxState::Failed, Some(FactoryOutboxFailure::Conflict))) => Status::Conflict,
    Some((FactoryOutboxState::Failed, Some(FactoryOutboxFailure::Forbidden))) => Status::Denied,
    Some((FactoryOutboxState::Failed, _)) => Status::Failed,
    _ => Status::Pending,
  };
  Ok(FactoryIssueFieldApplication {
    form: write.form.clone(),
    operation_id: expected.operation_id,
    status,
    delivery_record: latest.map(|row| row.id),
  })
}
