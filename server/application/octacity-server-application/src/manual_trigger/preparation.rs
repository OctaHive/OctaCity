use std::collections::BTreeMap;

use octacity_server_store::{ParameterResolutionError, PublishedRepository, TriggerKind};
use serde_json::Value;

use super::model::{ManualSourceSelection, ManualTriggerCommand, ManualTriggerContext, ManualTriggerInputError};

pub(super) struct PreparedManualTrigger {
  pub(super) parameters: BTreeMap<String, Value>,
  pub(super) allowed_pools: Vec<octacity_server_domain::PoolId>,
  pub(super) source: ManualSourceSelection,
}

pub(super) fn prepare_validated(
  command: &ManualTriggerCommand,
  context: &ManualTriggerContext,
) -> Result<PreparedManualTrigger, ManualTriggerInputError> {
  Ok(PreparedManualTrigger {
    parameters: context
      .configuration
      .definition
      .parameters
      .resolve(&command.parameters)
      .map_err(map_parameter_error)?,
    allowed_pools: effective_pools(context)?,
    source: select_source(&command.source, &context.repository)?,
  })
}

pub(super) fn validate_context(
  command: &ManualTriggerCommand,
  context: &ManualTriggerContext,
) -> Result<(), ManualTriggerInputError> {
  validate_context_for(command, context, TriggerKind::Manual)
}

pub(super) fn validate_context_for(
  command: &ManualTriggerCommand,
  context: &ManualTriggerContext,
  kind: TriggerKind,
) -> Result<(), ManualTriggerInputError> {
  let definition = &context.configuration.definition;
  if context.configuration.id != command.target.configuration_id
    || context.configuration.version != command.target.configuration_version
    || context.repository.id != definition.repository_id
    || context.repository.version != definition.repository_version
    || context.pipeline.id != definition.pipeline_id
    || context.pipeline.version != definition.pipeline_version
    || context
      .effective_policy
      .sources
      .last()
      .is_none_or(|source| source.project_id != context.configuration.project_id)
  {
    return Err(ManualTriggerInputError::ContextMismatch);
  }
  if !definition.triggers.allowed.contains(&kind) {
    return Err(match kind {
      TriggerKind::Scheduled => ManualTriggerInputError::ScheduledTriggerNotAllowed,
      TriggerKind::Internal => ManualTriggerInputError::InternalTriggerNotAllowed,
      TriggerKind::External => ManualTriggerInputError::ExternalTriggerNotAllowed,
      TriggerKind::Manual => ManualTriggerInputError::ManualTriggerNotAllowed,
    });
  }
  let policy = &context.effective_policy.policy;
  if !policy.repositories.contains(&context.repository.id) {
    return Err(ManualTriggerInputError::RepositoryNotAllowed);
  }
  if !policy.runtimes.contains(&definition.runtime.class) {
    return Err(ManualTriggerInputError::RuntimeNotAllowed);
  }
  if let Some(namespace) = &definition.cache.namespace {
    let namespace_allowed = policy
      .cache
      .namespaces
      .iter()
      .any(|allowed| allowed.as_str() == namespace);
    if !namespace_allowed
      || definition.cache.read && !policy.cache.read
      || definition.cache.write && !policy.cache.write
    {
      return Err(ManualTriggerInputError::CacheNotAllowed);
    }
  }
  if let Some(identity) = &definition.runtime.workload_identity_profile
    && !policy
      .identity_profiles
      .iter()
      .any(|allowed| allowed.as_str() == identity)
  {
    return Err(ManualTriggerInputError::WorkloadIdentityNotAllowed);
  }
  if !artifact_policy_within(definition.artifacts, policy.artifacts) {
    return Err(ManualTriggerInputError::ArtifactPolicyTooBroad);
  }
  Ok(())
}

fn artifact_policy_within(
  requested: octacity_server_domain::ArtifactPolicy,
  allowed: octacity_server_domain::ArtifactPolicy,
) -> bool {
  requested.artifact_count <= allowed.artifact_count
    && requested.artifact_bytes <= allowed.artifact_bytes
    && requested.report_count <= allowed.report_count
    && requested.report_bytes <= allowed.report_bytes
    && requested.single_output_bytes <= allowed.single_output_bytes
}

fn select_source(
  selection: &ManualSourceSelection,
  repository: &PublishedRepository,
) -> Result<ManualSourceSelection, ManualTriggerInputError> {
  match selection {
    ManualSourceSelection::DefaultReference => repository
      .definition
      .selection
      .default_reference
      .clone()
      .map(ManualSourceSelection::Reference)
      .ok_or(ManualTriggerInputError::DefaultReferenceMissing),
    ManualSourceSelection::Reference(reference)
      if repository.definition.selection.allowed_references.contains(reference) =>
    {
      Ok(ManualSourceSelection::Reference(reference.clone()))
    }
    ManualSourceSelection::Reference(_) => Err(ManualTriggerInputError::ReferenceNotAllowed),
    ManualSourceSelection::ExactRevision(revision) if repository.definition.selection.allow_exact_revision => {
      Ok(ManualSourceSelection::ExactRevision(revision.clone()))
    }
    ManualSourceSelection::ExactRevision(_) => Err(ManualTriggerInputError::ExactRevisionNotAllowed),
  }
}

fn map_parameter_error(error: ParameterResolutionError) -> ManualTriggerInputError {
  match error {
    ParameterResolutionError::TooLarge => ManualTriggerInputError::ParametersTooLarge,
    ParameterResolutionError::Unknown(name) => ManualTriggerInputError::UnknownParameter(name),
    ParameterResolutionError::Missing(name) => ManualTriggerInputError::MissingParameter(name),
    ParameterResolutionError::InvalidType(name) => ManualTriggerInputError::InvalidParameterType(name),
    ParameterResolutionError::UnsupportedValue(name) => ManualTriggerInputError::InvalidParameterValue(name),
  }
}

fn effective_pools(
  context: &ManualTriggerContext,
) -> Result<Vec<octacity_server_domain::PoolId>, ManualTriggerInputError> {
  let pools: Vec<_> = context
    .configuration
    .definition
    .allowed_pools
    .intersection(&context.effective_policy.policy.pools)
    .copied()
    .collect();
  if pools.is_empty() {
    Err(ManualTriggerInputError::NoAllowedPool)
  } else {
    Ok(pools)
  }
}
