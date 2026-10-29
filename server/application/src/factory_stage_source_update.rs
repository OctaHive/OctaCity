//! Reviewed stage effects pin installed scope; the original Work supplies the native item.
use crate::ApplicationError;
use octacity_server_factory::{
  WorkSourceBinding, WorkSourceConnectionRef, WorkSourceStatusEffects, WorkSourceStatusMapping,
};
use serde::{Deserialize, Serialize};

/// One immutable per-outcome mapping carried by the ordinary `set_status` action.
/// No native item, credential, URL or card identity can be supplied here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryStageSourceUpdate {
  /// Original installed Source scope; an action cannot redirect an admitted Work.
  pub source: WorkSourceBinding,
  /// Exact installed Reporter/mapping, independently authorized for updates.
  pub reporter: WorkSourceBinding,
  /// Reviewed managed connection and Project access revision.
  pub connection: Option<WorkSourceConnectionRef>,
  /// Literal bounded effects; unrelated fields are preserved.
  pub effects: WorkSourceStatusEffects,
  /// Reviewed installed native ownership, checked again against the selected Reporter.
  /// Absent for legacy actions; never inferred from literal label names.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub ownership_scopes: Option<std::collections::BTreeSet<octacity_server_factory::FactoryKey>>,
}
impl FactoryStageSourceUpdate {
  /// Rechecks the literal ceiling and any frozen independence claim under installed policy.
  pub fn validate_policy(
    &self,
    policy: &octacity_server_factory::WorkSourceReportingPolicy,
  ) -> Result<(), ApplicationError> {
    self.validate()?;
    let actual = policy
      .ownership_for(&self.effects)
      .map_err(|_| ApplicationError::invalid())?;
    if self.ownership_scopes.is_some() && self.ownership_scopes != actual {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
  /// Checks the closed shape without granting provider authority.
  pub fn validate(&self) -> Result<(), ApplicationError> {
    WorkSourceStatusMapping {
      statuses: vec![self.effects.clone()],
    }
    .validate()
    .map_err(|_| ApplicationError::invalid())?;
    if self.source.installation != self.reporter.installation
      || self.source.resource != self.reporter.resource
      || self
        .connection
        .as_ref()
        .is_some_and(|c| c.reporter_configuration.is_none())
    {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }

  /// Checks the exact immutable input scope used by publication and retained Reporter dispatch.
  /// Matching pins do not grant read/write authority or change the original native item.
  pub fn validate_for_input(
    &self,
    input: &octacity_server_factory::WorkSourceInputDraft,
  ) -> Result<(), ApplicationError> {
    self.validate()?;
    if self.source != input.source
      || Some(&self.reporter) != input.reporter.as_ref()
      || self.connection != input.connection
    {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
}

/// Installed source administration validates pins under current Project use authority.
/// Implementations must perform no provider writes or admission during this check.
#[async_trait::async_trait]
pub trait FactoryStageSourceAuthorizer: Send + Sync {
  /// Validates one exact reviewed source/mapping and literal effects.
  async fn validate_source_update(
    &self,
    context: &crate::ManagementRequestContext,
    project: octacity_server_domain::ProjectId,
    update: &FactoryStageSourceUpdate,
  ) -> Result<(), ApplicationError>;
}

pub(crate) async fn validate_flow_source_updates(
  flow: &octacity_server_factory::FactoryFlowConfiguration,
  authorizer: Option<&dyn FactoryStageSourceAuthorizer>,
  context: &crate::ManagementRequestContext,
  project: octacity_server_domain::ProjectId,
) -> Result<(), ApplicationError> {
  for update in flow_source_updates(flow) {
    let update = update?;
    authorizer
      .ok_or_else(ApplicationError::capability_unavailable)?
      .validate_source_update(context, project, &update)
      .await?;
  }
  Ok(())
}

/// Disabled inputs can be retained for repair; activation checks every pinned nested definition.
pub(crate) fn validate_input_source_updates(
  input: &octacity_server_factory::WorkSourceInputDraft,
  flow: &octacity_server_factory::FactoryFlowConfiguration,
) -> Result<(), ApplicationError> {
  if input.enabled {
    validate_flow_source_updates_for_input(input, flow)?;
  }
  Ok(())
}

/// Authoring can check an exact prospective input without enabling it or publishing a target.
pub(crate) fn validate_flow_source_updates_for_input(
  input: &octacity_server_factory::WorkSourceInputDraft,
  flow: &octacity_server_factory::FactoryFlowConfiguration,
) -> Result<(), ApplicationError> {
  for update in flow_source_updates(flow) {
    update?.validate_for_input(input)?;
  }
  Ok(())
}

fn flow_source_updates(
  flow: &octacity_server_factory::FactoryFlowConfiguration,
) -> impl Iterator<Item = Result<FactoryStageSourceUpdate, ApplicationError>> + '_ {
  flow_source_update_bindings(flow).map(|(_, _, update)| update)
}
// The same frozen action parsing is shared by publication and metadata readiness.
pub(crate) fn flow_source_update_bindings(
  flow: &octacity_server_factory::FactoryFlowConfiguration,
) -> impl Iterator<
  Item = (
    octacity_server_factory::FlowDefinitionRef,
    &octacity_server_factory::FactoryKey,
    Result<FactoryStageSourceUpdate, ApplicationError>,
  ),
> + '_ {
  flow.closure.definitions().iter().flat_map(|definition| {
    definition.nodes().iter().filter_map(move |node| {
      let binding = node.action().filter(|binding| {
        binding.action().as_str() == "set_status" && binding.parameters().get("source_update").is_some()
      })?;
      let update = crate::factory_flow_action::StatusParameters::from_binding(binding)
        .and_then(|parameters| parameters.source_update.ok_or_else(ApplicationError::invalid));
      Some((definition.reference(), node.key(), update))
    })
  })
}

/// Rejects two producers for the same native managed field; this grants no writer authority.
/// Callers first bind both effects to the original native item and accepted outcome.
/// A None entry is an opaque legacy producer; an empty list means no direct producer.
pub fn validate_issue_push_producers(
  mapping: &octacity_server_store::PublishedIssueSourceMapping,
  status: octacity_server_domain::IssueStatusId,
  updates: &[Option<FactoryStageSourceUpdate>],
) -> Result<(), ApplicationError> {
  use octacity_server_store::{IssueField, IssueSourceFieldAuthority};
  if !mapping.definition.push_enabled
    || mapping.definition.fields.get(&IssueField::Status) != Some(&IssueSourceFieldAuthority::OctacityOwnedPush)
  {
    return Ok(());
  }
  let Some(reporter) = &mapping.definition.reporter else {
    return Err(ApplicationError::invalid());
  };
  let Some(effects) = mapping
    .definition
    .status
    .as_ref()
    .and_then(|s| s.push.as_ref())
    .and_then(|m| m.effects(&status.to_string()))
  else {
    return Ok(());
  };
  if effects.add.is_empty() && effects.remove.is_empty() && effects.state.is_none() {
    return Ok(());
  }
  for update in updates {
    let Some(update) = update else {
      return Err(crate::FactoryValidationDiagnostics::invalid(
        "/action/source_update/producer",
      ));
    };
    update.validate()?;
    if update.reporter.installation.identity() == reporter.installation.identity()
      && (!update.effects.add.is_empty() || !update.effects.remove.is_empty() || update.effects.state.is_some())
      && ((effects.state.is_some() && update.effects.state.is_some())
        || effects
          .add
          .iter()
          .chain(&effects.remove)
          .any(|label| update.effects.add.contains(label) || update.effects.remove.contains(label))
        || match (&mapping.definition.reporter_ownership, &update.ownership_scopes) {
          (Some(ownership), Some(scopes)) => !ownership.scopes(effects).is_disjoint(scopes),
          _ => true, // Missing installed ownership cannot prove independent producers.
        })
    {
      return Err(crate::FactoryValidationDiagnostics::invalid(
        "/action/source_update/producer",
      ));
    }
  }
  Ok(())
}
