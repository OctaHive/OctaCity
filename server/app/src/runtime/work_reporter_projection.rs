//! Original source association and accepted-action authorization precede every process dispatch.
use super::*;
use octacity_server_application::{
  IssueReporterDeliveryPolicy, IssueStatusProjectionRequest, accepted_issue_status_request,
};
use octacity_server_store::{IssuePushIntent, IssueSourceProjectionStore};
use octacity_work_source_protocol::{FailureClass, Request, Response, reporter};
#[path = "work_reporter_ordering.rs"]
mod ordering;

enum OrderedRequest<'a> {
  Work(&'a FactoryWorkStatusRequest),
  Issue(&'a IssueStatusProjectionRequest),
}
impl OrderedRequest<'_> {
  fn budget(&self) -> BudgetLimit {
    match self {
      Self::Work(r) => r.budget,
      Self::Issue(r) => r.budget,
    }
  }
  fn deadline(&self) -> octacity_server_domain::Timestamp {
    match self {
      Self::Work(r) => r.deadline,
      Self::Issue(r) => r.deadline,
    }
  }
  fn operation_id(&self) -> FactoryDigest {
    match self {
      Self::Work(r) => r.operation_id,
      Self::Issue(r) => r.operation_id,
    }
  }
  fn project_id(&self) -> ProjectId {
    match self {
      Self::Work(r) => r.subject.project_id(),
      Self::Issue(r) => r.project_id,
    }
  }
  fn work_id(&self) -> Option<WorkEnvelopeId> {
    match self {
      Self::Work(r) => Some(r.work_id),
      Self::Issue(_) => None,
    }
  }
  fn claim(&self) -> Result<FactoryClaim, ApplicationError> {
    match self {
      Self::Work(r) => r.outbox_claim.ok_or_else(ApplicationError::invalid),
      Self::Issue(r) => Ok(r.claim),
    }
  }
}
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

impl<S: FactorySourceStore + FactoryRunStore + IssueSourceProjectionStore> HostedFactoryWorkStatusProjector<S> {
  fn projection_end(
    &self,
    deadline: octacity_server_domain::Timestamp,
    budget: BudgetLimit,
    started: tokio::time::Instant,
  ) -> Result<tokio::time::Instant, ApplicationError> {
    let duration = u64::try_from(deadline.unix_millis() - self.clock.now()?.unix_millis())
      .ok()
      .filter(|n| *n > 0)
      .ok_or_else(ApplicationError::invalid)?;
    Ok(
      started
        + self
          .timeout
          .min(Duration::from_millis(duration))
          .min(Duration::from_millis(budget.max_elapsed_millis())),
    )
  }
  async fn material(
    &self,
    request: &FactoryWorkStatusRequest,
  ) -> Result<(FactoryReporterProfile, reporter::ProjectionIntent), ApplicationError> {
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
    let stage_update =
      accepted_factory_status_source_update(&snapshot, &binding.capability, request, self.clock.now()?)?;
    let mut profile = self
      .profiles
      .iter()
      .find(|p| p.draft.project_id == request.subject.project_id() && p.binding == *binding)
      .cloned()
      .ok_or_else(ApplicationError::unavailable)?;
    profile.require_native_configuration(
      preparation
        .input
        .definition()
        .connection
        .as_ref()
        .and_then(|connection| connection.reporter_configuration.as_ref()),
    )?;
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
    let stage_mapping = if let Some(update) = stage_update {
      update.validate_for_input(preparation.input.definition())?;
      if update.effects.status != request.status.as_str() {
        return Err(ApplicationError::invalid());
      }
      Some(WorkSourceStatusMapping {
        statuses: vec![update.effects],
      })
    } else {
      None
    };
    let intent = reporter::ProjectionIntent {
      effects: if let Some(mapping) = stage_mapping
        .as_ref()
        .or(preparation.input.definition().reporting.as_ref())
      {
        draft
          .reporting_policy
          .as_ref()
          .ok_or_else(ApplicationError::invalid)?
          .validate_mapping(mapping)
          .map_err(|_| ApplicationError::invalid())?;
        let effects = mapping
          .effects(request.status.as_str())
          .ok_or_else(ApplicationError::invalid)?;
        Some(reporter::ProjectionEffects {
          add: effects.add.clone(),
          remove: effects.remove.clone(),
          state: effects.state.clone(),
        })
      } else {
        None
      },
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
    if let Some(connection) = &preparation.input.definition().connection {
      if profile.managed_access.as_ref().is_some_and(|access| {
        !access.matches_work_connection(connection, octacity_server_store::IntegrationPurpose::TaskUpdate)
      }) {
        return Err(ApplicationError::ReporterRejected(
          octacity_server_store::FactoryOutboxFailure::Forbidden,
        ));
      }
      profile.managed_access = Some(
        self
          .managed_connections
          .as_ref()
          .ok_or_else(ApplicationError::unavailable)?
          .for_work_binding(
            connection,
            &profile.binding,
            octacity_server_store::IntegrationPurpose::TaskUpdate,
          )?,
      );
    }
    Ok((profile, intent))
  }
  fn issue_profile(&self, intent: &IssuePushIntent) -> Result<FactoryReporterProfile, ApplicationError> {
    let desired = intent.desired.as_ref().ok_or_else(ApplicationError::invalid)?;
    self
      .profiles
      .iter()
      .find(|p| p.draft.project_id == intent.project_id && p.binding == desired.reporter)
      .cloned()
      .ok_or_else(ApplicationError::capability_unavailable)
  }
  async fn issue_material(
    &self,
    request: &IssueStatusProjectionRequest,
  ) -> Result<(FactoryReporterProfile, reporter::ProjectionIntent), ApplicationError> {
    let original = self.store.issue_push_projection(request.operation_id).await?;
    let mut profile = self.issue_profile(&original.intent)?;
    profile.require_native_configuration(
      original
        .intent
        .desired
        .as_ref()
        .and_then(|desired| desired.writer_access.as_ref())
        .and_then(|access| access.configuration.as_ref()),
    )?;
    let policy = profile
      .draft
      .issue_delivery
      .ok_or_else(ApplicationError::capability_unavailable)?;
    if accepted_issue_status_request(&original, policy, self.clock.now()?)? != *request {
      return Err(ApplicationError::invalid());
    }
    let desired = original.intent.desired.as_ref().ok_or_else(ApplicationError::invalid)?;
    if let Some(pin) = &desired.writer_access {
      if profile
        .managed_access
        .as_ref()
        .is_some_and(|access| access.issue_writer_access() != *pin)
      {
        return Err(ApplicationError::ReporterRejected(
          octacity_server_store::FactoryOutboxFailure::Forbidden,
        ));
      }
      if let Some(connections) = &self.managed_connections {
        profile.managed_access = Some(connections.for_issue_writer(pin, &desired.reporter));
      } else if !profile
        .managed_access
        .as_ref()
        .is_some_and(|a| a.issue_writer_access() == *pin)
      {
        return Err(ApplicationError::ReporterRejected(
          octacity_server_store::FactoryOutboxFailure::Forbidden,
        ));
      }
    }
    let effects = desired.effects.as_ref().ok_or_else(ApplicationError::invalid)?;
    profile
      .draft
      .reporting_policy
      .as_ref()
      .ok_or_else(ApplicationError::invalid)?
      .validate_mapping(&WorkSourceStatusMapping {
        statuses: vec![effects.clone()],
      })
      .map_err(|_| ApplicationError::invalid())?;
    if desired.link.selection.item.resource.as_str() != profile.draft.access.resource_id {
      return Err(ApplicationError::invalid());
    }
    let intent = reporter::ProjectionIntent {
      projection_id: request.operation_id.to_string(),
      configuration_digest: profile.draft.provider_configuration.to_string(),
      resource: profile.draft.access.clone(),
      item_id: desired.link.selection.item.item.as_str().into(),
      display_id: desired
        .link
        .display_id
        .clone()
        .ok_or_else(ApplicationError::capability_unavailable)?,
      mapping: profile.draft.mapping.clone(),
      status: desired.status_id.to_string(),
      effects: Some(reporter::ProjectionEffects {
        add: effects.add.clone(),
        remove: effects.remove.clone(),
        state: effects.state.clone(),
      }),
      network_hosts: profile.draft.network_hosts.clone(),
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
    if tokio::time::Instant::now() >= end {
      return Err(ApplicationError::unavailable().into());
    }
    if let Some(access) = &profile.managed_access {
      tokio::time::timeout_at(
        end,
        access.authorize_work_binding(
          profile.draft.project_id,
          &profile.binding,
          &profile.draft.plugin,
          &profile.draft.access.resource_id,
          octacity_server_store::IntegrationPurpose::TaskUpdate,
        ),
      )
      .await
      .unwrap_or_else(|_| Err(ApplicationError::unavailable()))
      .map_err(|error| InvocationFailure {
        error: match error {
          ApplicationError::CapabilityUnavailable
          | ApplicationError::Store(octacity_server_store::StoreError::CredentialRejected) => {
            ApplicationError::ReporterRejected(octacity_server_store::FactoryOutboxFailure::Forbidden)
          }
          other => other,
        },
        uncertain: false,
      })?;
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
    // Authorization consumes the same window; it cannot renew provider time.
    let remaining = end.saturating_duration_since(tokio::time::Instant::now());
    if remaining.is_zero() {
      return Err(InvocationFailure {
        error: ApplicationError::unavailable(),
        uncertain: false,
      });
    }
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
          FailureClass::Unsupported => {
            ApplicationError::ReporterRejected(octacity_server_store::FactoryOutboxFailure::Unsupported)
          }
          FailureClass::Cancelled => ApplicationError::unavailable(),
          _ => ApplicationError::invalid(),
        },
      }),
      _ => Err(ApplicationError::invalid().into()),
    }
  }
}
#[async_trait::async_trait]
impl<S: FactorySourceStore + FactoryRunStore + IssueSourceProjectionStore> FactoryWorkStatusProjector
  for HostedFactoryWorkStatusProjector<S>
{
  fn supports_issue_delivery(&self) -> bool {
    self.profiles.iter().any(|p| p.draft.issue_delivery.is_some())
  }
  fn issue_delivery_policy(&self, intent: &IssuePushIntent) -> Result<IssueReporterDeliveryPolicy, ApplicationError> {
    self
      .issue_profile(intent)?
      .draft
      .issue_delivery
      .ok_or_else(ApplicationError::capability_unavailable)
  }
  async fn set_issue_status(
    &self,
    request: IssueStatusProjectionRequest,
  ) -> Result<(FactoryDigest, BudgetUsage), ApplicationError> {
    let started = tokio::time::Instant::now();
    let end = self.projection_end(request.deadline, request.budget, started)?;
    let (profile, intent) = tokio::time::timeout_at(end, self.issue_material(&request))
      .await
      .map_err(|_| ApplicationError::unavailable())??;
    self
      .project_ordered(&OrderedRequest::Issue(&request), &profile, intent, started, end)
      .await
  }
  async fn set_status(
    &self,
    request: FactoryWorkStatusRequest,
  ) -> Result<(FactoryDigest, BudgetUsage), ApplicationError> {
    let started = tokio::time::Instant::now();
    let end = self.projection_end(request.deadline, request.budget, started)?;
    let (profile, intent) = tokio::time::timeout_at(end, self.material(&request))
      .await
      .map_err(|_| ApplicationError::unavailable())??;
    self
      .project_ordered(&OrderedRequest::Work(&request), &profile, intent, started, end)
      .await
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
