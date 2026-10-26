//! Shared installed Reporter resolution over the existing ordered outbox; producer acceptance stays independent.
use super::source_management::SourceManagement;
use octacity_server_application::*;
use octacity_server_domain::Timestamp;
use octacity_server_factory::{BudgetUsage, FactoryDigest};
use octacity_server_store::IssuePushIntent;
use std::sync::Arc;

/// One installed dispatcher with the cadence and lease policy it was built for.
pub(super) struct InstalledSourceReporting {
  pub dispatcher: Arc<dyn FactoryOutboxDispatcher>,
  pub policy: crate::FactoryRuntimeConfig,
  pub health: Arc<crate::readiness::WorkerHealth>,
}

impl SourceManagement {
  /// Installs shared reporting without I/O and registers its scoped worker health.
  /// Overlap removes conflicting explicit workers and returns unavailable, so
  /// neither owner can claim another profile's accepted history.
  pub(super) fn reporting_dispatcher(
    self: &Arc<Self>,
    explicit: &mut Vec<super::factory::InstalledFactoryOutbox>,
    readiness: &mut crate::readiness::ReadinessChecks,
  ) -> Result<Option<InstalledSourceReporting>, ApplicationError> {
    if self.profiles.reporters.is_empty() {
      return Ok(None);
    }
    let conflicts = |worker: &super::factory::InstalledFactoryOutbox| {
      worker.dispatcher.projection_capabilities().iter().any(|capability| {
        self
          .profiles
          .reporters
          .iter()
          .any(|reporter| &reporter.write.capability.capability == capability)
      })
    };
    if explicit.iter().any(&conflicts) {
      // Neither owner may consume an accepted profile belonging to the other.
      // Stop only ambiguous reporting workers; execution/manual paths stay installed.
      explicit.retain(|worker| !conflicts(worker));
      return Err(ApplicationError::capability_unavailable());
    }
    let consumer = self.profiles.consumer;
    let projector = self.clone() as Arc<dyn FactoryWorkStatusProjector>;
    let capabilities = self
      .profiles
      .reporters
      .iter()
      .map(|r| (r.write.capability.capability.clone(), projector.clone()))
      .collect();
    let worker = FactoryWorkStatusOutboxWorker::new(
      self.store.clone(),
      capabilities,
      FactoryReporterWorkerSettings::new(
        "managed-source-reporting"
          .parse()
          .map_err(|_| ApplicationError::invalid())?,
        consumer.claim_lifetime_millis,
        consumer.batch_size,
      )?,
      Arc::new(SystemFactoryExecutionClock),
    )?;
    let policy = crate::FactoryRuntimeConfig::source_consumer(
      consumer.worker(),
      consumer.max_attempts,
      consumer.maximum_retry_millis,
    );
    let health = Arc::new(crate::readiness::WorkerHealth::new(
      "managed-source-reporting",
      policy.poll_interval().saturating_add(policy.claim_lifetime()),
    ));
    readiness.push_factory(health.clone());
    Ok(Some(InstalledSourceReporting {
      dispatcher: Arc::new(worker),
      policy,
      health,
    }))
  }
}

/// Ambiguous optional delivery remains visible without gating manual operations.
pub(super) struct UnavailableSourceReporting;
#[async_trait::async_trait]
impl crate::readiness::ReadinessCheck for UnavailableSourceReporting {
  fn name(&self) -> &'static str {
    "managed-source-reporting"
  }
  async fn check(&self) -> bool {
    false
  }
}
#[async_trait::async_trait]
impl FactoryWorkStatusProjector for SourceManagement {
  fn supports_issue_delivery(&self) -> bool {
    self.profiles.reporters.iter().any(|r| r.issue_delivery.is_some())
  }
  fn issue_delivery_policy(&self, intent: &IssuePushIntent) -> Result<IssueReporterDeliveryPolicy, ApplicationError> {
    self
      .reporter_projector(intent.project_id, Arc::new(SystemFactoryExecutionClock))?
      .issue_delivery_policy(intent)
  }
  async fn set_issue_status(
    &self,
    request: IssueStatusProjectionRequest,
  ) -> Result<(FactoryDigest, BudgetUsage), ApplicationError> {
    self
      .reporter_projector(request.project_id, Arc::new(SystemFactoryExecutionClock))?
      .set_issue_status(request)
      .await
  }
  async fn set_status(
    &self,
    request: FactoryWorkStatusRequest,
  ) -> Result<(FactoryDigest, BudgetUsage), ApplicationError> {
    self
      .reporter_projector(request.subject.project_id(), Arc::new(SystemFactoryExecutionClock))?
      .set_status(request)
      .await
  }
  async fn cancel_status(
    &self,
    request: FactoryWorkStatusRequest,
    requested_at: Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    self
      .reporter_projector(request.subject.project_id(), Arc::new(SystemFactoryExecutionClock))?
      .cancel_status(request, requested_at)
      .await
  }
}
