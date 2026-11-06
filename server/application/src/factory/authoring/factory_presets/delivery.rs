//! Binds the delivery example to reviewed dependencies before ordinary Flow authoring.
use super::*;
use std::collections::BTreeMap;

/// Projects only the explicit triage fields needed by the two delivery entry gates.
pub(super) fn bind_predecessor(
  template: &mut FlowDefinitionTemplate,
  predecessor: &FlowDataSchema,
) -> Result<(), FactoryError> {
  let invalid = || FactoryError::InvalidConfiguration {
    field: "ticket delivery predecessor schema",
  };
  let key = |value: &str| FactoryKey::new(value).expect("release key");
  let source = predecessor.shape();
  let schemas = template.data_schemas()?;
  let field = |name: &str, path: &str, target: &FlowDataSchema| {
    let target = required_field(target.shape(), name).ok_or_else(invalid)?;
    checked_pointer(source, path, target)
  };
  let route_key = key("reproduction_route");
  let route = schemas.get(&route_key).ok_or_else(invalid)?;
  let route_fields = [
    (key("work_kind"), field("work_kind", "/assessment/work_kind", route)?),
    (
      key("reproducibility"),
      field("reproducibility", "/assessment/reproducibility", route)?,
    ),
    (
      key("triage_accepted"),
      checked_pointer(source, "/accepted", &FlowValueSchema::Boolean)?,
    ),
  ];
  let requirements_key = key("delivery_requirements_input");
  let requirements = schemas.get(&requirements_key).ok_or_else(invalid)?;
  let FlowValueSchema::Object { fields } = required_field(requirements.shape(), "summary").ok_or_else(invalid)? else {
    return Err(invalid());
  };
  let summary = fields
    .iter()
    .filter(|(_, field)| field.is_required())
    .map(|(name, field)| {
      let mapping = match name.as_str() {
        "research_required" | "research_accepted" => FlowValueMapping::Constant { value: false.into() },
        "admission_risk" => checked_pointer(source, "/admission_risk", field.value())?,
        name => checked_pointer(source, &format!("/assessment/{name}"), field.value())?,
      };
      Ok((name.clone(), mapping))
    })
    .collect::<Result<_, FactoryError>>()?;
  let requirement_fields = [
    (key("summary"), FlowValueMapping::Object { fields: summary }),
    (
      key("priority_assessment"),
      field("priority_assessment", "/assessment/priority_assessment", requirements)?,
    ),
  ];
  let binding = |fields| {
    FlowInputBinding::new(
      [(
        key("triage"),
        FlowInputSource::Incoming {
          schema: predecessor.reference().clone(),
          path: String::new(),
        },
      )]
      .into(),
      FlowDataMapping::new(FlowValueMapping::Object { fields })?,
    )
  };
  let route_binding = binding(route_fields.into())?;
  let requirements_binding = binding(requirement_fields.into())?;
  let entry = template
    .nodes
    .iter_mut()
    .find(|node| node.key == template.entry)
    .ok_or_else(invalid)?;
  entry.gate = Some(
    entry
      .gate
      .take()
      .ok_or_else(invalid)?
      .with_precondition(FlowPredicate::Compare {
        left: FlowOperand::Path {
          path: "/triage_accepted".into(),
        },
        comparison: FlowComparison::Equal,
        right: FlowOperand::Literal { value: true.into() },
      })?,
  );
  entry.input_binding = route_binding;
  template
    .nodes
    .iter_mut()
    .find(|node| node.key.as_str() == "requirements_needed")
    .ok_or_else(invalid)?
    .input_binding = requirements_binding;
  let Some(FlowTemplateSchema {
    shape: FlowValueSchema::Object { fields },
    ..
  }) = template.schemas.get_mut(&route_key)
  else {
    return Err(invalid());
  };
  fields.insert(
    key("triage_accepted"),
    FlowFieldSchema::required(FlowValueSchema::Boolean),
  );
  template.schemas.insert(
    key("predecessor"),
    FlowTemplateSchema {
      identity: predecessor.reference().identity().clone(),
      version: predecessor.reference().version().clone(),
      shape: source.clone(),
    },
  );
  Ok(())
}

fn required_field<'a>(schema: &'a FlowValueSchema, name: &str) -> Option<&'a FlowValueSchema> {
  let FlowValueSchema::Object { fields } = schema else {
    return None;
  };
  fields
    .iter()
    .find(|(key, field)| key.as_str() == name && field.is_required())
    .map(|(_, field)| field.value())
}

fn checked_pointer(
  source: &FlowValueSchema,
  path: &str,
  target: &FlowValueSchema,
) -> Result<FlowValueMapping, FactoryError> {
  let invalid = || FactoryError::InvalidConfiguration {
    field: "ticket delivery predecessor field",
  };
  let source = path
    .trim_start_matches('/')
    .split('/')
    .try_fold(source, required_field)
    .ok_or_else(invalid)?;
  let target = FlowDataSchema::new(
    "projection".parse().expect("schema key"),
    "v1".parse().expect("schema version"),
    target.clone(),
  )?;
  if !target.accepts_all(source) {
    return Err(invalid());
  }
  Ok(FlowValueMapping::Pointer {
    path: format!("/triage{path}"),
  })
}

/// Resolves release-local aliases; the resulting graph uses only ordinary node contracts.
pub(super) fn bind_installed_steps(
  template: &mut FlowDefinitionTemplate,
  commands: &BTreeMap<FactoryKey, FactoryPresetCommandSelection>,
  publication: &FactoryPresetPublicationSelection,
  publisher: &FactoryCandidatePublicationChoice,
) -> Result<BTreeMap<FactoryKey, FlowBuildBinding>, String> {
  let schemas = template.data_schemas().map_err(|_| "/commands".to_owned())?;
  let mut builds = BTreeMap::new();
  for node in template
    .nodes
    .iter_mut()
    .filter(|node| node.kind == FlowNodeKind::BuildCommand)
  {
    let alias = node.build_profile.as_ref().ok_or_else(|| "/commands".to_owned())?;
    let path = || format!("/commands/{alias}");
    let command = commands.get(alias).ok_or_else(path)?;
    let [outcome] = node.outcomes.as_slice() else {
      return Err(path());
    };
    if command.build.result_outcome != outcome.key || command.build.result_output_kind != EvidenceOutputKind::Report {
      return Err(path());
    }
    match schemas.get(&outcome.schema) {
      Some(schema) if schema != &command.result => return Err(format!("{}/result", path())),
      Some(_) => (),
      None => {
        let reference = command.result.reference();
        template.schemas.insert(
          outcome.schema.clone(),
          FlowTemplateSchema {
            identity: reference.identity().clone(),
            version: reference.version().clone(),
            shape: command.result.shape().clone(),
          },
        );
      }
    }
    node.permissions = command.permissions.clone();
    builds.insert(alias.clone(), command.build.clone());
  }
  let node = template
    .nodes
    .iter_mut()
    .find(|node| node.key.as_str() == "publish_pull_request")
    .ok_or_else(|| "/publication".to_owned())?;
  let original = node.action.as_ref().ok_or_else(|| "/publication".to_owned())?;
  let result = node
    .outcomes
    .iter()
    .find(|outcome| outcome.kind == FlowOutcomeKind::Success)
    .ok_or_else(|| "/publication".to_owned())?;
  if publisher.outcome.key() != &result.key
    || publisher.outcome.kind() != result.kind
    || !publisher.receipt_schema.as_ref().is_some_and(|receipt| {
      schemas.get(&result.schema) == Some(receipt) && publisher.outcome.schema() == receipt.reference()
    })
  {
    return Err("/publication/result".to_owned());
  }
  if !publication
    .permissions
    .plugins()
    .any(|plugin| plugin == &publication.plugin)
    || !publisher.permissions.is_no_broader_than(&publication.permissions)
  {
    return Err("/publication/permissions".to_owned());
  }
  let mut parameters = original.parameters().clone();
  parameters["target"] = serde_json::to_value(&publication.target).map_err(|_| "/publication/target".to_owned())?;
  node.action = Some(
    FlowActionBinding::new(publication.plugin.clone(), original.action().clone(), parameters)
      .map_err(|_| "/publication".to_owned())?,
  );
  node.permissions = publication.permissions.clone();
  Ok(builds)
}
