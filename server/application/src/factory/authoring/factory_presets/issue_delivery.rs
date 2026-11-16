//! Resolves configured snapshot fields, projections and choice conditions through common owners.
use super::*;
use octacity_server_store::PublishedIssueProperties;
use std::collections::BTreeMap;

const PARAMETERS: &[u8] = include_bytes!("../../../../../../config/factory/presets/ticket-delivery/parameters-v1.json");

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
  fields: BTreeMap<FactoryKey, FlowIssueFieldSource>,
  identity: FactoryKey,
  version: FactoryKey,
  source_alias: FactoryKey,
  admission_schema: FactoryKey,
  admission_allowed: Vec<FactoryKey>,
  schema_fields: BTreeMap<FactoryKey, Vec<FactoryKey>>,
  inputs: BTreeMap<FactoryKey, FlowDataMapping>,
  choice_condition: ChoiceCondition,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ChoiceCondition {
  node: FactoryKey,
  field: FactoryKey,
  path: String,
  outcome: FactoryKey,
  fallback: FactoryKey,
  output: FlowDataMapping,
}
fn parameters() -> Result<Parameters, String> {
  serde_json::from_slice(PARAMETERS).map_err(|_| "/issue_snapshot".to_owned())
}

pub(super) fn bind(
  template: &mut FlowDefinitionTemplate,
  selection: &FactoryPresetIssueSnapshotSelection,
  catalog: &PublishedIssueProperties,
) -> Result<FlowIssueSnapshotInput, String> {
  bind_contract(template, selection, catalog, true)
}

/// The common snapshot editor owns admission mapping and threshold edits after opening.
pub(super) fn draft(
  template: &mut FlowDefinitionTemplate,
  catalog: &PublishedIssueProperties,
) -> Result<FlowIssueSnapshotInput, String> {
  bind_contract(
    template,
    &FactoryPresetIssueSnapshotSelection {
      definition_version: catalog.version,
      admission: None,
      fields: BTreeMap::new(),
      choice_thresholds: BTreeMap::new(),
    },
    catalog,
    false,
  )
}

fn bind_contract(
  template: &mut FlowDefinitionTemplate,
  selection: &FactoryPresetIssueSnapshotSelection,
  catalog: &PublishedIssueProperties,
  configured: bool,
) -> Result<FlowIssueSnapshotInput, String> {
  let config = parameters()?;
  if configured {
    let invalid_admission = || "/issue_snapshot/admission".to_owned();
    let admission = selection.admission.as_ref().ok_or_else(invalid_admission)?;
    let source = template
      .schemas
      .get(&config.admission_schema)
      .ok_or_else(invalid_admission)?;
    let FlowValueSchema::Object { fields: declared } = &source.shape else {
      return Err(invalid_admission());
    };
    for field in &config.admission_allowed {
      let values = admission
        .allowed
        .get(field)
        .filter(|v| !v.is_empty())
        .ok_or_else(invalid_admission)?;
      let shape = declared.get(field).ok_or_else(invalid_admission)?;
      if values
        .iter()
        .any(|value| !shape.value().accepts_value(&serde_json::json!(value)))
      {
        return Err(invalid_admission());
      }
    }
  }
  let mut fields = BTreeMap::new();
  let mut shapes = BTreeMap::new();
  if selection.fields.keys().any(|alias| !config.fields.contains_key(alias)) {
    return Err("/issue_snapshot/fields".into());
  }
  for (alias, source) in &config.fields {
    let selected = selection
      .fields
      .get(alias)
      .cloned()
      .unwrap_or_else(|| FlowIssueFieldSelection {
        source: source.clone(),
        required: true,
      });
    let shape = crate::factory::issue_snapshot::field_shape(&selected, catalog)
      .or_else(|| {
        (!configured)
          .then(|| {
            config.schema_fields.keys().find_map(|schema| {
              let FlowValueSchema::Object { fields } = &template.schemas.get(schema)?.shape else {
                return None;
              };
              fields.get(alias).map(|field| field.value().clone())
            })
          })
          .flatten()
      })
      .ok_or_else(|| format!("/issue_snapshot/fields/{alias}"))?;
    shapes.insert(
      alias.clone(),
      if selected.required {
        FlowFieldSchema::required(shape)
      } else {
        FlowFieldSchema::optional(shape)
      },
    );
    fields.insert(alias.clone(), selected);
  }
  if configured {
    bind_choice_condition(template, &selection.choice_thresholds, catalog, &config, &fields)?;
  } else {
    let condition = &config.choice_condition;
    let initial = template
      .nodes
      .iter()
      .find(|node| node.key == condition.node)
      .and_then(|node| node.gate.as_ref())
      .and_then(|gate| gate.rules().first())
      .and_then(|rule| match &rule.when {
        FlowPredicate::Compare {
          comparison: FlowComparison::Equal,
          left: FlowOperand::Path { path },
          right: FlowOperand::Literal { value },
        } if rule.outcome == condition.outcome && path == &condition.path => value.as_str().map(str::to_owned),
        _ => None,
      });
    let thresholds = initial
      .map(|value| [(condition.node.clone(), value)].into())
      .unwrap_or_default();
    if bind_choice_condition(template, &thresholds, catalog, &config, &fields).is_err() {
      // An absent Project choice is an unfinished gate, never an inferred cutoff.
      template
        .nodes
        .iter_mut()
        .find(|node| node.key == condition.node)
        .ok_or_else(|| "/issue_snapshot/choice_thresholds".to_owned())?
        .gate = None;
    }
  }
  let invalid = || "/issue_snapshot/schema".to_owned();
  let schema = FlowDataSchema::new(
    config.identity.clone(),
    config.version.clone(),
    FlowValueSchema::Object { fields: shapes.clone() },
  )
  .map_err(|_| invalid())?;
  for (alias, selected) in &config.schema_fields {
    let schema = template.schemas.get_mut(alias).ok_or_else(invalid)?;
    let FlowValueSchema::Object { fields } = &mut schema.shape else {
      return Err(invalid());
    };
    for name in selected {
      let Some(value) = shapes.get(name) else {
        return Err(invalid());
      };
      if fields.get(name) != Some(value) {
        fields.insert(name.clone(), value.clone());
        schema.version = config.version.clone();
      }
    }
  }
  for (name, mapping) in config.inputs {
    let node = template
      .nodes
      .iter_mut()
      .find(|node| node.key == name)
      .ok_or_else(invalid)?;
    node.input_binding = FlowInputBinding::new(
      [(
        config.source_alias.clone(),
        FlowInputSource::Incoming {
          schema: schema.reference().clone(),
          path: String::new(),
        },
      )]
      .into(),
      mapping,
    )
    .map_err(|_| invalid())?;
  }
  template.schemas.insert(
    config.source_alias,
    FlowTemplateSchema {
      identity: schema.reference().identity().clone(),
      version: schema.reference().version().clone(),
      shape: schema.shape().clone(),
    },
  );
  Ok(FlowIssueSnapshotInput {
    version: 1,
    schema: schema.reference().clone(),
    definition_version: selection.definition_version,
    admission: selection.admission.clone(),
    fields,
    materials: Default::default(),
    evidence: Default::default(),
  })
}

fn bind_choice_condition(
  template: &mut FlowDefinitionTemplate,
  thresholds: &BTreeMap<FactoryKey, String>,
  catalog: &PublishedIssueProperties,
  config: &Parameters,
  fields: &BTreeMap<FactoryKey, FlowIssueFieldSelection>,
) -> Result<(), String> {
  let condition = &config.choice_condition;
  let invalid = || format!("/issue_snapshot/choice_thresholds/{}", condition.node);
  if thresholds.keys().any(|node| node != &condition.node) {
    return Err("/issue_snapshot/choice_thresholds".into());
  }
  let threshold = thresholds.get(&condition.node).ok_or_else(invalid)?;
  let source = &fields.get(&condition.field).ok_or_else(invalid)?.source;
  let program = FlowGateProgram::new(
    vec![FlowGateRule {
      when: crate::issue_choice_threshold(catalog, source, &condition.path, threshold).map_err(|_| invalid())?,
      outcome: condition.outcome.clone(),
    }],
    condition.fallback.clone(),
  )
  .and_then(|program| program.with_default_output_mapping(condition.output.clone()))
  .map_err(|_| invalid())?;
  template
    .nodes
    .iter_mut()
    .find(|node| node.key == condition.node)
    .ok_or_else(invalid)?
    .gate = Some(program);
  Ok(())
}

/// Discovery and resolution read the same configuration; Project definitions own value types.
pub(super) fn field_sources() -> BTreeMap<FactoryKey, FlowIssueFieldSource> {
  parameters().expect("bundled snapshot parameters").fields
}

pub(super) fn content() -> serde_json::Value {
  serde_json::from_slice(PARAMETERS).expect("bundled snapshot parameters")
}

pub(super) fn thresholds() -> BTreeMap<FactoryKey, FactoryKey> {
  let parameters = parameters().expect("bundled snapshot parameters");
  [(parameters.choice_condition.node, parameters.choice_condition.field)].into()
}
