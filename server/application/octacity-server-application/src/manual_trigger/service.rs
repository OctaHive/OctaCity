use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{AttemptNumber, Timestamp};
use octacity_server_store::{
  AcceptTrigger, ImmutableBuildInput, SuppressTrigger, TriggerAcceptanceProbe, TriggerAcceptanceStore, TriggerCause,
  TriggerEvaluationOutcome, TriggerIntentDigest, TriggerMetadata,
};
use serde::Serialize;

use super::{
  materialization::{
    ManualBuildIdentities, classify_error, effective_policy_snapshot, input_snapshot, materialize_jobs,
  },
  model::{
    AcceptManualTriggerCommand, ManualTriggerCommand, ManualTriggerError, ManualTriggerOutcome,
    RevisionResolutionRequest,
  },
  ports::{ManualTriggerContextProvider, RevisionResolver},
  preparation::{prepare_validated, validate_context},
};
use crate::{CommandHandler, CommandTransaction};

/// Application service that resolves source state and commits one complete initial Build graph.
pub struct ManualTriggerService {
  store: Arc<dyn TriggerAcceptanceStore>,
  context: Arc<dyn ManualTriggerContextProvider>,
  revisions: Arc<dyn RevisionResolver>,
}

impl ManualTriggerService {
  /// Creates the service from backend-neutral authoritative, context, and VCS ports.
  pub fn new(
    store: Arc<dyn TriggerAcceptanceStore>,
    context: Arc<dyn ManualTriggerContextProvider>,
    revisions: Arc<dyn RevisionResolver>,
  ) -> Self {
    Self {
      store,
      context,
      revisions,
    }
  }

  /// Accepts one manual Trigger and commits its complete first Attempt atomically.
  pub async fn accept(
    &self,
    command: ManualTriggerCommand,
    accepted_at: Timestamp,
  ) -> Result<ManualTriggerOutcome, ManualTriggerError> {
    let occurrence_id = ManualBuildIdentities::occurrence_id(&command);
    let trigger = octacity_server_store::NormalizedTriggerOccurrence::root(
      occurrence_id,
      command.trigger,
      command.target,
      command.deduplication_identity.clone(),
      TriggerCause::Manual {},
      TriggerMetadata::default(),
      command.observed_at,
    )
    .map_err(|_| ManualTriggerError::Invalid(super::model::ManualTriggerInputError::ContextMismatch))?;
    let intent_digest = manual_intent_digest(&command)?;
    if let Some(outcome) = self
      .store
      .replay_trigger_acceptance(TriggerAcceptanceProbe {
        trigger: trigger.clone(),
        intent_digest,
      })
      .await
      .map_err(ManualTriggerError::Store)?
    {
      return Ok(outcome.into());
    }
    let context = self
      .context
      .load(command.trigger, command.target)
      .await
      .map_err(ManualTriggerError::Context)?;
    validate_context(&command, &context).map_err(ManualTriggerError::Invalid)?;
    if !context.configuration.definition.enabled {
      return self
        .store
        .suppress_trigger(SuppressTrigger::new(trigger, intent_digest, accepted_at).map_err(classify_error)?)
        .await
        .map(TriggerEvaluationOutcome::Suppressed)
        .map(Into::into)
        .map_err(ManualTriggerError::Store);
    }
    let prepared = prepare_validated(&command, &context).map_err(ManualTriggerError::Invalid)?;
    let identities = ManualBuildIdentities::derive(occurrence_id, &context.pipeline);
    let immutable_revision = self
      .revisions
      .resolve(RevisionResolutionRequest {
        repository: context.repository.clone(),
        selection: prepared.source.clone(),
      })
      .await
      .map_err(ManualTriggerError::Revision)?;
    let jobs = materialize_jobs(&context, &identities, &prepared, &immutable_revision)?;
    let build = ImmutableBuildInput {
      id: identities.build_id,
      project_id: context.configuration.project_id,
      configuration_id: context.configuration.id,
      configuration_version: context.configuration.version,
      pipeline_id: context.pipeline.id,
      pipeline_version: context.pipeline.version,
      repository_id: context.repository.id,
      repository_version: context.repository.version,
      immutable_revision,
      input_snapshot: input_snapshot(&prepared)?,
      effective_policy_snapshot: effective_policy_snapshot(&context)?,
      project_job_concurrency_limit: context.effective_policy.policy.concurrency.active_jobs,
      priority: command.priority,
    };
    let request = AcceptTrigger::new(
      trigger,
      build,
      identities.attempt_id,
      AttemptNumber::FIRST,
      jobs,
      intent_digest,
      accepted_at,
    )
    .map_err(classify_error)?;
    self
      .store
      .accept_trigger(request)
      .await
      .map(TriggerEvaluationOutcome::Accepted)
      .map(Into::into)
      .map_err(ManualTriggerError::Store)
  }
}

#[async_trait]
impl CommandTransaction<AcceptManualTriggerCommand> for ManualTriggerService {
  type Error = ManualTriggerError;

  async fn commit_command(&self, command: AcceptManualTriggerCommand) -> Result<ManualTriggerOutcome, Self::Error> {
    self.accept(command.trigger, command.accepted_at).await
  }
}

#[async_trait]
impl CommandHandler<AcceptManualTriggerCommand> for ManualTriggerService {
  type Error = ManualTriggerError;

  async fn handle_command(&self, command: AcceptManualTriggerCommand) -> Result<ManualTriggerOutcome, Self::Error> {
    self.commit_command(command).await
  }
}

#[derive(Serialize)]
struct ManualTriggerIntent<'a> {
  trigger: octacity_server_store::TriggerDefinitionRef,
  target: octacity_server_store::TriggerTarget,
  deduplication_identity: &'a octacity_server_domain::TriggerIdentity,
  source: &'a super::model::ManualSourceSelection,
  parameters: &'a std::collections::BTreeMap<String, serde_json::Value>,
  priority: i64,
}

fn manual_intent_digest(command: &ManualTriggerCommand) -> Result<TriggerIntentDigest, ManualTriggerError> {
  let intent = ManualTriggerIntent {
    trigger: command.trigger,
    target: command.target,
    deduplication_identity: &command.deduplication_identity,
    source: &command.source,
    parameters: &command.parameters,
    priority: command.priority,
  };
  TriggerIntentDigest::derive(&intent).map_err(|_| ManualTriggerError::SnapshotEncoding)
}
