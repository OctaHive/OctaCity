use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{AttemptNumber, BuildId, Timestamp};
use octacity_server_store::{
  AcceptTrigger, ImmutableBuildInput, SuppressTrigger, TriggerAcceptanceProbe, TriggerAcceptanceStore,
  TriggerCausality, TriggerCause, TriggerEvaluationOutcome, TriggerEventKind, TriggerIntentDigest, TriggerMetadata,
};
use serde::Serialize;

use super::{
  materialization::{
    ManualBuildIdentities, classify_error, effective_policy_snapshot, input_snapshot, materialize_jobs,
    retention_deadlines,
  },
  model::{
    AcceptManualTriggerCommand, ManualTriggerCommand, ManualTriggerError, ManualTriggerOutcome,
    RevisionResolutionRequest,
  },
  ports::{ManualTriggerContextProvider, RevisionResolver},
  preparation::{prepare_validated, validate_context, validate_context_for},
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
    self
      .accept_root(command, accepted_at, octacity_server_store::TriggerKind::Manual)
      .await
  }

  pub(super) async fn resolve_manual_revision(
    &self,
    command: &ManualTriggerCommand,
  ) -> Result<Option<octacity_server_domain::ImmutableRevision>, ManualTriggerError> {
    let context = self
      .context
      .load_for(
        command.trigger,
        command.target,
        octacity_server_store::TriggerKind::Manual,
      )
      .await
      .map_err(ManualTriggerError::Context)?;
    validate_context(command, &context).map_err(ManualTriggerError::Invalid)?;
    if !context.configuration.definition.enabled {
      return Ok(None);
    }
    let prepared = prepare_validated(command, &context).map_err(ManualTriggerError::Invalid)?;
    self
      .revisions
      .resolve(RevisionResolutionRequest {
        repository: context.repository,
        selection: prepared.source,
      })
      .await
      .map(Some)
      .map_err(ManualTriggerError::Revision)
  }

  pub(super) async fn accept_with_resolved_revision(
    &self,
    command: ManualTriggerCommand,
    accepted_at: Timestamp,
    resolved_revision: Option<octacity_server_domain::ImmutableRevision>,
  ) -> Result<ManualTriggerOutcome, ManualTriggerError> {
    self
      .accept_root_with_revision(
        command,
        accepted_at,
        octacity_server_store::TriggerKind::Manual,
        RevisionMode::Resolved(resolved_revision),
      )
      .await
  }

  /// Evaluates one durable scheduled occurrence through the same Build transaction.
  pub async fn accept_scheduled(
    &self,
    command: ManualTriggerCommand,
    accepted_at: Timestamp,
  ) -> Result<ManualTriggerOutcome, ManualTriggerError> {
    self
      .accept_root(command, accepted_at, octacity_server_store::TriggerKind::Scheduled)
      .await
  }

  /// Evaluates one durable server-generated event through the shared Build transaction.
  pub async fn accept_internal(
    &self,
    command: ManualTriggerCommand,
    accepted_at: Timestamp,
    source_build_id: BuildId,
    event_kind: TriggerEventKind,
    causality: TriggerCausality,
  ) -> Result<ManualTriggerOutcome, ManualTriggerError> {
    let occurrence_id = ManualBuildIdentities::occurrence_id_for("internal", &command);
    let trigger = octacity_server_store::NormalizedTriggerOccurrence::derived(
      occurrence_id,
      command.trigger,
      command.target,
      command.deduplication_identity.clone(),
      TriggerCause::Internal {
        source_build_id,
        event_kind,
      },
      causality,
      command.observed_at,
    )
    .map_err(|_| ManualTriggerError::Invalid(super::model::ManualTriggerInputError::ContextMismatch))?;
    let intent_digest = derived_trigger_intent_digest(&command, &trigger)?;
    self
      .evaluate_occurrence(
        command,
        accepted_at,
        octacity_server_store::TriggerKind::Internal,
        trigger,
        intent_digest,
        RevisionMode::Resolve,
      )
      .await
  }

  /// Evaluates one already-authenticated external delivery through the shared Build transaction.
  pub async fn accept_external(
    &self,
    command: ManualTriggerCommand,
    accepted_at: Timestamp,
    cause: TriggerCause,
    provider_metadata: TriggerMetadata,
  ) -> Result<ManualTriggerOutcome, ManualTriggerError> {
    if !matches!(cause, TriggerCause::External { .. }) {
      return Err(ManualTriggerError::Invalid(
        super::model::ManualTriggerInputError::ContextMismatch,
      ));
    }
    let occurrence_id = ManualBuildIdentities::occurrence_id_for("external", &command);
    let trigger = octacity_server_store::NormalizedTriggerOccurrence::root(
      occurrence_id,
      command.trigger,
      command.target,
      command.deduplication_identity.clone(),
      cause,
      provider_metadata,
      command.observed_at,
    )
    .map_err(|_| ManualTriggerError::Invalid(super::model::ManualTriggerInputError::ContextMismatch))?;
    let intent_digest = derived_trigger_intent_digest(&command, &trigger)?;
    self
      .evaluate_occurrence(
        command,
        accepted_at,
        octacity_server_store::TriggerKind::External,
        trigger,
        intent_digest,
        RevisionMode::Resolve,
      )
      .await
  }

  async fn accept_root(
    &self,
    command: ManualTriggerCommand,
    accepted_at: Timestamp,
    kind: octacity_server_store::TriggerKind,
  ) -> Result<ManualTriggerOutcome, ManualTriggerError> {
    self
      .accept_root_with_revision(command, accepted_at, kind, RevisionMode::Resolve)
      .await
  }

  async fn accept_root_with_revision(
    &self,
    command: ManualTriggerCommand,
    accepted_at: Timestamp,
    kind: octacity_server_store::TriggerKind,
    revision_mode: RevisionMode,
  ) -> Result<ManualTriggerOutcome, ManualTriggerError> {
    let occurrence_id = match kind {
      octacity_server_store::TriggerKind::Manual => ManualBuildIdentities::occurrence_id(&command),
      octacity_server_store::TriggerKind::Scheduled => ManualBuildIdentities::occurrence_id_for("scheduled", &command),
      _ => {
        return Err(ManualTriggerError::Invalid(
          super::model::ManualTriggerInputError::ContextMismatch,
        ));
      }
    };
    let cause = match kind {
      octacity_server_store::TriggerKind::Manual => TriggerCause::Manual {},
      octacity_server_store::TriggerKind::Scheduled => TriggerCause::Scheduled {},
      _ => unreachable!("unsupported Build trigger kind was rejected"),
    };
    let trigger = octacity_server_store::NormalizedTriggerOccurrence::root(
      occurrence_id,
      command.trigger,
      command.target,
      command.deduplication_identity.clone(),
      cause,
      TriggerMetadata::default(),
      command.observed_at,
    )
    .map_err(|_| ManualTriggerError::Invalid(super::model::ManualTriggerInputError::ContextMismatch))?;
    let intent_digest = trigger_intent_digest(&command, kind)?;
    self
      .evaluate_occurrence(command, accepted_at, kind, trigger, intent_digest, revision_mode)
      .await
  }

  async fn evaluate_occurrence(
    &self,
    command: ManualTriggerCommand,
    accepted_at: Timestamp,
    kind: octacity_server_store::TriggerKind,
    trigger: octacity_server_store::NormalizedTriggerOccurrence,
    intent_digest: TriggerIntentDigest,
    revision_mode: RevisionMode,
  ) -> Result<ManualTriggerOutcome, ManualTriggerError> {
    let occurrence_id = trigger.id;
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
      .load_for(command.trigger, command.target, kind)
      .await
      .map_err(ManualTriggerError::Context)?;
    if kind == octacity_server_store::TriggerKind::Manual {
      validate_context(&command, &context).map_err(ManualTriggerError::Invalid)?;
    } else {
      validate_context_for(&command, &context, kind).map_err(ManualTriggerError::Invalid)?;
    }
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
    let immutable_revision = match revision_mode {
      RevisionMode::Resolve => self
        .revisions
        .resolve(RevisionResolutionRequest {
          repository: context.repository.clone(),
          selection: prepared.source.clone(),
        })
        .await
        .map_err(ManualTriggerError::Revision)?,
      RevisionMode::Resolved(Some(revision)) => revision,
      RevisionMode::Resolved(None) => {
        return Err(ManualTriggerError::Invalid(
          super::model::ManualTriggerInputError::ContextMismatch,
        ));
      }
    };
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
      retention: retention_deadlines(&context, accepted_at)?,
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

enum RevisionMode {
  Resolve,
  Resolved(Option<octacity_server_domain::ImmutableRevision>),
}

pub(crate) fn manual_trigger_identity(
  command: &ManualTriggerCommand,
) -> Result<(octacity_server_domain::TriggerOccurrenceId, TriggerIntentDigest), ManualTriggerError> {
  Ok((
    ManualBuildIdentities::occurrence_id(command),
    trigger_intent_digest(command, octacity_server_store::TriggerKind::Manual)?,
  ))
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
  kind: octacity_server_store::TriggerKind,
  trigger: octacity_server_store::TriggerDefinitionRef,
  target: octacity_server_store::TriggerTarget,
  deduplication_identity: &'a octacity_server_domain::TriggerIdentity,
  source: &'a super::model::ManualSourceSelection,
  parameters: &'a std::collections::BTreeMap<String, serde_json::Value>,
  priority: i64,
  source_time: Option<Timestamp>,
}

fn trigger_intent_digest(
  command: &ManualTriggerCommand,
  kind: octacity_server_store::TriggerKind,
) -> Result<TriggerIntentDigest, ManualTriggerError> {
  let intent = ManualTriggerIntent {
    kind,
    trigger: command.trigger,
    target: command.target,
    deduplication_identity: &command.deduplication_identity,
    source: &command.source,
    parameters: &command.parameters,
    priority: command.priority,
    source_time: (kind == octacity_server_store::TriggerKind::Scheduled).then_some(command.observed_at),
  };
  TriggerIntentDigest::derive(&intent).map_err(|_| ManualTriggerError::SnapshotEncoding)
}

#[derive(Serialize)]
struct DerivedTriggerIntent<'a> {
  occurrence: octacity_server_store::TriggerOccurrenceIntent,
  source: &'a super::model::ManualSourceSelection,
  parameters: &'a std::collections::BTreeMap<String, serde_json::Value>,
  priority: i64,
}

fn derived_trigger_intent_digest(
  command: &ManualTriggerCommand,
  occurrence: &octacity_server_store::NormalizedTriggerOccurrence,
) -> Result<TriggerIntentDigest, ManualTriggerError> {
  TriggerIntentDigest::derive(&DerivedTriggerIntent {
    occurrence: occurrence.intent(),
    source: &command.source,
    parameters: &command.parameters,
    priority: command.priority,
  })
  .map_err(|_| ManualTriggerError::SnapshotEncoding)
}
