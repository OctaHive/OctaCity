//! One Project-derived contract for an agent's form and its permitted Issue write set.
use crate::*;
use octacity_server_factory::{FactoryDigest, FactoryKey, FlowFieldSchema, FlowValueSchema};
use std::collections::BTreeSet;

pub use octacity_server_factory::{IssueFieldForm, IssueFieldTarget, IssueFormField};

/// Project catalog operations shared by publication and the guarded Issue writer.
pub trait IssueFieldFormContract {
  /// Content identity of the exact reviewed selection.
  fn digest(&self) -> Result<FactoryDigest, StoreError>;
  /// Validate the accepted closed form and construct a typed write command.
  fn complete(
    &self,
    catalog: &PublishedIssueProperties,
    result: &serde_json::Value,
    inputs: IssueClassificationAssessmentInputs,
  ) -> Result<IssueClassificationUpdate, StoreError>;
  /// Derive the result grammar from the immutable Project catalog.
  fn form_schema(&self, catalog: &PublishedIssueProperties) -> Result<FlowValueSchema, StoreError>;
}
impl IssueFieldFormContract for IssueFieldForm {
  /// Content identity of the reviewed selection, including Project, definition pin and requiredness.
  /// Exact definitions remain in the immutable Project catalog rather than a copied result schema.
  fn digest(&self) -> Result<FactoryDigest, StoreError> {
    let bytes = serde_json::to_vec(self).map_err(|_| super::issue_planning::planning_conflict())?;
    Ok(FactoryDigest::content_sha256(&bytes))
  }

  /// Decodes only this form's closed scalar result. Issue identity and provenance are supplied by the caller.
  fn complete(
    &self,
    catalog: &PublishedIssueProperties,
    result: &serde_json::Value,
    inputs: IssueClassificationAssessmentInputs,
  ) -> Result<IssueClassificationUpdate, StoreError> {
    let conflict = super::issue_planning::planning_conflict;
    let object = result.as_object().ok_or_else(conflict)?;
    let values = object
      .iter()
      .map(|(key, value)| Ok((FactoryKey::new(key.clone()).map_err(|_| conflict())?, value.clone())))
      .collect::<Result<_, StoreError>>()?;
    let update = IssueClassificationUpdate {
      selection: self.clone(),
      invocation: None,
      values,
      inputs,
    };
    update.validated_values(catalog)?;
    Ok(update)
  }

  /// Derives a closed form from the same identities that constrain the writer.
  fn form_schema(&self, catalog: &PublishedIssueProperties) -> Result<FlowValueSchema, StoreError> {
    let conflict = super::issue_planning::planning_conflict;
    if self.project_id != catalog.project_id
      || self.definition_version != catalog.version
      || self.fields.is_empty()
      || self.fields.len() > MAX_ISSUE_PROPERTIES + 4
    {
      return Err(conflict());
    }
    let mut seen = BTreeSet::new();
    let mut fields = std::collections::BTreeMap::new();
    for selected in &self.fields {
      if !seen.insert(&selected.field) {
        return Err(conflict());
      }
      let kind = selected.field.kind(catalog).ok_or_else(conflict)?;
      let shape = kind.value_schema().ok_or_else(conflict)?;
      fields.insert(
        selected.field.key()?,
        if selected.required {
          FlowFieldSchema::required(shape)
        } else {
          FlowFieldSchema::optional(shape)
        },
      );
    }
    Ok(FlowValueSchema::Object { fields })
  }
}
/// Stable scalar destinations interpreted against the Project's catalog.
pub trait IssueFieldTargetContract {
  /// Retained typed field identity and its catalog pin.
  fn reference(&self, definition_version: u64) -> IssueClassificationFieldReference;
  /// Generated transport key, independent of display names.
  fn key(&self) -> Result<FactoryKey, StoreError>;
  /// Project value type, unavailable for missing or retired fields.
  fn kind(&self, catalog: &PublishedIssueProperties) -> Option<IssuePropertyKind>;
}
impl IssueFieldTargetContract for IssueFieldTarget {
  fn reference(&self, definition_version: u64) -> IssueClassificationFieldReference {
    match self {
      Self::Priority => IssueClassificationFieldReference::Priority {
        definition_version: Some(definition_version),
      },
      Self::Component => IssueClassificationFieldReference::Component { definition_version },
      Self::Type => IssueClassificationFieldReference::Type { definition_version },
      Self::Size => IssueClassificationFieldReference::Size {
        definition_version: Some(definition_version),
      },
      Self::Property { id } => IssueClassificationFieldReference::Property {
        id: id.clone(),
        definition_version,
      },
    }
  }
  /// Stable generated transport key; property names cannot collide with built-in fields.
  fn key(&self) -> Result<FactoryKey, StoreError> {
    let key = match self {
      Self::Priority => "priority".to_owned(),
      Self::Component => "component".to_owned(),
      Self::Type => "type".to_owned(),
      Self::Size => "size".to_owned(),
      Self::Property { id } if super::issue_planning::valid_property_id(id) => format!("property.{id}"),
      Self::Property { .. } => return Err(super::issue_planning::planning_conflict()),
    };
    FactoryKey::new(key).map_err(|_| super::issue_planning::planning_conflict())
  }
  fn kind(&self, catalog: &PublishedIssueProperties) -> Option<IssuePropertyKind> {
    let choices = match self {
      Self::Priority => &catalog.definition.priorities,
      Self::Component => &catalog.definition.components,
      Self::Type => &catalog.definition.types,
      Self::Size => &catalog.definition.sizes,
      Self::Property { id } => {
        return catalog
          .definition
          .properties
          .iter()
          .find(|p| p.id == *id && !p.retired)
          .map(|p| p.kind.clone());
      }
    };
    Some(IssuePropertyKind::Choice {
      choices: choices.clone(),
    })
  }
}
impl IssuePropertyKind {
  fn form_value(&self, value: &serde_json::Value) -> Option<IssuePropertyValue> {
    Some(match self {
      Self::Text { .. } if !value.as_str()?.contains('\0') => IssuePropertyValue::Text(value.as_str()?.to_owned()),
      Self::Choice { .. } => IssuePropertyValue::Choice(value.as_str()?.to_owned()),
      Self::Boolean => IssuePropertyValue::Boolean(value.as_bool()?),
      Self::Number { .. } => IssuePropertyValue::Number(value.as_i64()?),
      Self::Date if super::issue_planning::valid_date(value.as_str()?) => {
        IssuePropertyValue::Date(value.as_str()?.to_owned())
      }
      _ => return None,
    })
  }
}

impl IssueClassificationUpdate {
  /// Rechecks the same generated contract at the mutation boundary, including typed in-process callers.
  pub(crate) fn validated_values(
    &self,
    catalog: &PublishedIssueProperties,
  ) -> Result<std::collections::BTreeMap<FactoryKey, IssuePropertyValue>, StoreError> {
    let conflict = super::issue_planning::planning_conflict;
    let raw = serde_json::Value::Object(
      self
        .values
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect(),
    );
    if !self.selection.form_schema(catalog)?.accepts_value(&raw) {
      return Err(conflict());
    }
    self
      .selection
      .fields
      .iter()
      .filter_map(|selected| {
        let key = selected.field.key();
        match key {
          Ok(key) => self.values.get(&key).map(|value| {
            selected
              .field
              .kind(catalog)
              .and_then(|kind| kind.form_value(value))
              .map(|value| (key, value))
              .ok_or_else(conflict)
          }),
          Err(error) => Some(Err(error)),
        }
      })
      .collect()
  }
}
