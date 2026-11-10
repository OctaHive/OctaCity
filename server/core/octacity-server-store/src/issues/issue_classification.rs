//! Trusted local Issue planning effects. No management wire input or provider intent.
use crate::*;
use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_factory::{FactoryDigest, FactoryKey, MAX_WORK_PRIORITY};
use std::collections::{BTreeMap, BTreeSet};

/// Explicit new-style action selection. Custom fields reuse Project semantic bindings;
/// priority and component remain independently selected built-in attributes.
#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueClassificationFieldSelection {
  /// Exact reviewed Project definition publication.
  pub definition_version: u64,
  /// Whether the action can apply built-in priority.
  #[serde(default)]
  pub priority: bool,
  /// Whether the action can apply built-in component.
  #[serde(default)]
  pub component: bool,
  /// Whether the accepted assessment's kind can apply the built-in Issue type.
  #[serde(default)]
  pub kind: bool,
  /// Whether the action can apply built-in size without label effects.
  #[serde(default)]
  pub size: bool,
  /// Selected classification properties; arbitrary custom properties are not implicit targets.
  #[serde(default)]
  pub properties: IssueClassificationBindings,
}
impl IssueClassificationFieldSelection {
  /// Validates exact current definitions without inventing a per-Flow vocabulary.
  pub fn validate(&self, catalog: &PublishedIssueProperties) -> Result<(), StoreError> {
    if self.definition_version == 0
      || self.definition_version != catalog.version
      || (!self.priority && !self.component && !self.kind && !self.size && self.properties.fields().next().is_none())
      || self
        .properties
        .fields()
        .any(|(slot, _)| matches!(slot, "component" | "kind" | "size"))
      || self.properties.fields().any(|(slot, id)| {
        !catalog
          .definition
          .classification
          .fields()
          .any(|pair| pair == (slot, id))
          || !catalog.definition.properties.iter().any(|p| p.id == id && !p.retired)
      })
    {
      return Err(StoreError::Conflict {
        entity: EntityKind::Issue,
      });
    }
    Ok(())
  }
}

/// Validated selected values carried only by the trusted Issue command, not another value store.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct IssueClassificationUpdate {
  /// Original explicit field/definition selection.
  pub selection: IssueClassificationFieldSelection,
  /// Assessed Project priority choice; never an implicit Work scheduling score.
  pub priority: Option<String>,
  /// Assessed built-in component; absence preserves the current field.
  pub component: Option<String>,
  /// Assessed built-in Issue type from the accepted kind; absence preserves the field.
  pub issue_type: Option<String>,
  /// Assessed built-in size; absence preserves the field.
  pub size: Option<IssueSize>,
  /// Assessed choice values by selected stable property ID; absence preserves that field.
  pub properties: BTreeMap<String, String>,
  /// Explicit assessment dependencies; omitted on earlier publications and receipts.
  #[serde(skip_serializing_if = "IssueClassificationAssessmentInputs::is_empty")]
  pub inputs: IssueClassificationAssessmentInputs,
}
impl IssueClassificationUpdate {
  /// Canonical locations actually supplied by the accepted assessment.
  pub fn field_references(&self) -> Vec<IssueClassificationFieldReference> {
    let version = self.selection.definition_version;
    let mut fields = Vec::new();
    if self.priority.is_some() {
      fields.push(IssueClassificationFieldReference::Priority {
        definition_version: Some(version),
      });
    }
    if self.component.is_some() {
      fields.push(IssueClassificationFieldReference::Component {
        definition_version: version,
      });
    }
    if self.issue_type.is_some() {
      fields.push(IssueClassificationFieldReference::Type {
        definition_version: version,
      });
    }
    if self.size.is_some() {
      fields.push(IssueClassificationFieldReference::Size);
    }
    fields.extend(
      self
        .properties
        .keys()
        .map(|id| IssueClassificationFieldReference::Property {
          id: id.clone(),
          definition_version: version,
        }),
    );
    fields
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
      update.selection.validate(catalog)?;
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
        || (update.priority.is_some()
          && (!update.selection.priority
            || self.content.priority != baseline.content.priority
            || policy.source_owned.contains(&IssueField::Priority)))
        || (update.component.is_some()
          && (!update.selection.component
            || self.planning.component != baseline.planning.component
            || policy.source_owned.contains(&IssueField::Component)))
        || (update.issue_type.is_some()
          && (!update.selection.kind
            || self.planning.issue_type != baseline.planning.issue_type
            || policy.source_owned.contains(&IssueField::Type)))
        || (update.size.is_some()
          && (!update.selection.size
            || self.planning.size != baseline.planning.size
            || policy.source_owned.contains(&IssueField::Size)))
        || (!update.properties.is_empty() && policy.source_owned.contains(&IssueField::Properties))
      {
        return Err(conflict());
      }
      if let Some(priority) = &update.priority {
        if !catalog
          .definition
          .priorities
          .iter()
          .any(|c| c.id == *priority && !c.retired)
        {
          return Err(conflict());
        }
        next.content.priority = Some(IssuePriority::Choice(priority.clone()));
      }
      if let Some(component) = &update.component {
        if !catalog
          .definition
          .components
          .iter()
          .any(|c| c.id == *component && !c.retired)
        {
          return Err(conflict());
        }
        next.planning.component = Some(component.clone());
      }
      if let Some(issue_type) = &update.issue_type {
        if !catalog
          .definition
          .types
          .iter()
          .any(|c| c.id == *issue_type && !c.retired)
        {
          return Err(conflict());
        }
        next.planning.issue_type = Some(issue_type.clone());
      }
      if let Some(size) = update.size {
        next.planning.size = Some(size);
      }
      for (id, value) in &update.properties {
        let slot = update
          .selection
          .properties
          .fields()
          .find(|(_, selected)| *selected == id)
          .map(|(slot, _)| slot)
          .ok_or_else(conflict)?;
        let valid = catalog.definition.properties.iter().any(|p| {
          p.id == *id
            && !p.retired
            && matches!(&p.kind, IssuePropertyKind::Choice { choices }
            if choices.iter().any(|c|c.id == *value && !c.retired))
        });
        if !valid
          || self.planning.values.get(id) != baseline.planning.values.get(id)
          || (slot == "reproducibility"
            && crate::IssueReproductionClaim::from_choice(value)
              .is_none_or(crate::IssueReproductionClaim::requires_verified_evidence))
        {
          return Err(conflict());
        }
        next
          .planning
          .values
          .insert(id.clone(), IssuePropertyValue::Choice(value.clone()));
      }
      let fields = update.field_references();
      if !fields.is_empty() {
        if update.component.is_some()
          || update.issue_type.is_some()
          || update.size.is_some()
          || !update.properties.is_empty()
        {
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
