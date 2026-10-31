//! Release presets resolve through ordinary Factory authoring; queries grant no execution authority.
use crate::*;
mod build;
mod contracts;
mod effects;
mod input;
mod release;
pub use contracts::*;
use octacity_server_factory::*;
use octacity_server_store::FactoryConfigurationStore;
pub use octacity_server_store::MAX_SOURCE_BODY_BYTES;
use release::released_triage;
use std::sync::Arc;

/// Shared read-only setup owner; publication remains in the ordinary versioned handlers.
pub struct FactoryPresetHandlers<S> {
  store: Arc<S>,
  source_actions: Option<Arc<dyn FactoryStageSourceAuthorizer>>,
  sources: Option<Arc<dyn AuthorizedManagementQueryHandler<ListWorkSourceInputChoicesQuery, Error = ApplicationError>>>,
  artifacts: Option<Arc<dyn IssueArtifactReader>>,
  repositories: Option<Arc<dyn AuthorizedManagementQueryHandler<GetRepositoryQuery, Error = ApplicationError>>>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
}
impl<S> FactoryPresetHandlers<S> {
  /// Uses the existing Project catalogs and management authority, without provider services.
  pub fn new(store: Arc<S>, policy: Arc<dyn ManagementAuthorizationPolicy>) -> Self {
    Self {
      store,
      policy,
      repositories: None,
      artifacts: None,
      sources: None,
      source_actions: None,
    }
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
    crate::factory_role_profiles::require_project_visibility(grant, query.page.project_id())?;
    self
      .store
      .factory_configuration_availability(query.page.project_id())
      .await?;
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Project,
      query.page.project_id(),
    )
    .await?;
    let (template, reference) = released_triage()?;
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
        parameters.push(parameter(
          &format!("/profiles/{alias}"),
          FactoryPresetParameterKind::RoleProfile,
        ));
        parameters.push(parameter(
          &format!("/context/{alias}"),
          FactoryPresetParameterKind::ProjectContext,
        ));
      }
    }
    let summary = FactoryPresetSummary {
      parameters,
      required_node_kinds,
      reference,
      profile_aliases: template
        .nodes
        .iter()
        .filter_map(|node| node.build_profile.clone())
        .collect(),
    };
    let items = if query
      .page
      .after()
      .is_none_or(|after| summary.reference.identity() > after)
    {
      vec![summary]
    } else {
      vec![]
    };
    Ok(FactoryPresetPage {
      total: 1,
      items,
      next_cursor: None,
    })
  }
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
    crate::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
    self.store.factory_configuration_availability(query.project_id).await?;
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Project,
      query.project_id,
    )
    .await?;
    let (mut template, preset) = released_triage()?;
    if preset != query.selection.preset {
      return Err(FactoryValidationDiagnostics::invalid("/preset"));
    }
    let mut result = FactoryPresetResolution {
      preset,
      completion: SourceCompletionDocument::review_preset(query.project_id, TRIAGE_COMPLETION_PRESET)
        .map_err(|_| ApplicationError::invalid())?,
      diagnostics: vec![],
      flow: None,
      configuration: None,
      input: None,
      dependencies: vec![],
    };
    let p = query.selection;
    let aliases: std::collections::BTreeSet<_> = template
      .nodes
      .iter()
      .filter_map(|node| node.build_profile.clone())
      .collect();
    if p.profiles.keys().any(|key| !aliases.contains(key)) || p.context.keys().any(|key| !aliases.contains(key)) {
      return Err(FactoryValidationDiagnostics::invalid("/profiles"));
    }
    for alias in &aliases {
      if !p.profiles.contains_key(alias) {
        result.diagnostics.push(required(format!("/profiles/{alias}")));
      }
    }
    for (missing, path) in [
      (p.repository.is_none(), "/repository"),
      (p.context.is_empty(), "/context"),
      (p.flow_id.is_none(), "/flow_id"),
      (p.configuration_id.is_none(), "/configuration_id"),
      (p.execution.is_none(), "/execution"),
      (p.configuration.is_none(), "/configuration"),
    ] {
      if missing {
        result.diagnostics.push(required(path.into()));
      }
    }
    if !result.diagnostics.is_empty() {
      return Ok(result);
    }
    let repository = p.repository.as_ref().unwrap();
    if repository.project_id != query.project_id {
      return Err(FactoryValidationDiagnostics::invalid("/repository"));
    }
    if crate::factory_configuration_choices::require_visible_reference(
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
    let now = SystemFactoryExecutionClock.now()?;
    for alias in &aliases {
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
        let visible = crate::factory_configuration_choices::require_visible_reference(
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
        let still_visible = crate::factory_configuration_choices::require_visible_reference(
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
    let choices = match crate::factory_configuration_choices::visible_factory_choices(
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
    template.execution = p.execution.unwrap();
    for node in &mut template.nodes {
      let Some(alias) = node.build_profile.clone() else {
        continue;
      };
      let selected = &p.profiles[&alias];
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
      node.context_binding = Some(
        p.context
          .get(&alias)
          .ok_or_else(|| FactoryValidationDiagnostics::invalid("/context"))?
          .bind(query.project_id)
          .map_err(|_| FactoryValidationDiagnostics::invalid("/context"))?,
      );
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
      .instantiate(p.flow_id.unwrap(), FlowDefinitionVersion::INITIAL, &Default::default())
      .map_err(|_| FactoryValidationDiagnostics::invalid("/profiles"))?;
    let root = &published.definition;
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
    .map_err(|_| FactoryValidationDiagnostics::invalid("/execution"))?;
    crate::factory_configuration_choices::validate_flow_choices(&flow, &choices, "/flow")?;
    if crate::factory_stage_source_update::validate_flow_source_updates(
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
    let config = p.configuration.unwrap();
    let configuration = FactoryConfigurationDraft {
      flow: Some(flow),
      admission_policy: config.admission_policy,
      wip_limits: config.wip_limits,
      hard_budget: config.hard_budget,
      permission_ceiling: config.permission_ceiling,
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
        .resolve_input(context, query.project_id, input, &target, repository)
        .await
      {
        Ok(input) => {
          if crate::factory_stage_source_update::validate_flow_source_updates_for_input(
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
    result.flow = Some(effects::ordinary_draft(root, limits, schemas));
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
    result.configuration = Some(configuration);
    crate::factory_configuration_choices::require_visible_reference(
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
