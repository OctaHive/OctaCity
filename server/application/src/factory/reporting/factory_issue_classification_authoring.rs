//! Review the installed Project mapping against the current Issue planning catalog.
use crate::factory::reporting::factory_issue_classification_action::{RECEIPT_EXAMPLE, authoring_receipt};
use crate::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::{
  FactoryConfigurationChoices, FactoryFlowConfiguration, FlowDataSchema, FlowFieldSchema, FlowOutcomeKind,
  FlowValueMapping, FlowValueSchema,
};
use octacity_server_store::{IssuePropertyStore, PublishedIssueProperties};
use std::collections::BTreeSet;

/// One authorized catalog read feeds both preview and selected-field publication checks.
pub(crate) struct ClassificationAuthoring {
  pub(crate) choices: FactoryIssueClassificationChoices,
  catalog: PublishedIssueProperties,
}

pub(crate) async fn current_choices(
  choices: &FactoryConfigurationChoices,
  properties: Option<&dyn IssuePropertyStore>,
  policy: &dyn ManagementAuthorizationPolicy,
  context: &ManagementRequestContext,
  project: ProjectId,
) -> Result<Option<ClassificationAuthoring>, ApplicationError> {
  let (Some((plugin, mapping)), Some(properties)) = (choices.issue_classification(), properties) else {
    return Ok(None);
  };
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
  let catalog = properties.issue_properties(project, None).await?;
  let size_labels = mapping
    .size_labels
    .iter()
    .map(|(size, label)| {
      let choice = catalog
        .definition
        .labels
        .iter()
        .find(|choice| choice.id == label.as_str() && !choice.retired)?;
      Some(FactoryIssueClassificationSizeLabel {
        size: size.clone(),
        label: label.clone(),
        name: choice.name.clone(),
      })
    })
    .collect::<Option<Vec<_>>>()
    .unwrap_or_default();
  let (receipt_schema, receipt_output_bytes) = authoring_receipt()?;
  Ok(Some(ClassificationAuthoring {
    choices: FactoryIssueClassificationChoices {
      plugin: plugin.clone(),
      policy: mapping.policy.clone(),
      priority_maximum: octacity_server_factory::MAX_WORK_PRIORITY,
      size_labels,
      receipt_schema,
      receipt_output_bytes,
      field_contract: super::factory_issue_classification_contract::current(&catalog),
    },
    catalog,
  }))
}

pub(crate) async fn validate_publication(
  flow: &FactoryFlowConfiguration,
  choices: &FactoryConfigurationChoices,
  properties: Option<&dyn IssuePropertyStore>,
  policy: &dyn ManagementAuthorizationPolicy,
  context: &ManagementRequestContext,
  project: ProjectId,
) -> Result<(), ApplicationError> {
  let mut actions = Vec::new();
  for definition in flow.closure.definitions() {
    for node in definition.nodes() {
      if let Some(action) = node
        .action()
        .filter(|action| action.action().as_str() == "apply_issue_classification")
      {
        actions.push((definition, node, action));
      }
    }
  }
  if actions.is_empty() {
    return Ok(());
  }
  let current = current_choices(choices, properties, policy, context, project).await?;
  for (definition, node, action) in actions {
    let unavailable = || {
      FactoryValidationDiagnostics::unavailable_binding(
        definition.reference(),
        Some(node.key()),
        "/action/parameters/size_labels",
      )
    };
    let selection =
      crate::factory::reporting::factory_issue_classification_action::Selection::from_binding(action, node.outcomes())
        .map_err(|_| {
          FactoryValidationDiagnostics::invalid_binding(definition.reference(), node.key(), "/action/parameters")
        })?;
    let resolved = current.as_ref().ok_or_else(unavailable)?;
    let current = &resolved.choices;
    if action.plugin() != &current.plugin
      || selection.policy != current.policy
      || (!(selection.fields.is_some() && selection.size_labels.is_empty())
        && (selection.size_labels.len() != current.size_labels.len()
          || current
            .size_labels
            .iter()
            .any(|choice| selection.size_labels.get(&choice.size) != Some(&choice.label))))
    {
      return Err(unavailable());
    }
    let invalid = |field| FactoryValidationDiagnostics::invalid_binding(definition.reference(), node.key(), field);
    let producer = definition
      .node(&selection.source_node)
      .filter(|producer| producer.key() != node.key())
      .and_then(|producer| producer.outcome(&selection.source_outcome))
      .filter(|outcome| outcome.kind() == FlowOutcomeKind::Success)
      .ok_or_else(|| invalid("/action/parameters/source_outcome"))?;
    let schema = |reference| flow.data_schemas.iter().find(|schema| schema.reference() == reference);
    let source = schema(producer.schema()).ok_or_else(|| invalid("/input_binding"))?;
    let target = node
      .input_schema()
      .and_then(schema)
      .ok_or_else(|| invalid("/input_schema"))?;
    let supported = if let Some(selection) = &selection.fields {
      let shape = super::factory_issue_classification_contract::selected(&resolved.catalog, selection)
        .ok_or_else(|| invalid("/action/parameters/fields"))?;
      let FlowValueSchema::Object { fields: mut projected } = target.shape().clone() else {
        return Err(invalid("/input_schema"));
      };
      // Dependencies retain the producer's bounded schema and exact record projection.
      // Their values are independently validated by the Issue owner, not Project choices.
      let metadata_key = "assessment_inputs".parse().expect("static assessment input key");
      let dependencies = projected.remove(&metadata_key);
      if dependencies.is_none()
        && matches!(source.shape(), FlowValueSchema::Object { fields } if fields.contains_key(&metadata_key))
      {
        return Err(invalid("/input_schema/assessment_inputs"));
      }
      if dependencies
        .as_ref()
        .is_some_and(|field| !matches!(field.value(), FlowValueSchema::Object { .. }))
      {
        return Err(invalid("/input_schema/assessment_inputs"));
      }
      if projected.is_empty() || !shape.accepts_all(&FlowValueSchema::Object { fields: projected }) {
        return Err(invalid("/input_schema"));
      }
      let FlowValueSchema::Object { fields } = shape else {
        unreachable!()
      };
      let mut supported = fields.into_keys().collect::<BTreeSet<_>>();
      if dependencies.is_some() {
        supported.insert(metadata_key);
      }
      supported
    } else {
      ["priority", "size"]
        .into_iter()
        .map(|name| name.parse().expect("static classification field"))
        .collect()
    };
    let (alias, mapping) = node
      .input_binding()
      .and_then(|input| input.record_payload_mapping(&selection.source_node, &selection.source_outcome))
      .ok_or_else(|| invalid("/input_binding"))?;
    let alias_path = format!("/{alias}");
    if !valid_projection(mapping, &alias_path, source, target, &supported, &mut Vec::new()) {
      return Err(invalid("/input_binding/mapping"));
    }
    if !node.permissions().plugins().any(|plugin| plugin == action.plugin()) {
      return Err(invalid("/permissions"));
    }
    let receipt = selection
      .receipt(node, &flow.data_schemas, RECEIPT_EXAMPLE)
      .map_err(|_| invalid("/outcomes"))?;
    let bytes = serde_json::to_vec(&receipt).map_err(|_| invalid("/outcomes"))?;
    if bytes.len() as u64 > node.budget().max_output_bytes() {
      return Err(invalid("/budget/max_output_bytes"));
    }
  }
  Ok(())
}

// A classification input may narrow an accepted assessment to its explicitly selected fields.
// Check both presence branches and reuse the schema owner's compatibility rules.
fn valid_projection<'a>(
  mapping: &'a FlowValueMapping,
  alias: &str,
  source: &FlowDataSchema,
  target: &FlowDataSchema,
  supported: &BTreeSet<octacity_server_factory::FactoryKey>,
  present: &mut Vec<&'a str>,
) -> bool {
  let (FlowValueSchema::Object { fields: source_fields }, FlowValueSchema::Object { fields: target_fields }) =
    (source.shape(), target.shape())
  else {
    return false;
  };
  if target_fields.is_empty() || !target_fields.keys().all(|key| supported.contains(key)) {
    return false;
  }
  match mapping {
    FlowValueMapping::Pointer { path } => path == alias && target.accepts_all(source.shape()),
    FlowValueMapping::Object { fields } => {
      let projected = fields
        .iter()
        .map(|(key, mapping)| {
          let original = source_fields.get(key)?;
          let FlowValueMapping::Pointer { path } = mapping else {
            return None;
          };
          (supported.contains(key)
            && path == &format!("{alias}/{key}")
            && (original.is_required() || present.contains(&path.as_str())))
          .then(|| (key.clone(), FlowFieldSchema::required(original.value().clone())))
        })
        .collect::<Option<_>>();
      projected.is_some_and(|fields| target.accepts_all(&FlowValueSchema::Object { fields }))
    }
    FlowValueMapping::IfPresent { path, then, otherwise } => {
      if !source_fields
        .keys()
        .any(|key| supported.contains(key) && path == &format!("{alias}/{key}"))
      {
        return false;
      }
      present.push(path);
      let then_valid = valid_projection(then, alias, source, target, supported, present);
      present.pop();
      then_valid && valid_projection(otherwise, alias, source, target, supported, present)
    }
    _ => false,
  }
}
