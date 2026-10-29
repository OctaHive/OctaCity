//! Exact-subject readiness is a bounded metadata observation, never execution authority.
use crate::*;
use octacity_server_factory::*;
use octacity_server_store::{FactoryConfigurationStore, FactoryRuntimeSettingsStore};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

mod placement;
mod runtime;
mod sources;
pub use placement::FactoryReadinessPlacement;

/// Closed metadata selection; clients cannot submit readiness or credential values.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryReadinessSelection {
  /// Exact independently selected code Repository.
  pub repository: WorkSourceRepositoryRef,
  /// Optional exact retained Source input, independently authorized.
  #[serde(default)]
  pub input: Option<WorkSourceMemberRef>,
}
/// Observes an immutable Configuration/Repository pair without admitting work.
#[derive(Clone, Debug)]
pub struct GetFactoryReadinessQuery {
  /// Exact Configuration identity.
  pub id: FactoryConfigurationId,
  /// Exact retained Configuration version, never an implicit current version.
  pub version: FactoryConfigurationVersion,
  /// Independently reviewed dependencies.
  pub selection: FactoryReadinessSelection,
}
impl Query for GetFactoryReadinessQuery {
  type Outcome = FactoryReadinessReport;
}
impl ManagementAuthorizationTarget for GetFactoryReadinessQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryConfigurationQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::FactoryConfiguration, self.id)
  }
}
/// Missing proof never becomes a ready claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryReadinessState {
  /// All required currently observed facts support this aspect.
  Ready,
  /// A known required prerequisite is absent or incompatible.
  Blocked,
  /// No fresh qualified observation establishes this aspect.
  Unknown,
}
/// A visible declared dependency in the exact frozen closure.
#[derive(Clone, Debug, Serialize)]
pub struct FactoryReadinessSubject {
  /// Exact containing definition; no mutable current alias.
  pub definition: FlowDefinitionRef,
  /// Stable technical node key, independent of business stage names.
  pub node: FactoryKey,
  /// Declared logical dependency alias; never a credential locator.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub dependency: Option<FactoryKey>,
}
/// One safe observation; paths and credentials stay in their original owners.
#[derive(Clone, Debug, Serialize)]
pub struct FactoryReadinessCheck {
  /// Safe declared target within the caller-visible frozen closure.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub subject: Option<FactoryReadinessSubject>,
  /// Project-scoped repair action code; the report's Project is the authorized target.
  pub remediation: String,
  /// Stable prerequisite category, independent of business stage names.
  pub aspect: String,
  /// Safe actionable reason code.
  pub reason: String,
  /// Whether this prerequisite contributes to executable readiness.
  pub required: bool,
  /// Independently observed state.
  pub state: FactoryReadinessState,
  /// Trusted metadata observation time.
  pub observed_at: Timestamp,
  /// Exclusive freshness deadline; this is not an authority lifetime.
  pub expires_at: Timestamp,
}
/// Non-authoritative snapshot of exact retained subjects.
#[derive(Clone, Debug, Serialize)]
pub struct FactoryReadinessReport {
  /// Exact original target, including its retained digest.
  pub configuration: FactoryConfigurationRef,
  /// Exact selected Repository pin, never inferred from a ticket.
  pub repository: WorkSourceRepositoryRef,
  /// Exact frozen root Flow digest/version.
  pub flow: FlowDefinitionRef,
  /// Aggregate required execution prerequisites, independent of delivery claims.
  pub execution: FactoryReadinessState,
  /// Native writes remain unverified until their independent execution owner confirms them.
  pub native_write: FactoryReadinessState,
  /// Trusted query observation time.
  pub observed_at: Timestamp,
  /// Earliest required observation expiry.
  pub expires_at: Timestamp,
  /// Bounded safe prerequisite observations.
  pub checks: Vec<FactoryReadinessCheck>,
}
/// Reuses existing immutable configuration and authorized Repository metadata.
pub struct FactoryReadinessHandlers<S> {
  store: Arc<S>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  clock: Arc<dyn FactoryExecutionClock>,
  maximum_age: Duration,
  artifacts: Option<Arc<dyn IssueArtifactReader>>,
  placement: Option<FactoryReadinessPlacement>,
  sources: Option<sources::SourceReaders>,
  source_actions: Option<Arc<dyn FactoryStageSourceAuthorizer>>,
  repositories: Option<Arc<dyn AuthorizedManagementQueryHandler<GetRepositoryQuery, Error = ApplicationError>>>,
}
impl<S> FactoryReadinessHandlers<S> {
  /// Freshness is supplied by server policy; no query can extend it.
  pub fn new(
    store: Arc<S>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    clock: Arc<dyn FactoryExecutionClock>,
    maximum_age: Duration,
  ) -> Result<Self, ApplicationError> {
    if maximum_age.is_zero() || i64::try_from(maximum_age.as_millis()).is_err() {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      store,
      policy,
      clock,
      maximum_age,
      repositories: None,
      artifacts: None,
      placement: None,
      sources: None,
      source_actions: None,
    })
  }
  /// Reuses the existing authorized Source input and current choice queries.
  pub fn with_sources<Q>(mut self, queries: Arc<Q>) -> Self
  where
    Q: AuthorizedManagementQueryHandler<GetWorkSourceConfigurationQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListWorkSourceInputChoicesQuery, Error = ApplicationError>
      + 'static,
  {
    self.sources = Some(sources::SourceReaders::new(queries));
    self
  }
  /// Reuses current independent Source/Reporter scope validation for frozen effects.
  pub fn with_source_actions(mut self, actions: Arc<dyn FactoryStageSourceAuthorizer>) -> Self {
    self.source_actions = Some(actions);
    self
  }
  /// Installs current Agent and ordinary execution metadata readers.
  pub fn with_placement(mut self, placement: FactoryReadinessPlacement) -> Self {
    self.placement = Some(placement);
    self
  }
  /// Reads only retained Artifact metadata; content/object APIs are never called.
  pub fn with_artifacts(mut self, artifacts: Arc<dyn IssueArtifactReader>) -> Self {
    self.artifacts = Some(artifacts);
    self
  }
  /// Installs the existing authorized exact Repository metadata read.
  pub fn with_repositories(
    mut self,
    repositories: Arc<dyn AuthorizedManagementQueryHandler<GetRepositoryQuery, Error = ApplicationError>>,
  ) -> Self {
    self.repositories = Some(repositories);
    self
  }
}
#[async_trait::async_trait]
impl<S: FactoryConfigurationStore + FactoryRuntimeSettingsStore> ManagementQueryUseCase<GetFactoryReadinessQuery>
  for FactoryReadinessHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetFactoryReadinessQuery,
  ) -> Result<FactoryReadinessReport, ApplicationError> {
    use FactoryReadinessState::*;
    let target = self
      .store
      .factory_configuration_version(query.id, query.version)
      .await?
      .configuration;
    let project = target.reference().project_id();
    let visible = |kind, id: String| {
      crate::factory_configuration_choices::require_visible_reference(self.policy.as_ref(), context, kind, id)
    };
    visible(ManagementResourceKind::Project, project.to_string()).await?;
    let repository = query.selection.repository;
    if repository.project_id != project {
      return Err(FactoryValidationDiagnostics::invalid("/repository"));
    }
    let now = self.clock.now()?;
    let end = Timestamp::from_unix_millis(
      now
        .unix_millis()
        .checked_add(i64::try_from(self.maximum_age.as_millis()).map_err(|_| ApplicationError::invalid())?)
        .ok_or_else(ApplicationError::invalid)?,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let mut checks = vec![check(
      "configuration",
      if target.is_enabled() { Ready } else { Blocked },
      if target.is_enabled() { "enabled" } else { "disabled" },
      true,
      now,
      end,
    )];
    let selected = match &self.repositories {
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
    let exact = selected.as_ref().is_some_and(|r| {
      r.project_id == project
        && r.id == repository.id
        && r.version == repository.version
        && r.definition_digest().ok().as_ref() == Some(&repository.definition_digest)
    });
    checks.push(check(
      "repository",
      if exact { Ready } else { Blocked },
      if exact { "exact" } else { "unavailable" },
      true,
      now,
      end,
    ));
    let installation = crate::factory_configuration_choices::visible_factory_choices(
      self.store.as_ref(),
      self.policy.as_ref(),
      context,
      project,
    )
    .await;
    let installed = installation.as_ref().is_ok_and(|choices| {
      let entries = choices.entries();
      let selected = |kind, reference| {
        entries
          .references
          .iter()
          .any(|entry| entry.kind == kind && &entry.reference == reference)
      };
      selected(FactoryChoiceKind::AdmissionPolicy, target.admission_policy())
        && selected(FactoryChoiceKind::PermissionCeiling, target.permission_ceiling())
        && crate::factory_configuration_choices::validate_flow_choices(target.flow(), choices, "/flow").is_ok()
    });
    checks.push(check(
      "installation",
      if installed { Ready } else { Blocked },
      if installed { "compatible" } else { "unavailable" },
      true,
      now,
      end,
    ));
    if observation_count(&target) > factory_configuration_document_bounds().choice_entries {
      checks.push(check("coverage", Unknown, "observation_limit", true, now, end));
    } else {
      if target
        .flow()
        .closure
        .definitions()
        .iter()
        .flat_map(|d| d.nodes())
        .any(|node| node.is_required() && node.build_bindings().next().is_some())
      {
        checks.push(match &self.placement {
          Some(placement) => {
            placement
              .observe(placement::PlacementObservation {
                context,
                policy: self.policy.as_ref(),
                target: &target,
                repository: &repository,
                now,
                end,
                maximum_age: self.maximum_age,
                choices: installation.as_ref().ok(),
              })
              .await
          }
          None => check("agent", Unknown, "qualification_unobserved", true, now, end),
        });
      }
      for definition in target.flow().closure.definitions() {
        for node in definition.nodes().iter().filter(|n| n.is_required()) {
          if let Some(binding) = node.context_binding() {
            for (name, source) in binding.static_sources() {
              let Some(FactoryContextReference::Artifact(artifact)) = source.source else {
                checks.push(at_node(
                  check("context", Unknown, "admission_input", source.required, now, end),
                  definition.reference(),
                  node.key(),
                  Some(name.clone()),
                ));
                continue;
              };
              let allowed = visible(ManagementResourceKind::Artifact, artifact.artifact_id().to_string())
                .await
                .is_ok();
              let metadata = match (&self.artifacts, allowed) {
                (Some(reader), true) => reader
                  .metadata(ReadFactoryArtifact {
                    project_id: project,
                    artifact: artifact.clone(),
                    maximum_bytes: octacity_artifact_store::MAX_ARTIFACT_READ_BYTES,
                    observed_at: now,
                    required_until: now,
                  })
                  .await
                  .ok(),
                _ => None,
              };
              let allowed = visible(ManagementResourceKind::Artifact, artifact.artifact_id().to_string())
                .await
                .is_ok();
              let (state, reason, expires) = match metadata.filter(|_| allowed) {
                Some(m)
                  if m.availability == IssueArtifactAvailability::Expired
                    || m.expires_at.is_some_and(|at| at <= now) =>
                {
                  (Blocked, "expired", end)
                }
                Some(m)
                  if m.availability == IssueArtifactAvailability::Available
                    && m.published_at.is_some_and(|at| at <= now) =>
                {
                  (Ready, "retained", m.expires_at.map_or(end, |at| at.min(end)))
                }
                _ => (Unknown, "unavailable", end),
              };
              checks.push(at_node(
                check("context", state, reason, true, now, expires),
                definition.reference(),
                node.key(),
                Some(name),
              ));
            }
          }
        }
      }
      for definition in target.flow().closure.definitions() {
        for node in definition.nodes().iter().filter(|node| node.is_required()) {
          if let Some(selection) = node.profile_selection() {
            for candidate in selection.candidates.values() {
              let subject = FactoryReadinessSubject {
                definition: definition.reference(),
                node: node.key().clone(),
                dependency: None,
              };
              checks.extend(
                runtime::observe(
                  self.store.as_ref(),
                  project,
                  installation.as_ref().ok(),
                  &candidate.profile,
                  now,
                  end,
                  subject,
                )
                .await,
              );
            }
          }
        }
      }
      for (definition, node, update) in crate::factory_stage_source_update::flow_source_update_bindings(target.flow()) {
        let (state, reason) = match (update, self.source_actions.as_ref()) {
          (Ok(update), Some(authorizer)) => {
            if authorizer
              .validate_source_update(context, project, &update)
              .await
              .is_ok()
            {
              (Ready, "configured")
            } else {
              (Blocked, "authority_unavailable")
            }
          }
          (Err(_), _) => (Blocked, "action_incompatible"),
          (_, None) => (Unknown, "authority_unobserved"),
        };
        checks.push(at_node(
          check("reporter_action", state, reason, true, now, end),
          definition,
          node,
          None,
        ));
      }
      if let Some(input) = &query.selection.input {
        match &self.sources {
          Some(sources) => checks.extend(sources.observe(context, &target, &repository, input, now, end).await),
          None => checks.push(check("source", Unknown, "authority_unobserved", true, now, end)),
        }
      }
    }
    checks.push(check("native_write", Unknown, "unverified", false, now, end));
    visible(ManagementResourceKind::Project, project.to_string()).await?;
    visible(ManagementResourceKind::FactoryConfiguration, query.id.to_string()).await?;
    visible(ManagementResourceKind::Repository, repository.id.to_string()).await?;
    let completed_at = self.clock.now()?;
    for fact in &mut checks {
      if fact.expires_at <= completed_at && fact.state != Unknown {
        fact.state = Unknown;
        fact.reason = "stale".into();
      }
    }
    let expires_at = checks
      .iter()
      .filter(|c| c.required)
      .map(|c| c.expires_at)
      .min()
      .unwrap_or(end);
    Ok(FactoryReadinessReport {
      configuration: target.reference().clone(),
      repository,
      flow: target.flow().closure.root(),
      execution: aggregate(&checks),
      native_write: Unknown,
      observed_at: completed_at,
      expires_at,
      checks,
    })
  }
}
fn check(
  aspect: &str,
  state: FactoryReadinessState,
  reason: &str,
  required: bool,
  observed_at: Timestamp,
  expires_at: Timestamp,
) -> FactoryReadinessCheck {
  FactoryReadinessCheck {
    subject: None,
    remediation: remediation(aspect).into(),
    aspect: aspect.into(),
    reason: reason.into(),
    state,
    required,
    observed_at,
    expires_at,
  }
}
fn aggregate(checks: &[FactoryReadinessCheck]) -> FactoryReadinessState {
  use FactoryReadinessState::*;
  if checks.iter().any(|c| c.required && c.state == Blocked) {
    Blocked
  } else if checks.iter().any(|c| c.required && c.state == Unknown) {
    Unknown
  } else {
    Ready
  }
}

// Reserve global Source/Reporter/action and coverage slots before dependency I/O.
fn observation_count(target: &FactoryConfiguration) -> usize {
  let mut count =
    8usize.saturating_add(crate::factory_stage_source_update::flow_source_update_bindings(target.flow()).count());
  for node in target
    .flow()
    .closure
    .definitions()
    .iter()
    .flat_map(|definition| definition.nodes())
    .filter(|node| node.is_required())
  {
    count = count.saturating_add(usize::from(node.build_bindings().next().is_some()));
    count = count.saturating_add(usize::from(node.action().is_some()));
    if let Some(context) = node.context_binding() {
      count = count.saturating_add(context.static_sources().len());
    }
    if let Some(selection) = node.profile_selection() {
      for candidate in selection.candidates.values() {
        count = count
          .saturating_add(1 + candidate.profile.definition().execution.mcp_servers.len() * runtime::MCP_ASPECTS.len());
      }
    }
  }
  count
}

fn remediation(aspect: &str) -> &'static str {
  match aspect {
    "configuration" => "review_configuration",
    "repository" => "review_repository",
    "installation" => "restore_runtime",
    "agent" => "enroll_compatible_agent",
    "context" => "retain_context",
    "model_authentication" => "review_model_credentials",
    "source" => "review_source",
    "reporter" | "reporter_action" => "review_reporter",
    "native_write" => "verify_native_write",
    "coverage" => "reduce_closure",
    _ => "review_mcp_connection",
  }
}

fn at_node(
  mut fact: FactoryReadinessCheck,
  definition: FlowDefinitionRef,
  node: &FactoryKey,
  dependency: Option<FactoryKey>,
) -> FactoryReadinessCheck {
  fact.subject = Some(FactoryReadinessSubject {
    definition,
    node: node.clone(),
    dependency,
  });
  fact
}
