//! Trusted local Issue planning effects. No management wire input or provider intent.
use crate::*;
use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_factory::{FactoryDigest, FactoryKey, MAX_WORK_PRIORITY};
use std::collections::BTreeSet;

/// A reviewed effect selected from an exact accepted Factory outcome.
#[derive(Clone, Debug)]
pub struct ApplyIssueClassification {
  /// Exact Issue association independently checked by the writer.
  pub association: IssueWorkAssociation,
  /// Stable identity of the configured classification policy.
  pub policy: FactoryKey,
  /// Exact accepted source record.
  pub record: FactoryDigest,
  /// Trusted acceptance time of the selected record, used within the Run order.
  pub accepted_at: Timestamp,
  /// Stable accepted action identity.
  pub operation_id: FactoryDigest,
  /// Validated Issue priority, when the outcome includes an assessment.
  pub priority: Option<u8>,
  /// Stable active Project label selected by the reviewed mapping.
  pub size_label: Option<String>,
  /// Label identities covered by the reviewed policy.
  pub managed_labels: BTreeSet<String>,
  /// Trusted application time, independent of the original operation identity.
  pub applied_at: Timestamp,
  /// Fenced runtime authority for a Factory-driven mutation.
  pub authority: Option<IssueClassificationAuthority>,
}

/// Bounds an in-process write to the current Run claim and its original deadline.
#[derive(Clone, Debug)]
pub struct IssueClassificationAuthority {
  /// Claim accepted by the action adapter.
  pub claim: FactoryDigest,
  /// Monotonic deadline derived from the attempt, claim and remaining budget.
  pub deadline: std::time::Instant,
}

/// The Issue owner applies a local classification and its replay receipt atomically.
#[async_trait::async_trait]
pub trait IssueClassificationStore: Send + Sync {
  /// Applies the accepted effect under the management principal's replay scope.
  async fn apply_issue_classification(
    &self,
    request: ManagementMutation<ApplyIssueClassification>,
  ) -> Result<IssueMutationOutcome, StoreError>;
}

impl IssueRevision {
  /// Applies the selected planning fields to one Issue revision under the current field policy.
  pub fn classified(
    &self,
    baseline: &Self,
    request: &ApplyIssueClassification,
    owned_label: Option<&str>,
    catalog: &PublishedIssueProperties,
    policy: &IssueFieldPolicy,
  ) -> Result<Self, StoreError> {
    let conflict = || StoreError::Conflict {
      entity: EntityKind::Issue,
    };
    let association = &request.association;
    if self.project_id != association.project_id
      || self.id != association.issue_id
      || baseline.id != self.id
      || baseline.project_id != self.project_id
      || self.applicability_version != baseline.applicability_version
      || !self.content_available
      || request.applied_at < self.updated_at
      || request.priority.is_some_and(|priority| {
        self.content.priority != baseline.content.priority
          || priority > MAX_WORK_PRIORITY
          || policy.source_owned.contains(&IssueField::Priority)
      })
    {
      return Err(conflict());
    }
    let mut next = self.clone();
    if let Some(priority) = request.priority {
      next.content.priority = Some(priority);
    }
    if let Some(label) = &request.size_label {
      if !request.managed_labels.contains(label)
        || !catalog
          .definition
          .labels
          .iter()
          .any(|choice| choice.id == *label && !choice.retired)
        || policy.source_owned.contains(&IssueField::Labels)
        || !self
          .planning
          .labels
          .intersection(&request.managed_labels)
          .eq(baseline.planning.labels.intersection(&request.managed_labels))
      {
        return Err(conflict());
      }
      if let Some(previous) = owned_label.filter(|previous| *previous != label) {
        next.planning.labels.remove(previous);
      }
      next.planning.labels.insert(label.clone());
      if next.planning.labels != self.planning.labels {
        next.planning.definition_version = catalog.version;
        next
          .planning
          .validate(catalog, Some(&self.planning), &BTreeSet::new())?;
      }
    }
    if next != *self {
      next.version = self.version.next().map_err(|_| conflict())?;
      next.updated_at = request.applied_at;
    }
    Ok(next)
  }
}
