//! Compile the released Issue field selections through the existing Project type owner.
use super::*;
use octacity_server_store::PublishedIssueProperties;
use std::collections::BTreeMap;

pub(super) fn bind(
  template: &mut FlowDefinitionTemplate,
  selection: &FactoryPresetIssueSnapshotSelection,
  catalog: &PublishedIssueProperties,
) -> Result<FlowIssueSnapshotInput, String> {
  let key = |value: &str| FactoryKey::new(value).expect("release key");
  let invalid_admission = || "/issue_snapshot/admission".to_owned();
  let admission = selection.admission.as_ref().ok_or_else(invalid_admission)?;
  let kinds = admission
    .allowed
    .get(&key("work_kind"))
    .filter(|kinds| !kinds.is_empty())
    .ok_or_else(invalid_admission)?;
  let route = template
    .schemas
    .get(&key("reproduction_route"))
    .ok_or_else(invalid_admission)?;
  let FlowValueSchema::Object { fields: route } = &route.shape else {
    return Err(invalid_admission());
  };
  let supported = route.get(&key("work_kind")).ok_or_else(invalid_admission)?;
  if kinds
    .iter()
    .any(|kind| !supported.value().accepts_value(&serde_json::json!(kind)))
  {
    return Err(invalid_admission());
  }
  let semantic = |slot| FlowIssueFieldSource::Classification { slot: key(slot) };
  let mut fields = BTreeMap::new();
  let mut shapes = BTreeMap::new();
  for (alias, source) in [
    ("priority", FlowIssueFieldSource::Priority),
    ("component", FlowIssueFieldSource::Component),
    ("work_kind", FlowIssueFieldSource::Type),
    ("size", FlowIssueFieldSource::Size),
    ("dependencies", FlowIssueFieldSource::Dependencies),
    ("risk", semantic("risk")),
    ("severity", semantic("severity")),
    ("recommended_route", semantic("recommended_route")),
    ("reproducibility", semantic("reproducibility")),
  ] {
    let selected = FlowIssueFieldSelection { source, required: true };
    let shape = crate::factory::issue_snapshot::field_shape(&selected, catalog)
      .ok_or_else(|| format!("/issue_snapshot/fields/{alias}"))?;
    fields.insert(key(alias), selected);
    shapes.insert(key(alias), FlowFieldSchema::required(shape));
  }
  let schema = FlowDataSchema::new(
    key("factory.ticket-delivery.issue-input"),
    key("v2"),
    FlowValueSchema::Object { fields: shapes.clone() },
  )
  .map_err(|_| "/issue_snapshot/schema".to_owned())?;
  let invalid = || "/issue_snapshot/schema".to_owned();
  let summary = template.schemas.get_mut(&key("summary")).ok_or_else(invalid)?;
  let FlowValueSchema::Object { fields: summary } = &mut summary.shape else {
    return Err(invalid());
  };
  summary.extend(shapes.clone());
  for alias in ["reproduction_route", "spec_handoff", "spec_gate_input"] {
    let schema = template.schemas.get_mut(&key(alias)).ok_or_else(invalid)?;
    let FlowValueSchema::Object { fields } = &mut schema.shape else {
      return Err(invalid());
    };
    for name in ["work_kind", "reproducibility"] {
      if let Some(field) = fields.get_mut(&key(name)) {
        let selected = &shapes[&key(name)];
        if field != selected {
          *field = selected.clone();
          schema.version = key("v2");
        }
      }
    }
  }
  let incoming = |mapping| {
    FlowInputBinding::new(
      [(
        key("issue"),
        FlowInputSource::Incoming {
          schema: schema.reference().clone(),
          path: String::new(),
        },
      )]
      .into(),
      FlowDataMapping::new(mapping).map_err(|_| invalid())?,
    )
    .map_err(|_| invalid())
  };
  let entry = template
    .nodes
    .iter_mut()
    .find(|node| node.key == template.entry)
    .ok_or_else(invalid)?;
  entry.input_binding = incoming(FlowValueMapping::Object {
    fields: ["work_kind", "reproducibility"]
      .map(|name| {
        (
          key(name),
          FlowValueMapping::Pointer {
            path: format!("/issue/{name}"),
          },
        )
      })
      .into(),
  })?;
  template
    .nodes
    .iter_mut()
    .find(|node| node.key == key("requirements_needed"))
    .ok_or_else(invalid)?
    .input_binding = incoming(FlowValueMapping::Pointer { path: "/issue".into() })?;
  template.schemas.insert(
    key("issue"),
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
