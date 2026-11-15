//! Trusted local Issue planning effects. No management wire input or provider intent.
use crate::*;
use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_factory::{FactoryDigest, FactoryKey, MAX_WORK_PRIORITY};
use std::collections::{BTreeMap, BTreeSet};

/// Values from the selected Project-derived form; not a second Issue value store.
#[derive(Clone, Eq, PartialEq, serde::Serialize)]
pub struct IssueClassificationUpdate {
  /// Exact form whose identities authorize the selected writes.
  pub selection: IssueFieldForm,
  /// Only supplied values, keyed by the form's generated stable identities.
  pub values: BTreeMap<FactoryKey, serde_json::Value>,
  /// Explicit immutable dependencies of this assessment.
  pub inputs: IssueClassificationAssessmentInputs,
}
impl std::fmt::Debug for IssueClassificationUpdate {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("IssueClassificationUpdate")
      .finish_non_exhaustive()
  }
}
impl IssueClassificationUpdate {
  /// Canonical locations actually supplied by the accepted form.
  pub fn field_references(&self) -> Vec<IssueClassificationFieldReference> {
    self
      .selection
      .fields
      .iter()
      .filter(|selected| selected.field.key().is_ok_and(|key| self.values.contains_key(&key)))
      .map(|selected| selected.field.reference(self.selection.definition_version))
      .collect()
  }
}

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
  /// New explicit built-in/custom classification selection; absent for legacy priority/label actions.
  pub fields: Option<IssueClassificationUpdate>,
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
    actor: &AuditActor,
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
      next.content.priority = Some(priority.into());
      if next.content.priority != self.content.priority {
        // Legacy receipts retain their own provenance; a changed value cannot
        // keep an earlier typed assessment's field attribution/catalog pin.
        next
          .classification
          .fields
          .retain(|field| field.field.identity() != IssueClassificationFieldIdentity::Priority);
      }
    }
    if let Some(update) = &request.fields {
      let values = update.validated_values(catalog)?;
      if update.inputs.validate().is_err()
        || update
          .inputs
          .subject
          .as_ref()
          .is_some_and(|subject| subject.exact().is_ok_and(|s| s.project_id() != self.project_id))
        || update.inputs.materials.iter().any(|reference| {
          !baseline
            .materials
            .entries()
            .iter()
            .any(|entry| &entry.reference == reference)
            || !self
              .materials
              .entries()
              .iter()
              .any(|entry| &entry.reference == reference)
        })
        || catalog.project_id != self.project_id
        || request.priority.is_some()
      {
        return Err(conflict());
      }
      for selected in &update.selection.fields {
        let Some(value) = values.get(&selected.field.key()?) else {
          continue;
        };
        let (unchanged, owned) = match &selected.field {
          IssueFieldTarget::Priority => (self.content.priority == baseline.content.priority, IssueField::Priority),
          IssueFieldTarget::Component => (
            self.planning.component == baseline.planning.component,
            IssueField::Component,
          ),
          IssueFieldTarget::Type => (
            self.planning.issue_type == baseline.planning.issue_type,
            IssueField::Type,
          ),
          IssueFieldTarget::Size => (self.planning.size == baseline.planning.size, IssueField::Size),
          IssueFieldTarget::Property { id } => (
            self.planning.values.get(id) == baseline.planning.values.get(id),
            IssueField::Properties,
          ),
        };
        if !unchanged || policy.source_owned.contains(&owned) {
          return Err(conflict());
        }
        match (&selected.field, value) {
          (IssueFieldTarget::Priority, IssuePropertyValue::Choice(value)) => {
            next.content.priority = Some(IssuePriority::Choice(value.clone()))
          }
          (IssueFieldTarget::Component, IssuePropertyValue::Choice(value)) => {
            next.planning.component = Some(value.clone())
          }
          (IssueFieldTarget::Type, IssuePropertyValue::Choice(value)) => next.planning.issue_type = Some(value.clone()),
          (IssueFieldTarget::Size, IssuePropertyValue::Choice(value)) => next.planning.size = Some(value.clone()),
          (IssueFieldTarget::Property { id }, value) => {
            next.planning.values.insert(id.clone(), value.clone());
          }
          _ => return Err(conflict()),
        }
      }
      let fields = update.field_references();
      if !fields.is_empty() {
        if values.keys().any(|key| key.as_str() != "priority") {
          next.planning.definition_version = catalog.version;
          next
            .planning
            .validate(catalog, Some(&self.planning), &BTreeSet::new())?;
        }
        // Replace only supplied field origins; unrelated attribution/evidence keeps its original age.
        next
          .classification
          .fields
          .retain(|prior| !fields.iter().any(|field| field.identity() == prior.field.identity()));
        let origin = IssueClassificationOrigin::Pipeline {
          actor: actor.clone(),
          work_id: association.work_id,
          run_id: association.run_id,
          record: request.record,
          form: update.selection.digest()?,
          policy: request.policy.clone(),
          operation_id: request.operation_id,
        };
        next
          .classification
          .fields
          .extend(fields.into_iter().map(|field| IssueClassificationAttribution {
            field,
            origin: origin.clone(),
            assessed_version: baseline.version,
            requirements_version: baseline.applicability_version,
            assessed_at: request.accepted_at,
            evidence: vec![],
            inputs: update.inputs.clone(),
          }));
        next.classification.validate().map_err(|_| conflict())?;
      }
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
