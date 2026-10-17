//! Original source association and accepted-action authorization precede every process dispatch.
use super::*;
use octacity_work_source_protocol::{FailureClass, Request, Response, reporter};
#[path = "work_reporter_ordering.rs"]
mod ordering;

// A failed request and an unfinished remote write have different ordering consequences.
struct InvocationFailure {
  error: ApplicationError,
  uncertain: bool,
}
impl From<ApplicationError> for InvocationFailure {
  fn from(error: ApplicationError) -> Self {
    Self { error, uncertain: true }
  }
}
impl From<InvocationFailure> for ApplicationError {
  fn from(failure: InvocationFailure) -> Self {
    failure.error
  }
}

impl<S: FactorySourceStore + FactoryRunStore> HostedFactoryWorkStatusProjector<S> {
  async fn material(
    &self,
    request: &FactoryWorkStatusRequest,
  ) -> Result<(&FactoryReporterProfile, reporter::ProjectionIntent), ApplicationError> {
    let preparation = self
      .store
      .source_preparation_for_work(request.subject.project_id(), request.work_id)
      .await?
      .ok_or_else(ApplicationError::invalid)?;
    let correlation = preparation.admission.as_ref().ok_or_else(ApplicationError::invalid)?;
    let snapshot = self.store.factory_run_snapshot(correlation.run_id).await?;
    let binding = preparation
      .input
      .definition()
      .reporter
      .as_ref()
      .ok_or_else(ApplicationError::invalid)?;
    validate_accepted_factory_status_request(&snapshot, &binding.capability, request, self.clock.now()?)?;
    let profile = self
      .profiles
      .iter()
      .find(|p| p.draft.project_id == request.subject.project_id() && p.binding == *binding)
      .ok_or_else(ApplicationError::unavailable)?;
    let draft = &profile.draft;
    if correlation.work_id != request.work_id
      || snapshot.work.id() != request.work_id
      || preparation
        .admission_identity(ManagementSecurityScope::trusted_network())
        .external_identity
        != request.external_identity
      || preparation.item.resource.as_str() != draft.access.resource_id
      || !request.permissions.plugins().any(|p| p == &draft.plugin)
      || !request
        .permissions
        .secret_profiles()
        .any(|p| p == &draft.credential_profile)
      || draft.network_hosts.iter().any(|host| {
        !request
          .permissions
          .network_hosts()
          .any(|allowed| allowed.as_str() == host)
      })
    {
      return Err(ApplicationError::invalid());
    }
    let original: WorkSourceSnapshot =
      serde_json::from_slice(&preparation.snapshot).map_err(|_| ApplicationError::invalid())?;
    if original.item != preparation.item {
      return Err(ApplicationError::invalid());
    }
    let intent = reporter::ProjectionIntent {
      projection_id: request.operation_id.to_string(),
      configuration_digest: draft.provider_configuration.to_string(),
      resource: draft.access.clone(),
      item_id: preparation.item.item.as_str().into(),
      display_id: original.display_id,
      mapping: draft.mapping.clone(),
      status: request.status.as_str().into(),
      network_hosts: draft.network_hosts.clone(),
    };
    intent.validate().map_err(|_| ApplicationError::invalid())?;
    Ok((profile, intent))
  }
  async fn invoke(
    &self,
    profile: &FactoryReporterProfile,
    intent: reporter::ProjectionIntent,
    step: Option<reporter::ProjectionStep>,
    end: tokio::time::Instant,
  ) -> Result<(reporter::ProjectionPlan, u64), InvocationFailure> {
    let remaining = end.saturating_duration_since(tokio::time::Instant::now());
    if remaining.is_zero() {
      return Err(ApplicationError::unavailable().into());
    }
    let adapter = self
      .registry
      .resolve(
        profile.draft.plugin.identity().as_str(),
        &profile.draft.plugin.digest().to_string(),
      )
      .map_err(|_| ApplicationError::unavailable())?;
    let project = step.is_some();
    let request = Request::Reporter(reporter::Request {
      protocol_version: reporter::ORDERED_PROTOCOL_VERSION,
      request_id: format!("reporter-{}", uuid::Uuid::new_v4()),
      command: if let Some(step) = step {
        reporter::Command::Apply(Box::new(reporter::StepRequest { intent, step }))
      } else {
        reporter::Command::Inspect(Box::new(intent))
      },
    });
    let response = adapter
      .execute(request, remaining, self.grace, self.cancellation.child_token())
      .await
      .map_err(|error| {
        use octacity_server_work_source::HostFailureClass;
        match error.class() {
          // Once Project was dispatched, an invalid or missing response cannot
          // establish whether effects landed. A bounded outbox retry observes
          // the original intent before it can authorize another write.
          HostFailureClass::ProtocolFault if project => ApplicationError::unavailable(),
          HostFailureClass::Permanent | HostFailureClass::InvalidRequest | HostFailureClass::ProtocolFault => {
            ApplicationError::invalid()
          }
          _ => ApplicationError::unavailable(),
        }
      })?;
    let size = serde_json::to_vec(&response)
      .map_err(|_| ApplicationError::invalid())?
      .len() as u64;
    match response {
      Response::Reporter(reporter::Response {
        outcome: reporter::Outcome::Planned(receipt) | reporter::Outcome::Progressed(receipt),
        ..
      }) => Ok((receipt, size)),
      Response::Reporter(reporter::Response {
        outcome: reporter::Outcome::Failure(failure),
        ..
      }) => Err(InvocationFailure {
        // Transient/invalid completion can still follow an applied remote request.
        uncertain: !matches!(
          failure.class,
          FailureClass::Conflict | FailureClass::Permanent | FailureClass::InvalidRequest | FailureClass::Unsupported
        ),
        error: match failure.class {
          FailureClass::Conflict => ApplicationError::SourceDependency {
            class: octacity_server_store::WorkSourceFailure::Conflict,
            retry_after_millis: None,
          },
          FailureClass::Transient => ApplicationError::SourceDependency {
            class: if failure.retry_after_millis.is_some() {
              octacity_server_store::WorkSourceFailure::RateLimited
            } else {
              octacity_server_store::WorkSourceFailure::Transient
            },
            retry_after_millis: failure.retry_after_millis,
          },
          FailureClass::Unsupported | FailureClass::Cancelled => ApplicationError::unavailable(),
          _ => ApplicationError::invalid(),
        },
      }),
      _ => Err(ApplicationError::invalid().into()),
    }
  }
}
#[async_trait::async_trait]
impl<S: FactorySourceStore + FactoryRunStore> FactoryWorkStatusProjector for HostedFactoryWorkStatusProjector<S> {
  async fn set_status(
    &self,
    request: FactoryWorkStatusRequest,
  ) -> Result<(FactoryDigest, BudgetUsage), ApplicationError> {
    let started = tokio::time::Instant::now();
    let now = self.clock.now()?;
    let duration = u64::try_from(request.deadline.unix_millis() - now.unix_millis())
      .ok()
      .filter(|v| *v > 0)
      .ok_or_else(ApplicationError::invalid)?;
    let end = started
      + self
        .timeout
        .min(Duration::from_millis(duration))
        .min(Duration::from_millis(request.budget.max_elapsed_millis()));
    let (profile, intent) = tokio::time::timeout_at(end, self.material(&request))
      .await
      .map_err(|_| ApplicationError::unavailable())??;
    self.project_ordered(&request, profile, intent, started, end).await
  }

  async fn cancel_status(
    &self,
    _: FactoryWorkStatusRequest,
    _: octacity_server_domain::Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    // This projector authorizes only independently claimed outbox dispatches.
    // The synchronous action cancellation path has no such authority; host
    // timeouts/cancellation and worker fencing stop an outbox operation instead.
    Err(ApplicationError::capability_unavailable())
  }
}
fn measured_usage(started: tokio::time::Instant, output_bytes: u64) -> Result<BudgetUsage, ApplicationError> {
  Ok(BudgetUsage {
    elapsed_millis: u64::try_from(started.elapsed().as_millis()).map_err(|_| ApplicationError::invalid())?,
    output_bytes,
    ..Default::default()
  })
}
