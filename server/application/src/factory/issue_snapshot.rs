//! Shared Issue input contract resolution for authoring and execution.
//! Resolves one authorized catalog; never reads Issue bodies or producing Runs.
use crate::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
use octacity_server_store::*;

/// One independently authorized current catalog for read-only authoring discovery.
pub(crate) async fn current_catalog(
  properties: Option<&dyn IssuePropertyStore>,
  policy: &dyn ManagementAuthorizationPolicy,
  context: &ManagementRequestContext,
  project: ProjectId,
) -> Result<Option<PublishedIssueProperties>, ApplicationError> {
  let Some(properties) = properties else { return Ok(None) };
  let query = GetIssuePropertiesQuery {
    project_id: project,
    version: None,
  };
  let grant = policy
    .authorize(
      context,
      ManagementAction::View,
      &query.management_resource().map_err(|_| ApplicationError::invalid())?,
    )
    .await;
  let Ok(grant) = grant else { return Ok(None) };
  if crate::factory::profiles::factory_role_profiles::require_project_visibility(&grant, project).is_err() {
    return Ok(None);
  }
  properties
    .issue_properties(project, None)
    .await
    .map(Some)
    .map_err(Into::into)
}

/// Returns the exact validated catalog when the Flow declares an Issue input.
pub(crate) async fn resolve(
  flow: &FactoryFlowConfiguration,
  project: ProjectId,
  properties: Option<&dyn IssuePropertyStore>,
  policy: &dyn ManagementAuthorizationPolicy,
  context: &ManagementRequestContext,
) -> Result<Option<PublishedIssueProperties>, ApplicationError> {
  let Some(input) = &flow.issue_snapshot else {
    return Ok(None);
  };
  let unavailable = || FactoryValidationDiagnostics::unavailable("/issue_snapshot/definition_version");
  let query = GetIssuePropertiesQuery {
    project_id: project,
    version: Some(input.definition_version),
  };
  let grant = policy
    .authorize(
      context,
      ManagementAction::View,
      &query.management_resource().map_err(|_| unavailable())?,
    )
    .await
    .map_err(|_| unavailable())?;
  crate::factory::profiles::factory_role_profiles::require_project_visibility(&grant, project)
    .map_err(|_| unavailable())?;
  let catalog = properties
    .ok_or_else(unavailable)?
    .issue_properties(project, Some(input.definition_version))
    .await
    .map_err(|error| match error {
      StoreError::NotFound { .. } => unavailable(),
      other => other.into(),
    })?;
  if catalog.project_id != project || catalog.version != input.definition_version {
    return Err(unavailable());
  }
  let target = flow
    .data_schemas
    .iter()
    .find(|schema| schema.reference() == &input.schema)
    .ok_or_else(|| FactoryValidationDiagnostics::invalid("/issue_snapshot/schema"))?;
  let FlowValueSchema::Object { fields: targets } = target.shape() else {
    return Err(FactoryValidationDiagnostics::invalid("/issue_snapshot/schema"));
  };
  let mut projected = std::collections::BTreeMap::new();
  for (alias, selection) in &input.fields {
    let field = format!("/issue_snapshot/fields/{alias}");
    let producer = field_shape(selection, &catalog).ok_or_else(|| FactoryValidationDiagnostics::invalid(&field))?;
    projected.insert(
      alias.clone(),
      compatible(targets, alias, selection.required, producer, &field)?,
    );
  }
  if let Some(policy) = &input.admission {
    validate_admission_policy(input, policy, &catalog)?;
  }
  let mut identities = std::collections::BTreeSet::new();
  let mut total_entries = 0;
  let mut total_bytes = 0;
  for (alias, selection) in &input.materials {
    let field = format!("/issue_snapshot/materials/{alias}");
    let invalid = || FactoryValidationDiagnostics::invalid(&field);
    let count = match &selection.source {
      FlowIssueMaterialSource::Selected { max_entries } => usize::from(*max_entries),
      FlowIssueMaterialSource::Exact { references } => {
        for reference in references {
          let identity = match reference {
            FlowIssueMaterialReference::Comment { comment_id, .. } => (0, comment_id.as_uuid()),
            FlowIssueMaterialReference::IssueDocument { artifact }
            | FlowIssueMaterialReference::ProjectDocument { artifact } => (1, artifact.artifact_id().as_uuid()),
          };
          if !identities.insert(identity) {
            return Err(invalid());
          }
        }
        references.len()
      }
    };
    if count == 0
      || count > MAX_ISSUE_MATERIALS
      || selection.max_bytes == 0
      || selection.max_bytes > MAX_FLOW_DATA_BYTES
    {
      return Err(invalid());
    }
    total_entries += count;
    total_bytes += selection.max_bytes;
    if total_entries > MAX_ISSUE_MATERIALS || total_bytes > MAX_FLOW_DATA_BYTES {
      return Err(FactoryValidationDiagnostics::invalid("/issue_snapshot/materials"));
    }
    let minimum = if selection.required {
      match &selection.source {
        FlowIssueMaterialSource::Exact { .. } => count as u16,
        FlowIssueMaterialSource::Selected { .. } => 1,
      }
    } else {
      0
    };
    let value = compatible(
      targets,
      alias,
      selection.required,
      FlowValueSchema::Array {
        item: Box::new(FlowValueSchema::String {
          min_bytes: 0,
          max_bytes: selection.max_bytes,
        }),
        min_items: minimum,
        max_items: count as u16,
      },
      &field,
    )?;
    projected.insert(alias.clone(), value);
  }
  for (alias, selection) in &input.evidence {
    let field = format!("/issue_snapshot/evidence/{alias}");
    if !input.fields.contains_key(&selection.field)
      || selection.maximum == 0
      || usize::from(selection.maximum) > MAX_ISSUE_CLASSIFICATION_REFERENCES
    {
      return Err(FactoryValidationDiagnostics::invalid(&field));
    }
    let value = compatible(
      targets,
      alias,
      selection.required,
      FlowValueSchema::Array {
        item: Box::new(FlowValueSchema::String {
          min_bytes: 64,
          max_bytes: 64,
        }),
        min_items: u16::from(selection.required),
        max_items: selection.maximum,
      },
      &field,
    )?;
    projected.insert(alias.clone(), value);
  }
  for (alias, field) in targets {
    if field.is_required()
      && !input.fields.contains_key(alias)
      && !input.materials.contains_key(alias)
      && !input.evidence.contains_key(alias)
    {
      return Err(FactoryValidationDiagnostics::invalid(format!(
        "/issue_snapshot/schema/{alias}"
      )));
    }
  }
  let source = FlowValueSchema::Object { fields: projected };
  for definition in flow.closure.definitions() {
    for node in definition.nodes() {
      if let Some(binding) = node.input_binding()
        && binding.incoming_schemas().any(|schema| schema == &input.schema)
      {
        let target = node
          .input_schema()
          .and_then(|reference| flow.data_schemas.iter().find(|schema| schema.reference() == reference))
          .ok_or_else(|| {
            FactoryValidationDiagnostics::invalid_binding(definition.reference(), node.key(), "/input_binding")
          })?;
        if !binding.fits_incoming(&input.schema, &source, target) {
          return Err(FactoryValidationDiagnostics::invalid_binding(
            definition.reference(),
            node.key(),
            "/input_binding",
          ));
        }
      }
    }
  }
  Ok(Some(catalog))
}

pub(crate) fn field_shape(
  selection: &FlowIssueFieldSelection,
  catalog: &PublishedIssueProperties,
) -> Option<FlowValueSchema> {
  let text = |max_bytes| FlowValueSchema::String {
    min_bytes: 0,
    max_bytes,
  };
  let choices = |choices: &[IssuePropertyChoice]| {
    let values: Vec<_> = choices
      .iter()
      .filter(|choice| !choice.retired)
      .map(|choice| serde_json::Value::String(choice.id.clone()))
      .collect();
    if values.is_empty() {
      // An optional empty choice list can only be omitted at preparation. Preserve
      // the canonical ID wire kind here; the exact catalog still permits no value.
      (!selection.required).then_some(FlowValueSchema::String {
        min_bytes: 1,
        max_bytes: MAX_ISSUE_PROPERTY_KEY_BYTES,
      })
    } else {
      Some(FlowValueSchema::Enum { values })
    }
  };
  let id = match &selection.source {
    FlowIssueFieldSource::Title => {
      return Some(FlowValueSchema::String {
        min_bytes: 1,
        max_bytes: MAX_ISSUE_TITLE_BYTES,
      });
    }
    FlowIssueFieldSource::Description | FlowIssueFieldSource::Acceptance => {
      return Some(text(MAX_ISSUE_DOCUMENT_BYTES));
    }
    FlowIssueFieldSource::Priority => return choices(&catalog.definition.priorities),
    FlowIssueFieldSource::Component => return choices(&catalog.definition.components),
    FlowIssueFieldSource::Type => return choices(&catalog.definition.types),
    FlowIssueFieldSource::Size => {
      return Some(FlowValueSchema::Enum {
        values: [IssueSize::Small, IssueSize::Medium, IssueSize::Large]
          .map(|size| serde_json::to_value(size).expect("canonical Issue size"))
          .into(),
      });
    }
    FlowIssueFieldSource::Dependencies => {
      return Some(FlowValueSchema::Array {
        min_items: 0,
        max_items: MAX_ISSUE_CLASSIFICATION_REFERENCES as u16,
        item: Box::new(FlowValueSchema::Object {
          fields: [
            ("work_id", 36, 36),
            ("run_id", 36, 36),
            ("work_digest", 64, 64),
            ("terminal", 1, MAX_FACTORY_KEY_BYTES),
          ]
          .into_iter()
          .map(|(name, min_bytes, max_bytes)| {
            (
              FactoryKey::new(name).expect("static dependency field"),
              FlowFieldSchema::required(FlowValueSchema::String { min_bytes, max_bytes }),
            )
          })
          .collect(),
        }),
      });
    }
    FlowIssueFieldSource::Property { id } => id.as_str(),
    FlowIssueFieldSource::Classification { slot } => {
      catalog
        .definition
        .classification
        .fields()
        .find(|(name, _)| *name == slot.as_str())?
        .1
    }
  };
  let property = catalog
    .definition
    .properties
    .iter()
    .find(|property| property.id == id && !property.retired)?;
  Some(match &property.kind {
    IssuePropertyKind::Boolean => FlowValueSchema::Boolean,
    IssuePropertyKind::Text { max_bytes } => text(usize::from(*max_bytes)),
    IssuePropertyKind::Number { minimum, maximum } => FlowValueSchema::Integer {
      minimum: *minimum,
      maximum: *maximum,
    },
    IssuePropertyKind::Date => FlowValueSchema::String {
      min_bytes: 10,
      max_bytes: 10,
    },
    IssuePropertyKind::Choice { choices: values } => return choices(values),
  })
}

fn compatible(
  targets: &std::collections::BTreeMap<FactoryKey, FlowFieldSchema>,
  alias: &FactoryKey,
  required: bool,
  producer: FlowValueSchema,
  field: &str,
) -> Result<FlowFieldSchema, ApplicationError> {
  let invalid = || FactoryValidationDiagnostics::invalid(field);
  let expected = targets.get(alias).ok_or_else(invalid)?;
  if (!required && expected.is_required()) || !expected.value().accepts_all(&producer) {
    return Err(invalid());
  }
  Ok(if required {
    FlowFieldSchema::required(producer)
  } else {
    FlowFieldSchema::optional(producer)
  })
}

// Policy aliases must name required canonical classification, not arbitrary prose or
// an editable ordinary `accepted` property. Catalogs remain the choice/type owner.
fn validate_admission_policy(
  input: &FlowIssueSnapshotInput,
  policy: &FlowIssueAdmissionPolicy,
  catalog: &PublishedIssueProperties,
) -> Result<(), ApplicationError> {
  let invalid = || FactoryValidationDiagnostics::invalid("/issue_snapshot/admission");
  if !input
    .fields
    .get(&policy.priority.field)
    .is_some_and(|s| matches!(s.source, FlowIssueFieldSource::Priority))
  {
    return Err(invalid());
  }
  let mappings = std::iter::once((
    &policy.priority.field,
    policy.priority.values.keys().collect::<Vec<_>>(),
  ))
  .chain(std::iter::once((
    &policy.risk.field,
    policy.risk.values.keys().collect(),
  )))
  .chain(
    policy
      .allowed
      .iter()
      .map(|(field, allowed)| (field, allowed.iter().collect())),
  );
  for (field, values) in mappings {
    let selection = input.fields.get(field).ok_or_else(invalid)?;
    let classified = match &selection.source {
      FlowIssueFieldSource::Priority
      | FlowIssueFieldSource::Component
      | FlowIssueFieldSource::Type
      | FlowIssueFieldSource::Size
      | FlowIssueFieldSource::Classification { .. } => true,
      FlowIssueFieldSource::Property { id } => catalog.definition.classification.fields().any(|(_, bound)| bound == id),
      _ => false,
    };
    if !selection.required || !classified || values.is_empty() || values.len() > MAX_FLOW_SCHEMA_ENTRIES {
      return Err(invalid());
    }
    let shape = field_shape(selection, catalog).ok_or_else(invalid)?;
    if values
      .iter()
      .any(|v| !shape.accepts_value(&serde_json::Value::String((*v).clone())))
    {
      return Err(invalid());
    }
  }
  Ok(())
}
