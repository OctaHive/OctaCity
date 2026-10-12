//! Non-authoritative reporting from immutable accepted records, under independent outbox fences.
use crate::factory_flow_action::StatusParameters;
use crate::{ApplicationError, FactoryExecutionClock, FactoryWorkStatusProjector, FactoryWorkStatusRequest};
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::*;
use std::{collections::BTreeMap, sync::Arc};

/// Independent reporting-worker settings; no setting chooses a Flow transition.
#[derive(Clone, Debug)]
pub struct FactoryReporterWorkerSettings {
  owner: FactoryKey,
  claim_millis: u64,
  limit: u16,
}
impl FactoryReporterWorkerSettings {
  /// Validates a bounded dispatch window and batch size.
  pub fn new(owner: FactoryKey, claim_millis: u64, limit: u16) -> Result<Self, ApplicationError> {
    if claim_millis == 0 || claim_millis > 3_600_000 {
      return Err(ApplicationError::invalid());
    }
    ClaimFactoryOutbox::new(
      owner.clone(),
      Timestamp::from_unix_millis(0).map_err(|_| ApplicationError::invalid())?,
      Timestamp::from_unix_millis(claim_millis as i64).map_err(|_| ApplicationError::invalid())?,
      limit,
    )?;
    Ok(Self {
      owner,
      claim_millis,
      limit,
    })
  }
}
/// Classified non-authoritative dispatch progress, with no provider diagnostics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FactoryOutboxDispatchSummary {
  /// Events owned by this bounded pass.
  pub claimed: u16,
  /// Events observed or applied idempotently.
  pub delivered: u16,
  /// Events rescheduled within their published retry policy.
  pub retrying: u16,
  /// Events that exhausted their own reporting policy.
  pub failed: u16,
}
/// Runtime extension for deferred technical effects; it owns no Flow decisions.
#[async_trait::async_trait]
pub trait FactoryOutboxDispatcher: Send + Sync {
  /// Runs one bounded fenced dispatch pass; source state never enters the owner.
  async fn dispatch_once(&self, at: Timestamp) -> Result<FactoryOutboxDispatchSummary, ApplicationError>;
}
/// Arbitrary Work status projection from accepted node records only.
/// Unknown-response recovery reuses the original operation identity and leaves
/// authoritative Flow data untouched, regardless of reporter availability.
pub struct FactoryWorkStatusOutboxWorker<S> {
  store: Arc<S>,
  projectors: BTreeMap<ImmutableReference, Arc<dyn FactoryWorkStatusProjector>>,
  settings: FactoryReporterWorkerSettings,
  clock: Arc<dyn FactoryExecutionClock>,
}
struct ProjectionMaterial<'a> {
  request: FactoryWorkStatusRequest,
  policy: crate::factory_flow_action::FactoryReporterRetryPolicy,
  projector: &'a Arc<dyn FactoryWorkStatusProjector>,
  original_deadline: Timestamp,
}
impl<S> FactoryWorkStatusOutboxWorker<S> {
  /// Installs trusted projectors by exact immutable identity, independent of source names.
  pub fn new(
    store: Arc<S>,
    projectors: BTreeMap<ImmutableReference, Arc<dyn FactoryWorkStatusProjector>>,
    settings: FactoryReporterWorkerSettings,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Result<Self, ApplicationError> {
    if projectors.is_empty() || projectors.len() > 256 {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      store,
      projectors,
      settings,
      clock,
    })
  }
  fn now(&self, at: Timestamp) -> Result<Timestamp, ApplicationError> {
    self.clock.now().map(|now| now.max(at))
  }
}
impl<S: FactoryRunStore> FactoryWorkStatusOutboxWorker<S> {
  /// Claims only reporting events, then settles under the same live fence.
  pub async fn run_once(&self, at: Timestamp) -> Result<FactoryOutboxDispatchSummary, ApplicationError> {
    let at = self.now(at)?;
    let expires = Timestamp::from_unix_millis(
      at.unix_millis()
        .checked_add(self.settings.claim_millis as i64)
        .ok_or_else(ApplicationError::invalid)?,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let claimed = self
      .store
      .claim_factory_outbox(
        ClaimFactoryOutbox::new(self.settings.owner.clone(), at, expires, self.settings.limit)?
          .only_kind(FactoryKey::new("work.status.projection").map_err(|_| ApplicationError::invalid())?),
      )
      .await?;
    let mut summary = FactoryOutboxDispatchSummary {
      claimed: u16::try_from(claimed.len()).map_err(|_| ApplicationError::invalid())?,
      ..Default::default()
    };
    for owned in claimed {
      let row = owned.record;
      let claim = row.claim.ok_or_else(ApplicationError::invalid)?;
      let snapshot = self.store.factory_run_snapshot(row.run_id).await?;
      let material = self.material(&snapshot, &row, claim.expires_at(), at);
      let settlement = match material {
        Err(_) => FactoryOutboxSettlement::Failed,
        Ok(ProjectionMaterial {
          request,
          policy,
          projector,
          original_deadline,
        }) => {
          if row.attempt >= policy.max_attempts || self.now(at)? >= request.deadline {
            FactoryOutboxSettlement::Failed
          } else {
            let result = projector.set_status(request.clone()).await;
            let now = self.now(at)?;
            claim
              .verify_fence(claim.fence(), now)
              .map_err(|_| ApplicationError::invalid())?;
            match result {
              Ok((_, usage))
                if usage.attempts == 0 && usage.validate(request.budget).is_ok() && now < request.deadline =>
              {
                FactoryOutboxSettlement::Delivered
              }
              Err(error) if transient(&error) && row.attempt + 1 < policy.max_attempts => {
                let retry = now
                  .unix_millis()
                  .checked_add(policy.retry_delay_millis as i64)
                  .and_then(|n| Timestamp::from_unix_millis(n).ok());
                match retry {
                  Some(retry) if retry < original_deadline => FactoryOutboxSettlement::RetryAt(retry),
                  _ => FactoryOutboxSettlement::Failed,
                }
              }
              _ => FactoryOutboxSettlement::Failed,
            }
          }
        }
      };
      let now = self.now(at)?;
      claim
        .verify_fence(claim.fence(), now)
        .map_err(|_| ApplicationError::invalid())?;
      self
        .store
        .settle_factory_outbox(SettleFactoryOutbox {
          operation_id: row.operation_id,
          owner: self.settings.owner.clone(),
          fence: claim.fence(),
          observed_at: now,
          settlement,
        })
        .await?;
      match settlement {
        FactoryOutboxSettlement::Delivered => summary.delivered += 1,
        FactoryOutboxSettlement::RetryAt(_) => summary.retrying += 1,
        FactoryOutboxSettlement::Failed => summary.failed += 1,
      }
    }
    Ok(summary)
  }
  fn record<'a>(
    &self,
    snapshot: &'a FactoryRunSnapshot,
    row: &FactoryOutboxRecord,
  ) -> Result<&'a FlowNodeRecord, ApplicationError> {
    snapshot
      .flow
      .data
      .records
      .iter()
      .find(|record| record.digest().ok() == Some(row.input_digest))
      .ok_or_else(ApplicationError::invalid)
  }
  fn material<'a>(
    &'a self,
    snapshot: &FactoryRunSnapshot,
    row: &FactoryOutboxRecord,
    claim_end: Timestamp,
    at: Timestamp,
  ) -> Result<ProjectionMaterial<'a>, ApplicationError> {
    let record = self.record(snapshot, row)?;
    let attempt = snapshot
      .flow
      .attempts
      .iter()
      .find(|attempt| attempt.id() == record.node_attempt_id())
      .ok_or_else(ApplicationError::invalid)?;
    let node = snapshot
      .admitted_flow
      .closure()
      .definition(record.input().definition())
      .and_then(|definition| definition.node(record.input().node()))
      .ok_or_else(ApplicationError::invalid)?;
    let binding = node.action().ok_or_else(ApplicationError::invalid)?;
    if node.kind() != FlowNodeKind::TrustedAction
      || binding.action().as_str() != "set_status"
      || attempt.execution() != &NodeExecutionIdentity::External(binding.plugin().clone())
      || record.payload().value()["receipt"] != serde_json::json!(row.operation_id)
      || !node.permissions().plugins().any(|allowed| allowed == binding.plugin())
    {
      return Err(ApplicationError::invalid());
    }
    let parameters: StatusParameters =
      serde_json::from_value(binding.parameters().clone()).map_err(|_| ApplicationError::invalid())?;
    let policy = parameters.outbox.ok_or_else(ApplicationError::invalid)?;
    policy.validate()?;
    let projector = self
      .projectors
      .get(binding.plugin())
      .ok_or_else(ApplicationError::invalid)?;
    let deadline = attempt.deadline().min(claim_end);
    if at >= deadline {
      return Err(ApplicationError::invalid());
    }
    Ok(ProjectionMaterial {
      request: FactoryWorkStatusRequest {
        operation_id: row.operation_id,
        work_id: snapshot.work.id(),
        external_identity: snapshot.work.external_identity().clone(),
        subject: snapshot.work.subject().clone(),
        status: parameters.status,
        permissions: node.permissions().clone(),
        budget: node.budget(),
        deadline,
      },
      policy,
      projector,
      original_deadline: attempt.deadline(),
    })
  }
}
#[async_trait::async_trait]
impl<S: FactoryRunStore + 'static> FactoryOutboxDispatcher for FactoryWorkStatusOutboxWorker<S> {
  async fn dispatch_once(&self, at: Timestamp) -> Result<FactoryOutboxDispatchSummary, ApplicationError> {
    self.run_once(at).await
  }
}
fn transient(error: &ApplicationError) -> bool {
  matches!(error, ApplicationError::Store(StoreError::Unavailable))
}
