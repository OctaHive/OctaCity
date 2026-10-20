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
/// Source effects from accepted Factory records or explicitly enabled Issue metadata.
/// Unknown-response recovery reuses the original operation identity and leaves
/// authoritative Flow data untouched, regardless of reporter availability.
pub struct FactoryWorkStatusOutboxWorker<S> {
  store: Arc<S>,
  projectors: BTreeMap<ImmutableReference, Arc<dyn FactoryWorkStatusProjector>>,
  settings: FactoryReporterWorkerSettings,
  clock: Arc<dyn FactoryExecutionClock>,
}
enum ReporterRequest {
  Work(Box<FactoryWorkStatusRequest>),
  Issue(crate::IssueStatusProjectionRequest),
}
impl ReporterRequest {
  fn deadline(&self) -> Timestamp {
    match self {
      Self::Work(r) => r.deadline,
      Self::Issue(r) => r.deadline,
    }
  }
  fn budget(&self) -> BudgetLimit {
    match self {
      Self::Work(r) => r.budget,
      Self::Issue(r) => r.budget,
    }
  }
  async fn dispatch(
    &self,
    projector: &dyn FactoryWorkStatusProjector,
  ) -> Result<(FactoryDigest, BudgetUsage), ApplicationError> {
    match self {
      Self::Work(r) => projector.set_status(r.as_ref().clone()).await,
      Self::Issue(r) => projector.set_issue_status(r.clone()).await,
    }
  }
}
struct ProjectionMaterial<'a> {
  request: ReporterRequest,
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
impl<S: FactoryRunStore + IssueSourceProjectionStore> FactoryWorkStatusOutboxWorker<S> {
  /// Claims only reporting events, then settles under the same live fence.
  pub async fn run_once(&self, at: Timestamp) -> Result<FactoryOutboxDispatchSummary, ApplicationError> {
    let at = self.now(at)?;
    let expires = Timestamp::from_unix_millis(
      at.unix_millis()
        .checked_add(self.settings.claim_millis as i64)
        .ok_or_else(ApplicationError::invalid)?,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let mut kinds = vec![FactoryKey::new("work.status.projection").map_err(|_| ApplicationError::invalid())?];
    if self.projectors.values().any(|p| p.supports_issue_delivery()) {
      kinds.push(FactoryKey::new("issue.status.projection").map_err(|_| ApplicationError::invalid())?);
    }
    let claimed = self
      .store
      .claim_factory_outbox(
        ClaimFactoryOutbox::new(self.settings.owner.clone(), at, expires, self.settings.limit)?.only_kinds(kinds)?,
      )
      .await?;
    let mut summary = FactoryOutboxDispatchSummary {
      claimed: u16::try_from(claimed.len()).map_err(|_| ApplicationError::invalid())?,
      ..Default::default()
    };
    for owned in claimed {
      let row = owned.record;
      let claim = row.claim.ok_or_else(ApplicationError::invalid)?;
      let snapshot;
      let material = match row.subject {
        FactoryOutboxSubject::Run(run) => {
          snapshot = self.store.factory_run_snapshot(run).await?;
          self.material(&snapshot, &row, at)
        }
        FactoryOutboxSubject::Issue(_) => self.issue_material(&row, at).await,
      };
      let settlement = match material {
        // Unavailable accepted material leaves ownership recoverable after expiry.
        // It cannot establish a terminal delivery failure or authorize another input.
        Err(error) if transient(&error) => return Err(error),
        Err(ApplicationError::ReporterRejected(reason)) => FactoryOutboxSettlement::Rejected(reason),
        Err(ApplicationError::CapabilityUnavailable) => {
          FactoryOutboxSettlement::Rejected(FactoryOutboxFailure::Unsupported)
        }
        Err(_) => FactoryOutboxSettlement::Failed,
        Ok(ProjectionMaterial {
          request,
          policy,
          projector,
          original_deadline,
        }) => {
          if row.attempt >= policy.max_attempts || self.now(at)? >= request.deadline() {
            FactoryOutboxSettlement::Rejected(FactoryOutboxFailure::Exhausted)
          } else {
            let result = request.dispatch(projector.as_ref()).await;
            let now = self.now(at)?;
            claim
              .verify_fence(claim.fence(), now)
              .map_err(|_| ApplicationError::invalid())?;
            match result {
              Ok((_, usage))
                if usage.attempts == 0 && usage.validate(request.budget()).is_ok() && now < request.deadline() =>
              {
                FactoryOutboxSettlement::Delivered
              }
              Err(error) if transient(&error) && row.attempt + 1 < policy.max_attempts => {
                let provider_delay = match &error {
                  ApplicationError::SourceDependency {
                    retry_after_millis: Some(delay),
                    ..
                  } => u64::from(*delay),
                  _ => 0,
                };
                let retry = now
                  .unix_millis()
                  .checked_add(policy.retry_delay_millis.max(provider_delay) as i64)
                  .and_then(|n| Timestamp::from_unix_millis(n).ok());
                match retry {
                  Some(retry) if retry < original_deadline => FactoryOutboxSettlement::RetryAt(retry),
                  _ => FactoryOutboxSettlement::Rejected(FactoryOutboxFailure::Exhausted),
                }
              }
              Err(ApplicationError::SourceDependency {
                class: WorkSourceFailure::Conflict,
                ..
              }) => FactoryOutboxSettlement::Rejected(FactoryOutboxFailure::Conflict),
              Err(ApplicationError::ReporterRejected(reason)) => FactoryOutboxSettlement::Rejected(reason),
              Err(error) if transient(&error) => FactoryOutboxSettlement::Rejected(FactoryOutboxFailure::Exhausted),
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
        FactoryOutboxSettlement::Failed | FactoryOutboxSettlement::Rejected(_) => summary.failed += 1,
      }
    }
    Ok(summary)
  }
  async fn issue_material<'a>(
    &'a self,
    row: &FactoryOutboxRecord,
    at: Timestamp,
  ) -> Result<ProjectionMaterial<'a>, ApplicationError> {
    let projection = self.store.issue_push_projection(row.operation_id).await?;
    if projection.record != *row {
      return Err(ApplicationError::invalid());
    }
    let desired = projection
      .intent
      .desired
      .as_ref()
      .ok_or_else(ApplicationError::invalid)?;
    let projector = self
      .projectors
      .get(&desired.reporter.capability)
      .ok_or_else(ApplicationError::invalid)?;
    let policy = projector.issue_delivery_policy(&projection.intent)?;
    let request = crate::accepted_issue_status_request(&projection, policy, at)?;
    Ok(ProjectionMaterial {
      request: ReporterRequest::Issue(request),
      policy: policy.retry,
      original_deadline: policy.deadline(projection.accepted_at)?,
      projector,
    })
  }
  fn material<'a>(
    &'a self,
    snapshot: &FactoryRunSnapshot,
    row: &FactoryOutboxRecord,
    at: Timestamp,
  ) -> Result<ProjectionMaterial<'a>, ApplicationError> {
    let material = accepted_status_material(snapshot, row, at)?;
    Ok(ProjectionMaterial {
      request: ReporterRequest::Work(Box::new(material.request)),
      policy: material.policy,
      projector: self
        .projectors
        .get(material.plugin)
        .ok_or_else(ApplicationError::invalid)?,
      original_deadline: material.original_deadline,
    })
  }
}
struct AcceptedStatusMaterial<'a> {
  request: FactoryWorkStatusRequest,
  policy: crate::factory_flow_action::FactoryReporterRetryPolicy,
  plugin: &'a ImmutableReference,
  original_deadline: Timestamp,
  source_update: Option<crate::FactoryStageSourceUpdate>,
}
/// Verifies an exact source projection against accepted action history and the current live outbox claim.
/// Pending, superseded, terminal, forged or expired requests cannot authorize provider effects.
pub fn validate_accepted_factory_status_request(
  snapshot: &FactoryRunSnapshot,
  plugin: &ImmutableReference,
  request: &FactoryWorkStatusRequest,
  at: Timestamp,
) -> Result<(), ApplicationError> {
  accepted_factory_status_source_update(snapshot, plugin, request, at).map(|_| ())
}
/// Returns only the source effects pinned by the verified accepted action and live outbox claim.
/// Caller-supplied parameters or later task/card state cannot populate this result.
pub fn accepted_factory_status_source_update(
  snapshot: &FactoryRunSnapshot,
  plugin: &ImmutableReference,
  request: &FactoryWorkStatusRequest,
  at: Timestamp,
) -> Result<Option<crate::FactoryStageSourceUpdate>, ApplicationError> {
  let row = snapshot
    .outbox
    .iter()
    .filter(|row| row.operation_id == request.operation_id)
    .max_by_key(|row| {
      let state = match row.state {
        FactoryOutboxState::Pending => 0,
        FactoryOutboxState::Claimed => 1,
        FactoryOutboxState::Delivered | FactoryOutboxState::Failed => 2,
      };
      (row.attempt, state, row.recorded_at, row.id)
    })
    .ok_or_else(ApplicationError::invalid)?;
  let material = accepted_status_material(snapshot, row, at)?;
  if material.plugin != plugin || material.request != *request {
    return Err(ApplicationError::invalid());
  }
  Ok(material.source_update)
}
fn accepted_status_material<'a>(
  snapshot: &'a FactoryRunSnapshot,
  row: &FactoryOutboxRecord,
  at: Timestamp,
) -> Result<AcceptedStatusMaterial<'a>, ApplicationError> {
  let claim = row.claim.ok_or_else(ApplicationError::invalid)?;
  if row.state != FactoryOutboxState::Claimed || row.kind.as_str() != "work.status.projection" {
    return Err(ApplicationError::invalid());
  }
  claim
    .verify_fence(claim.fence(), at)
    .map_err(|_| ApplicationError::invalid())?;
  let claim_end = claim.expires_at();
  let AcceptedFactoryStatusAction { attempt, node, binding } =
    accepted_factory_status_action(snapshot, row).map_err(|_| ApplicationError::invalid())?;
  let parameters: StatusParameters =
    serde_json::from_value(binding.parameters().clone()).map_err(|_| ApplicationError::invalid())?;
  let policy = parameters.outbox.ok_or_else(ApplicationError::invalid)?;
  policy.validate()?;
  let deadline = attempt.deadline().min(claim_end);
  if at >= deadline || row.attempt >= policy.max_attempts {
    return Err(ApplicationError::invalid());
  }
  Ok(AcceptedStatusMaterial {
    request: FactoryWorkStatusRequest {
      operation_id: row.operation_id,
      outbox_claim: Some(claim),
      work_id: snapshot.work.id(),
      external_identity: snapshot.work.external_identity().clone(),
      subject: snapshot.work.subject().clone(),
      status: parameters.status,
      permissions: node.permissions().clone(),
      budget: node.budget(),
      deadline,
    },
    policy,
    plugin: binding.plugin(),
    original_deadline: attempt.deadline(),
    source_update: parameters.source_update,
  })
}

#[async_trait::async_trait]
impl<S: FactoryRunStore + IssueSourceProjectionStore + 'static> FactoryOutboxDispatcher
  for FactoryWorkStatusOutboxWorker<S>
{
  async fn dispatch_once(&self, at: Timestamp) -> Result<FactoryOutboxDispatchSummary, ApplicationError> {
    self.run_once(at).await
  }
}
fn transient(error: &ApplicationError) -> bool {
  matches!(
    error,
    ApplicationError::Store(StoreError::Unavailable)
      | ApplicationError::SourceDependency {
        class: WorkSourceFailure::RateLimited
          | WorkSourceFailure::Transient
          | WorkSourceFailure::Unavailable
          | WorkSourceFailure::Timeout
          | WorkSourceFailure::Cancelled,
        ..
      }
  )
}
