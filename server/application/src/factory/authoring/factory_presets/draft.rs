//! Loading examples is independent of configuring installed execution bindings.
use super::*;

#[async_trait::async_trait]
impl<S: FactoryConfigurationStore> ManagementQueryUseCase<OpenFactoryPresetDraftQuery> for FactoryPresetHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: OpenFactoryPresetDraftQuery,
  ) -> Result<FactoryFlowDefinitionDraft, ApplicationError> {
    crate::factory::profiles::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
    let availability = self.store.factory_configuration_availability(query.project_id).await?;
    crate::factory::authoring::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Project,
      query.project_id,
    )
    .await?;
    let release = RELEASES
      .into_iter()
      .find(|release| release.matches(&query.preset))
      .ok_or_else(|| FactoryValidationDiagnostics::invalid("/preset"))?;
    let (mut template, reference) = release.load()?;
    if reference != query.preset {
      return Err(FactoryValidationDiagnostics::invalid("/preset"));
    }
    let parameters = release.parameters()?;
    if let Some(optional) = &parameters.optional_command {
      delivery::without_command(&mut template, optional);
    }
    let catalog = if release.issue_snapshot || !parameters.field_forms.is_empty() {
      crate::factory::issue_snapshot::current_catalog(
        self.issue_properties.as_deref(),
        self.policy.as_ref(),
        context,
        query.project_id,
      )
      .await?
    } else {
      None
    };
    let mut issue_snapshot = None;
    if let Some(catalog) = &catalog {
      if release.issue_snapshot {
        issue_snapshot =
          Some(issue_delivery::draft(&mut template, catalog).map_err(FactoryValidationDiagnostics::invalid)?);
      }
      let forms = parameters.field_forms;
      if !forms.is_empty()
        && matches!(
          availability,
          octacity_server_store::FactoryConfigurationAvailability::Available(_)
        )
      {
        let choices = crate::factory::authoring::factory_configuration_choices::visible_factory_choices(
          self.store.as_ref(),
          self.policy.as_ref(),
          context,
          query.project_id,
        )
        .await?;
        if let Some((writer, policy)) = choices.issue_classification() {
          fields::draft(&mut template, forms, catalog, writer, &policy.policy)
            .map_err(FactoryValidationDiagnostics::invalid)?;
        }
      }
    }
    let published = template
      .draft(query.flow_id, FlowDefinitionVersion::INITIAL)
      .map_err(|_| ApplicationError::invalid())?;
    let definition = published
      .definition
      .without_code()
      .and_then(|definition| definition.with_name(release.name.map(FactoryText::new).transpose()?))
      .map_err(|_| ApplicationError::invalid())?;
    let limits = FlowAdmissionLimits::product_defaults(
      template.execution.max_active_nodes(),
      template.execution.budget(),
      template.execution.permissions().clone(),
    )
    .map_err(|_| ApplicationError::invalid())?;
    let mut draft = effects::ordinary_draft(&definition, limits, published.schemas);
    draft.issue_snapshot = issue_snapshot;
    draft.schema_slots = template
      .nodes
      .iter()
      .filter(|node| node.kind == FlowNodeKind::BuildCommand)
      .flat_map(|node| &node.outcomes)
      .filter(|outcome| !template.schemas.contains_key(&outcome.schema))
      .filter_map(|outcome| {
        draft
          .data_schemas
          .iter()
          .find(|schema| schema.reference().identity() == &outcome.schema)
          .map(|schema| schema.reference().clone())
      })
      .collect();
    for node in &template.nodes {
      if node.build_profile.is_none() && node.capture_write_paths.is_none() && node.capture_deliverables.is_none() {
        continue;
      }
      let deliverables = node
        .capture_deliverables
        .as_ref()
        .map(|binding| {
          let policy = match &binding.policy {
            FlowTemplateDeliverablePolicy::Inline(policy) => policy.clone(),
            FlowTemplateDeliverablePolicy::Reference { reference } => template
              .deliverable_policies
              .get(reference)
              .cloned()
              .ok_or_else(ApplicationError::invalid)?,
          };
          Ok::<_, ApplicationError>(CandidateDeliverableBinding {
            accepted_contract: binding.accepted_contract.clone(),
            policy,
          })
        })
        .transpose()?;
      draft.execution_defaults.insert(
        node.key.clone(),
        FactoryFlowExecutionDefaults {
          capture_write_paths: node.capture_write_paths.clone(),
          capture_deliverables: deliverables,
        },
      );
    }
    Ok(draft)
  }
}
