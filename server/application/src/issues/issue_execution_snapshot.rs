//! Resolves canonical Issue selections once, before the existing recoverable intake.
use crate::*;
use octacity_server_factory::*;
use octacity_server_store::*;
use std::collections::BTreeMap;

/// A retained digest permits only an exact pre-format reservation to keep its legacy bytes.
pub(crate) async fn select(
  store: &impl IssuePropertyStore,
  policy: &dyn ManagementAuthorizationPolicy,
  context: &ManagementRequestContext,
  issue: &IssueRevision,
  target: &IssueExecutionTarget,
  flow: &FactoryFlowConfiguration,
  retained_digest: Option<FactoryDigest>,
) -> Result<IssueExecutionSnapshot, ApplicationError> {
  let input = flow.issue_snapshot.as_ref().ok_or_else(ApplicationError::invalid)?;
  if target.predecessor_run.is_some() || !issue.content_available {
    return Err(FactoryValidationDiagnostics::invalid("/issue_snapshot"));
  }
  let catalog = crate::factory::issue_snapshot::resolve(flow, issue.project_id, Some(store), policy, context)
    .await?
    .ok_or_else(ApplicationError::invalid)?;
  let selected: Vec<_> = input
    .fields
    .iter()
    .map(|(alias, selection)| (alias, selection, field_value(&selection.source, issue, &catalog)))
    .collect();
  let mut references: Vec<_> = selected
    .iter()
    .filter_map(|(_, _, (_, reference))| reference.clone())
    .collect();
  let mut definitions = BTreeMap::from([(catalog.version, catalog.clone())]);
  for field in &issue.classification.fields {
    if !selected.iter().any(|(_, _, (_, reference))| {
      reference
        .as_ref()
        .is_some_and(|selected| selected.identity() == field.field.identity())
    }) {
      continue;
    }
    let version = field.field.definition_version();
    if let Some(version) = version
      && let std::collections::btree_map::Entry::Vacant(entry) = definitions.entry(version)
    {
      entry.insert(store.issue_properties(issue.project_id, Some(version)).await?);
    }
  }
  let review = issue.review_classification(&catalog, &definitions, None, issue.updated_at);
  let mut snapshot = IssueExecutionSnapshot {
    project_id: issue.project_id,
    issue_id: issue.id,
    version: issue.version,
    applicability_version: issue.applicability_version,
    target: target.clone(),
    title_digest: FactoryDigest::content_sha256(issue.content.title.as_bytes()),
    description_digest: FactoryDigest::content_sha256(issue.content.description.as_bytes()),
    acceptance_digest: FactoryDigest::content_sha256(
      issue.content.acceptance.as_deref().unwrap_or_default().as_bytes(),
    ),
    input: input.clone(),
    definitions: BTreeMap::new(),
    values: BTreeMap::new(),
    attribution: BTreeMap::new(),
    omissions: BTreeMap::new(),
  };
  for (alias, selection, (value, field)) in selected {
    let Some(value) = value else {
      if selection.required {
        return Err(FactoryValidationDiagnostics::unavailable_with_code(
          format!("/issue_snapshot/fields/{alias}"),
          "issue_snapshot_missing_value",
        ));
      }
      snapshot.omissions.insert(alias.clone(), IssueSnapshotOmission::Missing);
      continue;
    };
    // A supplied value needs a nonempty vocabulary even when its projection is optional.
    let supplied = FlowIssueFieldSelection {
      required: true,
      ..selection.clone()
    };
    if !crate::factory::issue_snapshot::field_shape(&supplied, &catalog)
      .is_some_and(|shape| shape.accepts_value(&value))
    {
      return Err(FactoryValidationDiagnostics::unavailable_with_code(
        format!("/issue_snapshot/fields/{alias}"),
        "issue_snapshot_incompatible_value",
      ));
    }
    let assessment = field.as_ref().and_then(|field| {
      issue
        .classification
        .fields
        .iter()
        .find(|a| a.field.identity() == field.identity())
    });
    if assessment.is_some_and(|assessment| !same_semantic_binding(assessment, &catalog, &definitions)) {
      return Err(FactoryValidationDiagnostics::unavailable_with_code(
        format!("/issue_snapshot/fields/{alias}"),
        "issue_snapshot_semantic_binding_changed",
      ));
    }
    let applicable = field.as_ref().is_none_or(|field| {
      // Ordinary properties need no classification origin. Semantic bindings and built-ins do.
      let classified = !matches!(field, IssueClassificationFieldReference::Property { id, .. }
        if !catalog.definition.classification.fields().any(|(_, bound)| bound == id));
      !classified
        || review
          .fields
          .iter()
          .find(|r| r.field.identity() == field.identity())
          .is_some_and(|r| {
            r.problems
              .iter()
              .all(|p| matches!(p, IssueClassificationProblem::CodeSubjectUnavailable))
              && !r.requires_verified_evidence
          })
    });
    if !applicable {
      if selection.required {
        return Err(FactoryValidationDiagnostics::unavailable_with_code(
          format!("/issue_snapshot/fields/{alias}"),
          "issue_snapshot_inapplicable",
        ));
      }
      snapshot
        .omissions
        .insert(alias.clone(), IssueSnapshotOmission::Inapplicable);
      continue;
    }
    snapshot.values.insert(alias.clone(), value);
    if let Some(assessment) = assessment {
      snapshot.attribution.insert(alias.clone(), assessment.clone());
    }
  }

  // Material/evidence readers are independent capabilities. Until configured, a
  // required selection blocks; optional selections retain an explicit omission.
  for (alias, required) in input
    .materials
    .iter()
    .map(|(a, s)| (a, s.required))
    .chain(input.evidence.iter().map(|(a, s)| (a, s.required)))
  {
    if required {
      return Err(FactoryValidationDiagnostics::unavailable(format!(
        "/issue_snapshot/{alias}"
      )));
    }
    snapshot
      .omissions
      .insert(alias.clone(), IssueSnapshotOmission::Unavailable);
  }
  let schema = flow
    .data_schemas
    .iter()
    .find(|s| s.reference() == &input.schema)
    .ok_or_else(ApplicationError::invalid)?;
  FlowPayload::new(
    schema,
    serde_json::to_value(&snapshot.values).map_err(|_| ApplicationError::invalid())?,
  )
  .map_err(|_| FactoryValidationDiagnostics::invalid("/issue_snapshot/fields"))?;
  references.extend(snapshot.attribution.values().map(|assessment| assessment.field.clone()));
  for (&version, definition) in &definitions {
    if version == input.definition_version
      || references
        .iter()
        .any(|field| field.definition_version() == Some(version))
    {
      snapshot
        .definitions
        .insert(version, IssueSnapshotDefinitions::selected(definition, &references)?);
    }
  }
  let digest = snapshot.digest()?;
  if let Some(expected) = retained_digest
    && digest != expected
  {
    // An existing reservation may predate the fragment format. All readiness
    // checks above still apply; only its exact already-sealed legacy bytes qualify.
    snapshot.definitions = definitions
      .into_iter()
      .map(|(version, catalog)| (version, IssueSnapshotDefinitions::Legacy(catalog)))
      .collect();
    if snapshot.digest()? != expected {
      return Err(ApplicationError::invalid());
    }
  }
  Ok(snapshot)
}

// A value assessed as risk cannot become reproduction merely because an older
// consumer selects the same property ID. Compatible catalog revisions remain usable.
fn same_semantic_binding(
  assessment: &IssueClassificationAttribution,
  consumer: &PublishedIssueProperties,
  definitions: &BTreeMap<u64, PublishedIssueProperties>,
) -> bool {
  let IssueClassificationFieldReference::Property { id, definition_version } = &assessment.field else {
    return true;
  };
  consumer
    .definition
    .classification
    .fields()
    .find(|(_, bound)| *bound == id)
    .is_none_or(|(slot, _)| {
      definitions.get(definition_version).is_some_and(|origin| {
        origin
          .definition
          .classification
          .fields()
          .any(|(original_slot, bound)| original_slot == slot && bound == id)
      })
    })
}

fn field_value(
  source: &FlowIssueFieldSource,
  issue: &IssueRevision,
  catalog: &PublishedIssueProperties,
) -> (Option<serde_json::Value>, Option<IssueClassificationFieldReference>) {
  use FlowIssueFieldSource as Source;
  use IssueClassificationFieldReference as Field;
  let version = catalog.version;
  let string = |s: &String| serde_json::Value::String(s.clone());
  let property = |id: &str| {
    let value = issue.planning.values.get(id).map(|value| match value {
      IssuePropertyValue::Text(v) | IssuePropertyValue::Choice(v) | IssuePropertyValue::Date(v) => string(v),
      IssuePropertyValue::Number(v) => (*v).into(),
      IssuePropertyValue::Boolean(v) => (*v).into(),
    });
    (
      value,
      Some(Field::Property {
        id: id.to_owned(),
        definition_version: version,
      }),
    )
  };
  match source {
    Source::Title => (Some(string(&issue.content.title)), None),
    Source::Description => (Some(string(&issue.content.description)), None),
    Source::Acceptance => (issue.content.acceptance.as_ref().map(string), None),
    Source::Priority => (
      match &issue.content.priority {
        Some(IssuePriority::Choice(v)) => Some(string(v)),
        _ => None,
      },
      Some(Field::Priority {
        definition_version: Some(version),
      }),
    ),
    Source::Component => (
      issue.planning.component.as_ref().map(string),
      Some(Field::Component {
        definition_version: version,
      }),
    ),
    Source::Type => (
      issue.planning.issue_type.as_ref().map(string),
      Some(Field::Type {
        definition_version: version,
      }),
    ),
    Source::Size => (
      issue
        .planning
        .size
        .map(|v| serde_json::to_value(v).expect("Issue size")),
      Some(Field::Size),
    ),
    Source::Dependencies => (
      Some(serde_json::to_value(&issue.classification.dependencies).expect("typed dependencies")),
      Some(Field::Dependencies),
    ),
    Source::Property { id } => property(id),
    Source::Classification { slot } => catalog
      .definition
      .classification
      .fields()
      .find(|(name, _)| *name == slot.as_str())
      .map(|(_, id)| property(id))
      .unwrap_or((None, None)),
  }
}
