use std::sync::Arc;

use octacity_server_domain::{Timestamp, TriggerIdentity};
use octacity_server_store::{
  ClaimInternalTriggerEvents, CompleteInternalTriggerEvent, InternalTriggerEventStore, WorkerOwner,
};
use thiserror::Error;

use crate::{
  InternalTriggerDefinition, InternalTriggerSourceStrategy, ManualSourceSelection, ManualTriggerCommand,
  ManualTriggerError, ManualTriggerService,
};

/// Counts produced by one bounded internal-Trigger outbox pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InternalTriggerBatchOutcome {
  /// Transactional-outbox events durably claimed by this replica.
  pub claimed_events: usize,
  /// Matching occurrences accepted, replayed, or intentionally suppressed.
  pub evaluated_occurrences: usize,
  /// Candidate derivations rejected by cycle or depth protection.
  pub protected_occurrences: usize,
}

/// Restart-safe worker delivering terminal Build events to the Trigger Engine.
pub struct InternalTriggerWorker<S> {
  store: Arc<S>,
  triggers: Arc<ManualTriggerService>,
}

impl<S> InternalTriggerWorker<S>
where
  S: InternalTriggerEventStore,
{
  /// Creates a worker from the durable outbox and shared Build evaluator.
  pub fn new(store: Arc<S>, triggers: Arc<ManualTriggerService>) -> Self {
    Self { store, triggers }
  }

  /// Claims and delivers one bounded batch at an explicit authoritative time.
  pub async fn run_once(
    &self,
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<InternalTriggerBatchOutcome, InternalTriggerWorkerError> {
    let claims = self
      .store
      .claim_internal_trigger_events(ClaimInternalTriggerEvents::new(
        owner,
        observed_at,
        claim_expires_at,
        limit,
      )?)
      .await?;
    let mut outcome = InternalTriggerBatchOutcome {
      claimed_events: claims.len(),
      ..InternalTriggerBatchOutcome::default()
    };
    for claim in claims {
      for matched in &claim.matches {
        let causality = match octacity_server_trigger::derive_internal_causality(
          &claim.source_occurrence,
          matched.trigger,
          &claim.trigger_ancestry,
        ) {
          Ok(causality) => causality,
          Err(_) => {
            outcome.protected_occurrences += 1;
            continue;
          }
        };
        let definition: InternalTriggerDefinition = serde_json::from_value(matched.definition.clone())
          .map_err(|_| InternalTriggerWorkerError::InvalidPersistedDefinition)?;
        if definition.event_kind != claim.event_kind {
          return Err(InternalTriggerWorkerError::InvalidPersistedDefinition);
        }
        let deduplication_identity = TriggerIdentity::new(format!("internal:{}", claim.event_identity))
          .map_err(|_| InternalTriggerWorkerError::InvalidPersistedDefinition)?;
        self
          .triggers
          .accept_internal(
            ManualTriggerCommand {
              trigger: matched.trigger,
              target: matched.target,
              deduplication_identity,
              source: downstream_source(definition.source, &claim.source_revision),
              parameters: definition.parameters,
              priority: definition.priority,
              observed_at: claim.occurred_at,
            },
            observed_at,
            claim.source_build_id,
            claim.event_kind.into(),
            causality,
          )
          .await?;
        outcome.evaluated_occurrences += 1;
      }
      self
        .store
        .complete_internal_trigger_event(CompleteInternalTriggerEvent {
          event_identity: claim.event_identity,
          owner: claim.owner,
          completed_at: observed_at,
        })
        .await?;
    }
    Ok(outcome)
  }
}

fn downstream_source(
  strategy: InternalTriggerSourceStrategy,
  upstream_revision: &octacity_server_domain::ImmutableRevision,
) -> ManualSourceSelection {
  match strategy {
    InternalTriggerSourceStrategy::InheritRevision => ManualSourceSelection::ExactRevision(upstream_revision.clone()),
    InternalTriggerSourceStrategy::ResolveTarget(source) => source,
  }
}

/// Failure from one bounded internal-Trigger outbox pass.
#[derive(Debug, Error)]
pub enum InternalTriggerWorkerError {
  /// Durable outbox work could not be claimed or completed.
  #[error("internal trigger outbox store failed")]
  Store(#[from] octacity_server_store::StoreError),
  /// Persisted kind-specific definition violates its strict typed contract.
  #[error("persisted internal trigger definition is invalid")]
  InvalidPersistedDefinition,
  /// One occurrence could not be evaluated into a Build.
  #[error("internal trigger evaluation failed")]
  Trigger(#[from] ManualTriggerError),
}

#[cfg(test)]
mod tests {
  use std::{
    collections::BTreeMap,
    fmt::Debug,
    future::Future,
    str::FromStr,
    sync::Mutex,
    task::{Context, Poll, Waker},
  };

  use async_trait::async_trait;
  use octacity_server_domain::{
    BuildConfigurationId, BuildConfigurationVersion, ImmutableRevision, TriggerId, TriggerOccurrenceId, TriggerVersion,
  };
  use octacity_server_store::{
    InternalTriggerEventClaim, InternalTriggerMatch, MutationDisposition, NormalizedTriggerOccurrence, StoreError,
    TerminalBuildEvent, TriggerCause, TriggerDefinitionRef, TriggerMetadata, TriggerTarget,
  };

  use super::*;
  use crate::{
    ManualTriggerContext, ManualTriggerContextError, ManualTriggerContextProvider, RevisionResolutionError,
    RevisionResolutionRequest, RevisionResolver,
  };

  #[test]
  fn a_causal_loop_is_completed_without_evaluating_another_build() {
    run_ready(async {
      let trigger = definition(1);
      let source_occurrence = root_occurrence(trigger);
      let store = Arc::new(FakeOutboxStore {
        claims: Mutex::new(vec![InternalTriggerEventClaim {
          event_identity: TriggerIdentity::new("00000000-0000-0000-0000-000000000099").unwrap(),
          source_build_id: id(2),
          source_revision: ImmutableRevision::new("abc").unwrap(),
          source_target: target(),
          source_occurrence: source_occurrence.clone(),
          trigger_ancestry: vec![trigger],
          event_kind: TerminalBuildEvent::Succeeded,
          occurred_at: time(10),
          matches: vec![InternalTriggerMatch {
            trigger,
            target: target(),
            definition: serde_json::to_value(InternalTriggerDefinition {
              upstream: target(),
              event_kind: TerminalBuildEvent::Succeeded,
              source: InternalTriggerSourceStrategy::InheritRevision,
              parameters: BTreeMap::new(),
              priority: 0,
            })
            .unwrap(),
          }],
          owner: WorkerOwner::new("worker:test").unwrap(),
          claim_expires_at: time(30),
        }]),
        completed: Mutex::new(0),
      });
      let triggers = Arc::new(ManualTriggerService::new(
        Arc::new(NeverTriggerStore),
        Arc::new(NeverContext),
        Arc::new(NeverResolver),
      ));
      let outcome = InternalTriggerWorker::new(store.clone(), triggers)
        .run_once(WorkerOwner::new("worker:test").unwrap(), time(20), time(30), 1)
        .await
        .unwrap();

      assert_eq!(outcome.claimed_events, 1);
      assert_eq!(outcome.evaluated_occurrences, 0);
      assert_eq!(outcome.protected_occurrences, 1);
      assert_eq!(*store.completed.lock().unwrap(), 1);
    });
  }

  #[test]
  fn inherited_source_uses_the_upstream_immutable_revision() {
    let revision = ImmutableRevision::new("upstream-commit").unwrap();
    assert_eq!(
      downstream_source(InternalTriggerSourceStrategy::InheritRevision, &revision),
      ManualSourceSelection::ExactRevision(revision)
    );
  }

  struct FakeOutboxStore {
    claims: Mutex<Vec<InternalTriggerEventClaim>>,
    completed: Mutex<usize>,
  }

  #[async_trait]
  impl InternalTriggerEventStore for FakeOutboxStore {
    async fn claim_internal_trigger_events(
      &self,
      _request: ClaimInternalTriggerEvents,
    ) -> Result<Vec<InternalTriggerEventClaim>, StoreError> {
      Ok(std::mem::take(&mut *self.claims.lock().unwrap()))
    }

    async fn complete_internal_trigger_event(
      &self,
      _request: CompleteInternalTriggerEvent,
    ) -> Result<MutationDisposition, StoreError> {
      *self.completed.lock().unwrap() += 1;
      Ok(MutationDisposition::Applied)
    }
  }

  struct NeverTriggerStore;

  #[async_trait]
  impl octacity_server_store::TriggerAcceptanceStore for NeverTriggerStore {
    async fn replay_trigger_acceptance(
      &self,
      _request: octacity_server_store::TriggerAcceptanceProbe,
    ) -> Result<Option<octacity_server_store::TriggerEvaluationOutcome>, StoreError> {
      panic!("cycle protection must run before Trigger evaluation")
    }

    async fn accept_trigger(
      &self,
      _request: octacity_server_store::AcceptTrigger,
    ) -> Result<octacity_server_store::AcceptTriggerOutcome, StoreError> {
      panic!("cycle protection must run before Trigger evaluation")
    }

    async fn suppress_trigger(
      &self,
      _request: octacity_server_store::SuppressTrigger,
    ) -> Result<octacity_server_store::SuppressTriggerOutcome, StoreError> {
      panic!("cycle protection must run before Trigger evaluation")
    }
  }

  struct NeverContext;

  #[async_trait]
  impl ManualTriggerContextProvider for NeverContext {
    async fn load_for(
      &self,
      _trigger: TriggerDefinitionRef,
      _target: TriggerTarget,
      _kind: octacity_server_store::TriggerKind,
    ) -> Result<ManualTriggerContext, ManualTriggerContextError> {
      panic!("cycle protection must run before context loading")
    }
  }

  struct NeverResolver;

  #[async_trait]
  impl RevisionResolver for NeverResolver {
    async fn resolve(&self, _request: RevisionResolutionRequest) -> Result<ImmutableRevision, RevisionResolutionError> {
      panic!("cycle protection must run before revision resolution")
    }
  }

  fn root_occurrence(trigger: TriggerDefinitionRef) -> NormalizedTriggerOccurrence {
    let occurrence_id = id::<TriggerOccurrenceId>(3);
    NormalizedTriggerOccurrence::root(
      occurrence_id,
      trigger,
      target(),
      TriggerIdentity::new("manual:root").unwrap(),
      TriggerCause::Manual {},
      TriggerMetadata::default(),
      time(1),
    )
    .unwrap()
  }

  fn definition(value: u128) -> TriggerDefinitionRef {
    TriggerDefinitionRef {
      id: id::<TriggerId>(value),
      version: TriggerVersion::INITIAL,
    }
  }

  fn target() -> TriggerTarget {
    TriggerTarget {
      configuration_id: id::<BuildConfigurationId>(4),
      configuration_version: BuildConfigurationVersion::INITIAL,
    }
  }

  fn time(value: i64) -> Timestamp {
    Timestamp::from_unix_millis(value).unwrap()
  }

  fn id<T>(value: u128) -> T
  where
    T: FromStr,
    T::Err: Debug,
  {
    format!("00000000-0000-0000-0000-{value:012x}").parse().unwrap()
  }

  fn run_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
      Poll::Ready(value) => value,
      Poll::Pending => panic!("in-memory worker future unexpectedly yielded"),
    }
  }
}
