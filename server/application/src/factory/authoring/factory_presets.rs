//! Release presets resolve through ordinary Factory authoring; queries grant no execution authority.
use crate::*;
mod build;
mod contracts;
mod delivery;
mod effects;
mod input;
mod issue_delivery;
mod release;
#[cfg(test)]
mod tests;
pub use contracts::*;
use octacity_server_factory::*;
use octacity_server_store::FactoryConfigurationStore;
pub use octacity_server_store::MAX_SOURCE_BODY_BYTES;
use release::RELEASES;
use std::sync::Arc;

/// Shared read-only setup owner; publication remains in the ordinary versioned handlers.
pub struct FactoryPresetHandlers<S> {
  store: Arc<S>,
  source_actions: Option<Arc<dyn FactoryStageSourceAuthorizer>>,
  sources: Option<Arc<dyn AuthorizedManagementQueryHandler<ListWorkSourceInputChoicesQuery, Error = ApplicationError>>>,
  artifacts: Option<Arc<dyn IssueArtifactReader>>,
  repositories: Option<Arc<dyn AuthorizedManagementQueryHandler<GetRepositoryQuery, Error = ApplicationError>>>,
  flows: Option<Arc<dyn AuthorizedManagementQueryHandler<GetFactoryFlowDefinitionQuery, Error = ApplicationError>>>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  issue_properties: Option<Arc<dyn octacity_server_store::IssuePropertyStore>>,
}
impl<S> FactoryPresetHandlers<S> {
  /// Uses the existing Project catalogs and management authority, without provider services.
  pub fn new(store: Arc<S>, policy: Arc<dyn ManagementAuthorizationPolicy>) -> Self {
    Self {
      store,
      policy,
      issue_properties: None,
      repositories: None,
      flows: None,
      artifacts: None,
      sources: None,
      source_actions: None,
    }
  }
  /// Resolves Project-owned classification types under the granted Project visibility.
  pub fn with_issue_properties(mut self, properties: Arc<dyn octacity_server_store::IssuePropertyStore>) -> Self {
    self.issue_properties = Some(properties);
    self
  }
  /// Reuses current installed Source/Reporter validation for ordinary reporting actions.
  pub fn with_source_actions(mut self, authorizer: Arc<dyn FactoryStageSourceAuthorizer>) -> Self {
    self.source_actions = Some(authorizer);
    self
  }
  /// Reuses installed credential-free Source/Reporter choices and current Project access.
  pub fn with_sources(
    mut self,
    sources: Arc<dyn AuthorizedManagementQueryHandler<ListWorkSourceInputChoicesQuery, Error = ApplicationError>>,
  ) -> Self {
    self.sources = Some(sources);
    self
  }
  /// Uses the existing retained Artifact metadata authority, never its content method.
  pub fn with_artifacts(mut self, artifacts: Arc<dyn IssueArtifactReader>) -> Self {
    self.artifacts = Some(artifacts);
    self
  }
  /// Reuses the authorized immutable Repository metadata owner; no VCS calls occur.
  pub fn with_repositories(
    mut self,
    repositories: Arc<dyn AuthorizedManagementQueryHandler<GetRepositoryQuery, Error = ApplicationError>>,
  ) -> Self {
    self.repositories = Some(repositories);
    self
  }
  /// Reuses exact, authorized Flow metadata to resolve a published predecessor.
  pub fn with_flows(
    mut self,
    flows: Arc<dyn AuthorizedManagementQueryHandler<GetFactoryFlowDefinitionQuery, Error = ApplicationError>>,
  ) -> Self {
    self.flows = Some(flows);
    self
  }
}
#[async_trait::async_trait]
impl<S: FactoryConfigurationStore> ManagementQueryUseCase<ListFactoryPresetsQuery> for FactoryPresetHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListFactoryPresetsQuery,
  ) -> Result<FactoryPresetPage, ApplicationError> {
    crate::factory::profiles::factory_role_profiles::require_project_visibility(grant, query.page.project_id())?;
    self
      .store
      .factory_configuration_availability(query.page.project_id())
      .await?;
    crate::factory::authoring::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Project,
      query.page.project_id(),
    )
    .await?;
    let after = query.page.after();
    let version_offset = after
      .and_then(|cursor| RELEASES.iter().position(|release| &release.cursor() == cursor))
      .map(|index| index + 1);
    let mut items = Vec::new();
    for release in RELEASES.into_iter().skip(version_offset.unwrap_or(0)) {
      let (template, reference) = release.load()?;
      // Older identity cursors still advance beyond every version of that identity.
      if version_offset.is_some() || after.is_none_or(|after| reference.identity() > after) {
        items.push((
          release.cursor(),
          summary(template, reference, release.name, release.issue_snapshot)?,
        ));
      }
    }
    let limit = usize::from(query.page.limit());
    let next_cursor = (items.len() > limit).then(|| items[limit - 1].0.clone());
    items.truncate(limit);
    Ok(FactoryPresetPage {
      total: RELEASES.len() as u64,
      items: items.into_iter().map(|(_, item)| item).collect(),
      next_cursor,
    })
  }
}

fn summary(
  template: FlowDefinitionTemplate,
  reference: ImmutableReference,
  name: Option<&'static str>,
  issue_snapshot: bool,
) -> Result<FactoryPresetSummary, ApplicationError> {
  let delivery = name.is_some();
  let mut parameters = vec![
    parameter("/flow_id", FactoryPresetParameterKind::Identity),
    parameter("/configuration_id", FactoryPresetParameterKind::Identity),
    parameter("/repository", FactoryPresetParameterKind::Repository),
    parameter("/execution", FactoryPresetParameterKind::ExecutionPolicy),
    parameter("/configuration", FactoryPresetParameterKind::ConfigurationPolicy),
  ];
  parameters.push(FactoryPresetParameter {
    path: "/input".into(),
    kind: FactoryPresetParameterKind::SourceInput,
    required: false,
  });
  let mut required_node_kinds = Vec::new();
  let mut profile_aliases = Vec::new();
  if delivery {
    parameters.push(parameter("/publication", FactoryPresetParameterKind::Publication));
    parameters.push(parameter(
      if issue_snapshot {
        "/issue_snapshot"
      } else {
        "/accepted_predecessor"
      },
      if issue_snapshot {
        FactoryPresetParameterKind::IssueSnapshot
      } else {
        FactoryPresetParameterKind::AcceptedPredecessor
      },
    ));
  }
  for node in &template.nodes {
    if !required_node_kinds.contains(&node.kind) {
      required_node_kinds.push(node.kind);
    }
    if node.kind == FlowNodeKind::DeterministicGate {
      parameters.push(FactoryPresetParameter {
        path: format!("/effects/{}", node.key),
        kind: FactoryPresetParameterKind::ReportingEffects,
        required: false,
      });
    }
    if let Some(alias) = &node.build_profile {
      if delivery && node.kind == FlowNodeKind::BuildCommand {
        parameters.push(FactoryPresetParameter {
          path: format!("/commands/{alias}"),
          kind: FactoryPresetParameterKind::Command,
          required: alias.as_str() != "reproduction",
        });
        continue;
      }
      profile_aliases.push(alias.clone());
      parameters.push(parameter(
        &format!("/profiles/{alias}"),
        FactoryPresetParameterKind::RoleProfile,
      ));
      if !delivery {
        parameters.push(parameter(
          &format!("/context/{alias}"),
          FactoryPresetParameterKind::ProjectContext,
        ));
      }
    }
  }
  let editor = if let Some(name) = name {
    let schemas = template.data_schemas().map_err(|_| ApplicationError::invalid())?;
    Some(FactoryPresetEditor {
      name,
      budget: template.execution.budget(),
      max_active_nodes: template.execution.max_active_nodes(),
      steps: template
        .nodes
        .iter()
        .filter_map(|node| {
          let alias = node.build_profile.clone()?;
          let [outcome] = node.outcomes.as_slice() else {
            return None;
          };
          Some(FactoryPresetEditorStep {
            alias,
            kind: node.kind,
            outcome: outcome.key.clone(),
            result: schemas.get(&outcome.schema).cloned(),
          })
        })
        .collect(),
    })
  } else {
    None
  };
  Ok(FactoryPresetSummary {
    parameters,
    required_node_kinds,
    reference,
    profile_aliases,
    editor,
  })
}
#[async_trait::async_trait]
impl<S: FactoryConfigurationStore + octacity_server_store::FactoryRoleProfileStore>
  ManagementQueryUseCase<ResolveFactoryPresetQuery> for FactoryPresetHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ResolveFactoryPresetQuery,
  ) -> Result<FactoryPresetResolution, ApplicationError> {
    crate::factory::profiles::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
    self.store.factory_configuration_availability(query.project_id).await?;
    crate::factory::authoring::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Project,
      query.project_id,
    )
    .await?;
    let release = RELEASES
      .into_iter()
      .find(|release| release.matches(&query.selection.preset))
      .ok_or_else(|| FactoryValidationDiagnostics::invalid("/preset"))?;
    let (mut template, preset) = release.load()?;
    if preset != query.selection.preset {
      return Err(FactoryValidationDiagnostics::invalid("/preset"));
    }
    let mut result = FactoryPresetResolution {
      preset,
      completion: SourceCompletionDocument::review_preset(query.project_id, release.completion)
        .map_err(|_| ApplicationError::invalid())?,
      diagnostics: vec![],
      flow: None,
      configuration: None,
      input: None,
      dependencies: vec![],
    };
    let p = query.selection;
    let delivery = release.is_delivery();
    if release.issue_snapshot && p.accepted_predecessor.is_some() {
      return Err(FactoryValidationDiagnostics::invalid("/accepted_predecessor"));
    }
    if !release.issue_snapshot && p.issue_snapshot.is_some() {
      return Err(FactoryValidationDiagnostics::invalid("/issue_snapshot"));
    }
    if p.flow_only && (!delivery || p.input.is_some() || p.configuration.is_some() || p.configuration_id.is_some()) {
      return Err(FactoryValidationDiagnostics::invalid("/flow_only"));
    }
    if delivery
      && !p
        .commands
        .contains_key(&FactoryKey::new("reproduction").expect("release key"))
    {
      delivery::without_reproduction(&mut template);
    }
    let commands: std::collections::BTreeSet<_> = template
      .nodes
      .iter()
      .filter(|node| delivery && node.kind == FlowNodeKind::BuildCommand)
      .filter_map(|node| node.build_profile.clone())
      .collect();
    let aliases: std::collections::BTreeSet<_> = template
      .nodes
      .iter()
      .filter_map(|node| node.build_profile.clone())
      .filter(|alias| !commands.contains(alias))
      .collect();
    if p.profiles.keys().any(|key| !aliases.contains(key)) {
      return Err(FactoryValidationDiagnostics::invalid("/profiles"));
    }
    if p.context.keys().any(|key| delivery || !aliases.contains(key)) {
      return Err(FactoryValidationDiagnostics::invalid("/context"));
    }
    for alias in &aliases {
      if !p.profiles.contains_key(alias) {
        result.diagnostics.push(required(format!("/profiles/{alias}")));
      }
    }
    if p.commands.keys().any(|alias| !commands.contains(alias)) {
      return Err(FactoryValidationDiagnostics::invalid("/commands"));
    }
    for alias in &commands {
      if !p.commands.contains_key(alias) {
        result.diagnostics.push(required(format!("/commands/{alias}")));
      }
    }
    for (missing, path) in [
      (!p.flow_only && p.repository.is_none(), "/repository"),
      (!delivery && p.context.is_empty(), "/context"),
      (delivery && p.publication.is_none(), "/publication"),
      (
        delivery && !release.issue_snapshot && p.accepted_predecessor.is_none(),
        "/accepted_predecessor",
      ),
      (release.issue_snapshot && p.issue_snapshot.is_none(), "/issue_snapshot"),
      (p.flow_id.is_none(), "/flow_id"),
      (!p.flow_only && p.configuration_id.is_none(), "/configuration_id"),
      (p.execution.is_none(), "/execution"),
      (!p.flow_only && p.configuration.is_none(), "/configuration"),
    ] {
      if missing {
        result.diagnostics.push(required(path.into()));
      }
    }
    if !result.diagnostics.is_empty() {
      return Ok(result);
    }
    if let Some(repository) = &p.repository {
      if repository.project_id != query.project_id {
        return Err(FactoryValidationDiagnostics::invalid("/repository"));
      }
      if crate::factory::authoring::factory_configuration_choices::require_visible_reference(
        self.policy.as_ref(),
        context,
        ManagementResourceKind::Repository,
        repository.id,
      )
      .await
      .is_err()
      {
        return Ok(result.unavailable("/repository"));
      }
      let selected_repository = match &self.repositories {
        Some(reader) => reader
          .handle_authorized_query(
            context,
            GetRepositoryQuery {
              repository_id: repository.id,
              version: repository.version,
            },
          )
          .await
          .ok(),
        None => None,
      };
      if !selected_repository.as_ref().is_some_and(|r| {
        r.project_id == query.project_id
          && r.id == repository.id
          && r.version == repository.version
          && r.definition_digest().ok().as_ref() == Some(&repository.definition_digest)
      }) {
        result.diagnostics.push(FactoryPresetDependencyProblem {
          path: "/repository".into(),
          reason: FactoryPresetDependencyReason::Unavailable,
        });
        return Ok(result);
      }
    }
    let now = SystemFactoryExecutionClock.now()?;
    for alias in aliases.iter().filter(|_| !delivery) {
      let Some(selection) = p.context.get(alias) else {
        result.diagnostics.push(required(format!("/context/{alias}")));
        continue;
      };
      if selection.bind(query.project_id).is_err() {
        result.diagnostics.push(required(format!("/context/{alias}")));
        continue;
      }
      for (name, artifact) in [("goals", selection.goals.as_ref()), ("rules", selection.rules.as_ref())]
        .into_iter()
        .filter_map(|(name, artifact)| artifact.map(|artifact| (name.to_owned(), artifact)))
        .chain(
          selection
            .documents
            .iter()
            .map(|(key, value)| (format!("documents/{key}"), value)),
        )
      {
        let visible = crate::factory::authoring::factory_configuration_choices::require_visible_reference(
          self.policy.as_ref(),
          context,
          ManagementResourceKind::Artifact,
          artifact.artifact_id(),
        )
        .await
        .is_ok();
        let metadata = match (&self.artifacts, visible) {
          (Some(reader), true) => reader
            .metadata(ReadFactoryArtifact {
              project_id: query.project_id,
              artifact: artifact.clone(),
              maximum_bytes: octacity_artifact_store::MAX_ARTIFACT_READ_BYTES,
              observed_at: now,
              required_until: now,
            })
            .await
            .ok(),
          _ => None,
        };
        let still_visible = crate::factory::authoring::factory_configuration_choices::require_visible_reference(
          self.policy.as_ref(),
          context,
          ManagementResourceKind::Artifact,
          artifact.artifact_id(),
        )
        .await
        .is_ok();
        if !still_visible || !metadata.is_some_and(|m| m.availability == IssueArtifactAvailability::Available) {
          result.diagnostics.push(FactoryPresetDependencyProblem {
            path: format!("/context/{alias}/{name}"),
            reason: FactoryPresetDependencyReason::Unavailable,
          });
        }
      }
    }
    if !result.diagnostics.is_empty() {
      return Ok(result);
    }
    let choices = match crate::factory::authoring::factory_configuration_choices::visible_factory_choices(
      self.store.as_ref(),
      self.policy.as_ref(),
      context,
      query.project_id,
    )
    .await
    {
      Ok(choices) => choices,
      Err(_) => return Ok(result.unavailable("/execution")),
    };
    let publication = if delivery {
      let publication = p.publication.as_ref().expect("required selection");
      let Some(publisher) = choices.candidate_publications().iter().find(|choice| {
        choice.plugin == publication.plugin
          && choice.target == publication.target
          && choice.review_url
          && choice.receipt_schema.is_some()
      }) else {
        return Ok(result.unavailable("/publication/target"));
      };
      Some((publication, publisher))
    } else {
      None
    };
    if let Some((alias, _)) = p.commands.iter().find(|(_, selection)| {
      !choices.supports_build_binding(&selection.build)
        || choices
          .build_pipeline_contract(&selection.build)
          .is_none_or(|contract| !contract.matches_command(&selection.build, selection.result.reference()))
    }) {
      return Ok(result.unavailable(format!("/commands/{alias}")));
    }
    let command_bindings = match publication {
      Some((publication, publisher)) => {
        match delivery::bind_installed_steps(&mut template, &p.commands, publication, publisher) {
          Ok(builds) => builds,
          Err(path) => return Ok(result.unavailable(path)),
        }
      }
      None => Default::default(),
    };
    if let Some(predecessor) = p.accepted_predecessor.as_ref().filter(|_| delivery) {
      let Some(reader) = &self.flows else {
        return Ok(result.unavailable("/accepted_predecessor"));
      };
      let Ok(published) = reader
        .handle_authorized_query(
          context,
          GetFactoryFlowDefinitionQuery {
            id: predecessor.definition.id(),
            version: Some(predecessor.definition.version()),
          },
        )
        .await
      else {
        return Ok(result.unavailable("/accepted_predecessor"));
      };
      let Some(schema) = super::factory_flow_definition::predecessor_schema(&published, query.project_id, predecessor)
      else {
        return Ok(result.unavailable("/accepted_predecessor"));
      };
      if delivery::bind_predecessor(&mut template, schema).is_err() {
        return Ok(result.unavailable("/accepted_predecessor/schema"));
      }
    }
    let issue_snapshot = if let Some(selection) = &p.issue_snapshot {
      let unavailable = || "/issue_snapshot/definition_version";
      let Some(properties) = &self.issue_properties else {
        return Ok(result.unavailable(unavailable()));
      };
      let catalog = match properties
        .issue_properties(query.project_id, Some(selection.definition_version))
        .await
      {
        Ok(catalog) if catalog.project_id == query.project_id && catalog.version == selection.definition_version => {
          catalog
        }
        Ok(_) | Err(octacity_server_store::StoreError::NotFound { .. }) => {
          return Ok(result.unavailable(unavailable()));
        }
        Err(error) => return Err(error.into()),
      };
      match issue_delivery::bind(&mut template, selection, &catalog) {
        Ok(input) => Some(input),
        Err(path) => return Ok(result.unavailable(path)),
      }
    } else {
      None
    };
    template.execution = p.execution.unwrap();
    for node in &mut template.nodes {
      let Some(alias) = node.build_profile.clone() else {
        continue;
      };
      if command_bindings.contains_key(&alias) {
        continue;
      }
      let Some(selected) = p.profiles.get(&alias) else {
        return Ok(result.unavailable(format!("/commands/{alias}")));
      };
      let role = self
        .store
        .factory_role(query.project_id, selected.role.id, Some(selected.role.version))
        .await;
      let profile = self
        .store
        .factory_role_profile(query.project_id, selected.profile.id, Some(selected.profile.version))
        .await;
      let (Ok(role), Ok(profile)) = (role, profile) else {
        return Ok(result.unavailable(format!("/profiles/{alias}")));
      };
      if role.reference() != &selected.role
        || profile.reference() != &selected.profile
        || profile.role() != &selected.role
      {
        return Ok(result.unavailable(format!("/profiles/{alias}")));
      }
      let build = match &selected.build {
        Some(build) => build.clone(),
        None => match build::for_profile(&choices, &profile, node) {
          Some(build) => build,
          None => return Ok(result.unavailable(format!("/profiles/{alias}/build"))),
        },
      };
      if !delivery {
        node.context_binding = Some(
          p.context
            .get(&alias)
            .ok_or_else(|| FactoryValidationDiagnostics::invalid("/context"))?
            .bind(query.project_id)
            .map_err(|_| FactoryValidationDiagnostics::invalid("/context"))?,
        );
      }
      node.permissions = profile.definition().permissions.clone();
      node.budget = profile.definition().budget;
      node.build_profile = None;
      node.profile_selection = Some(FlowProfileSelectionBinding {
        role: selected.role.clone(),
        candidates: [("selected".parse().unwrap(), FlowProfileCandidate { profile, build })].into(),
        policy: FlowProfileSelectionPolicy::Manual {
          candidate: Some("selected".parse().unwrap()),
        },
        failure: FlowProfileSelectionFailure::ManualSelection,
        selection_budget: BudgetUsage::default(),
      });
    }
    let (template, children) = effects::compose(template, p.effects, p.flow_id.unwrap())?;
    let published = template
      .instantiate(p.flow_id.unwrap(), FlowDefinitionVersion::INITIAL, &command_bindings)
      .map_err(|_| FactoryValidationDiagnostics::invalid("/profiles"))?;
    let root = &match release.name {
      Some(name) => published
        .definition
        .with_name(Some(FactoryText::new(name).map_err(|_| ApplicationError::invalid())?))
        .and_then(FlowDefinition::without_code)
        .map_err(|_| ApplicationError::invalid())?,
      None => published.definition,
    };
    let limits = FlowAdmissionLimits::product_defaults(
      template.execution.max_active_nodes(),
      template.execution.budget(),
      template.execution.permissions().clone(),
    )
    .map_err(|_| FactoryValidationDiagnostics::invalid("/execution"))?;
    let mut definitions = vec![root.clone()];
    let mut schemas = published.schemas.clone();
    for child in &children {
      definitions.push(child.definition.clone());
      for schema in &child.schemas {
        if !schemas.iter().any(|s| s.reference() == schema.reference()) {
          schemas.push(schema.clone());
        }
      }
    }
    let flow = FactoryFlowConfiguration::new(
      PinnedFlowDefinitionClosure::new(root.reference(), definitions).map_err(|_| ApplicationError::invalid())?,
      limits.clone(),
      schemas.clone(),
      Default::default(),
    )
    .and_then(|flow| match p.accepted_predecessor.as_ref().filter(|_| delivery) {
      Some(predecessor) => flow.with_accepted_predecessor(predecessor.clone()),
      None => Ok(flow),
    })
    .and_then(|flow| match issue_snapshot {
      Some(input) => flow.with_issue_snapshot(input),
      None => Ok(flow),
    })
    .map_err(|_| FactoryValidationDiagnostics::invalid("/execution"))?;
    crate::factory::issue_snapshot::resolve(
      &flow,
      query.project_id,
      self.issue_properties.as_deref(),
      self.policy.as_ref(),
      context,
    )
    .await?;
    crate::factory::authoring::factory_configuration_choices::validate_flow_choices(
      &flow,
      &choices,
      "/flow",
      super::factory_configuration_choices::FlowChoiceValidation::Authoring,
    )?;
    if crate::factory::sources::factory_stage_source_update::validate_flow_source_updates(
      &flow,
      self.source_actions.as_deref(),
      context,
      query.project_id,
    )
    .await
    .is_err()
    {
      return Ok(result.unavailable("/effects"));
    }
    if let Some(config) = p.configuration {
      let configuration = FactoryConfigurationDraft {
        flow: Some(flow.clone()),
        admission_policy: config.admission_policy.into(),
        wip_limits: config.wip_limits,
        hard_budget: config.hard_budget,
        permission_ceiling: config.permission_ceiling.into(),
        enabled: false,
      };
      let target = FactoryConfiguration::publish(
        p.configuration_id.unwrap(),
        FactoryConfigurationVersion::INITIAL,
        query.project_id,
        FactoryDigest::content_sha256(&serde_json::to_vec(&configuration).map_err(|_| ApplicationError::invalid())?),
        configuration.clone(),
        &choices,
      )
      .map_err(FactoryValidationDiagnostics::from_factory_error)?;
      if let Some(input) = p.input {
        match self
          .resolve_input(
            context,
            query.project_id,
            input,
            &target,
            p.repository.as_ref().expect("required selection"),
          )
          .await
        {
          Ok(input) => {
            if crate::factory::sources::factory_stage_source_update::validate_flow_source_updates_for_input(
              &input.definition,
              target.flow(),
            )
            .is_err()
            {
              return Ok(result.unavailable("/effects"));
            }
            result.input = Some(input);
          }
          Err(_) => {
            result.diagnostics.push(FactoryPresetDependencyProblem {
              path: "/input".into(),
              reason: FactoryPresetDependencyReason::Unavailable,
            });
            return Ok(result);
          }
        }
      }
      result.configuration = Some(configuration);
    }
    let mut draft = effects::ordinary_draft(root, limits, schemas);
    draft.accepted_predecessor = flow.accepted_predecessor;
    draft.issue_snapshot = flow.issue_snapshot;
    result.flow = Some(draft);
    for child in children {
      let execution = child.definition.execution();
      let limits = FlowAdmissionLimits::product_defaults(
        execution.max_active_nodes(),
        execution.budget(),
        execution.permissions().clone(),
      )
      .map_err(|_| ApplicationError::invalid())?;
      result.dependencies.push(FactoryPresetFlowDependency {
        id: child.definition.reference().id(),
        definition: effects::ordinary_draft(&child.definition, limits, child.schemas),
      });
    }
    crate::factory::authoring::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Project,
      query.project_id,
    )
    .await?;
    Ok(result)
  }
}
fn parameter(path: &str, kind: FactoryPresetParameterKind) -> FactoryPresetParameter {
  FactoryPresetParameter {
    path: path.into(),
    kind,
    required: true,
  }
}
fn required(path: String) -> FactoryPresetDependencyProblem {
  FactoryPresetDependencyProblem {
    path,
    reason: FactoryPresetDependencyReason::Required,
  }
}
