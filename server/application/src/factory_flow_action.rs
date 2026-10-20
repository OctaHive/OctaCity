//! Plugin extensions of configured trusted-action nodes over the common node journal.

use crate::ApplicationError;
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use std::sync::Arc;

/// Scoped plugin input with no parent transcript, credentials or routing authority.
pub struct FactoryFlowActionRequest<'a> {
  /// Stable identity; plugins must deduplicate effects across retries and restart.
  pub operation_id: FactoryDigest,
  /// Internal Work identity.
  pub work_id: WorkEnvelopeId,
  /// Exact persisted attempt accepted by the common action adapter.
  pub node_attempt_id: NodeAttemptId,
  /// Provider-neutral source identity used by the selected connector.
  pub external_identity: &'a ExternalWorkIdentity,
  /// Exact admitted source subject.
  pub subject: &'a ExactSubject,
  /// Exact action and operator-published parameters.
  pub binding: &'a FlowActionBinding,
  /// Exact explicitly projected input; plugins receive no other node data.
  pub input: &'a FlowNodeInput,
  /// Published contracts for the finite declared outputs.
  pub schemas: &'a [FlowDataSchema],
  /// Maximum authority; the plugin must enforce it before any effect.
  pub permissions: &'a FactoryPermissionSet,
  /// Hard resource ceiling; the plugin must enforce it during execution.
  pub budget: BudgetLimit,
  /// Authoritative stop deadline, never extended by a retry.
  pub deadline: Timestamp,
  /// Persisted attempt start; retries do not reset elapsed accounting.
  pub started_at: Timestamp,
  /// Current fenced observer window, independent of the original hard deadline.
  pub authority_expires_at: Timestamp,
  /// Trusted observation time used to enforce input validity before effects.
  pub observed_at: Timestamp,
}

/// Trusted plugin execution facts; the Flow validates the declared outcome again.
pub struct FactoryFlowActionObservation {
  /// Verified domain facts for safe discovery, retained with the fenced result.
  /// Provider observations and arbitrary JSON cannot populate this projection.
  pub discovery: Option<FlowDiscoveryFacts>,
  /// One outcome declared by the immutable node.
  pub outcome: FactoryKey,
  /// Bounded result including the plugin's idempotent receipt, under a published contract.
  pub payload: FlowPayload,
  /// Measured resources; attempt creation is charged by the Flow owner.
  pub usage: BudgetUsage,
  /// Deferred effects; accepting the result never requires source availability.
  pub outbox: Vec<crate::FactoryNodeOutboxEffect>,
}

/// Server-installed extension of `trusted_action`, never a new primitive kind.
#[async_trait]
pub trait FactoryFlowActionPlugin: Send + Sync {
  /// Returns the exact installed plugin version and content identity.
  fn identity(&self) -> ImmutableReference;
  /// Validates action parameters and every possible result before external effects.
  fn validate(&self, binding: &FlowActionBinding, outcomes: &[FlowOutcomeDefinition]) -> Result<(), ApplicationError>;
  /// Stops or observes only the original operation; never dispatches an absent one.
  /// None means stopping is pending; Some contains the final measured consumption.
  async fn cancel(
    &self,
    request: FactoryFlowActionRequest<'_>,
    requested_at: Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError>;
  /// Executes idempotently within the supplied permissions, budget and deadline.
  async fn execute(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryFlowActionObservation, ApplicationError>;
}

/// Validates a generic attempt and invokes its exact installed action plugin.
pub struct FactoryFlowActionAdapter {
  plugin: Arc<dyn FactoryFlowActionPlugin>,
}
impl FactoryFlowActionAdapter {
  /// Selects an installed plugin; definition binding and attempt identity are checked on every call.
  #[must_use]
  pub fn new(plugin: Arc<dyn FactoryFlowActionPlugin>) -> Self {
    Self { plugin }
  }
  fn scoped_request<'a>(
    &self,
    request: &crate::FactoryNodeExecutionRequest<'a>,
    executing: bool,
  ) -> Result<FactoryFlowActionRequest<'a>, ApplicationError> {
    let definition = request
      .admitted
      .closure()
      .definition(request.input.definition())
      .ok_or_else(ApplicationError::invalid)?;
    let node = definition
      .node(request.attempt.node_key())
      .ok_or_else(ApplicationError::invalid)?;
    let binding = node.action().ok_or_else(ApplicationError::invalid)?;
    if request.input.factory_run_id() != request.run_id
      || request.input.node() != request.attempt.node_key()
      || request.input.digest().map_err(|_| ApplicationError::invalid())? != request.attempt.input_digest()
      || node.kind() != FlowNodeKind::TrustedAction
      || request.attempt.node_kind() != node.kind()
      || request.attempt.budget() != node.budget()
      || executing && request.observed_at >= request.attempt.deadline()
      || binding.plugin() != &self.plugin.identity()
      || !node.permissions().plugins().any(|allowed| allowed == binding.plugin())
      || request.attempt.execution() != &NodeExecutionIdentity::External(binding.plugin().clone())
    {
      return Err(ApplicationError::invalid());
    }
    request
      .attempt
      .verify_observer(&request.ownership, request.observed_at)
      .map_err(|_| ApplicationError::invalid())?;
    self.plugin.validate(binding, node.outcomes())?;
    let bytes = serde_json::to_vec(&(
      request.work.id(),
      request.work.external_identity(),
      request.work.subject(),
      binding,
      definition.reference(),
      request.attempt.input_digest(),
    ))
    .map_err(|_| ApplicationError::invalid())?;
    let operation_id = FactoryDigest::sha256(
      "octacity.factory.flow-action.v1",
      &[request.attempt.id().as_uuid().as_bytes(), &bytes],
    );
    Ok(FactoryFlowActionRequest {
      operation_id,
      work_id: request.work.id(),
      node_attempt_id: request.attempt.id(),
      external_identity: request.work.external_identity(),
      subject: request.work.subject(),
      binding,
      input: request.input,
      schemas: request.admitted.data_schemas(),
      permissions: node.permissions(),
      budget: node.budget(),
      deadline: request.attempt.deadline(),
      started_at: request.attempt.started_at(),
      authority_expires_at: request.ownership.claim().expires_at(),
      observed_at: request.observed_at,
    })
  }
}

#[async_trait]
impl crate::FactoryNodeExecutor for FactoryFlowActionAdapter {
  async fn cancel(
    &self,
    request: crate::FactoryNodeExecutionRequest<'_>,
    requested_at: Timestamp,
  ) -> Result<crate::FactoryNodeCancellationStep, ApplicationError> {
    if requested_at > request.observed_at {
      return Err(ApplicationError::invalid());
    }
    let scoped = self.scoped_request(&request, false)?;
    match self.plugin.cancel(scoped, requested_at).await? {
      None => Ok(crate::FactoryNodeCancellationStep::Waiting { execution: None }),
      Some(usage) if usage.attempts == 0 => Ok(crate::FactoryNodeCancellationStep::Stopped { execution: None, usage }),
      Some(_) => Err(ApplicationError::invalid()),
    }
  }

  async fn observe(
    &self,
    request: crate::FactoryNodeExecutionRequest<'_>,
  ) -> Result<crate::FactoryNodeExecutionStep, ApplicationError> {
    let scoped = self.scoped_request(&request, true)?;
    let definition = request
      .admitted
      .closure()
      .definition(request.input.definition())
      .ok_or_else(ApplicationError::invalid)?;
    let result = self.plugin.execute(scoped).await?;
    if result.usage.attempts != 0 {
      return Err(ApplicationError::invalid());
    }
    let schema = request
      .admitted
      .data_schema(result.payload.schema())
      .ok_or_else(ApplicationError::invalid)?;
    FlowPayload::restore(
      &serde_json::to_vec(&result.payload).map_err(|_| ApplicationError::invalid())?,
      schema,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let mut record = FlowNodeRecord::new(
      request.input,
      request.attempt,
      definition,
      FlowRecordObservation {
        outcome: result.outcome,
        payload: result.payload,
        producer: request.attempt.execution().clone(),
        observed_at: request.observed_at,
      },
    )
    .map_err(|_| ApplicationError::invalid())?;
    if let Some(facts) = result.discovery {
      record = record.with_discovery(facts).map_err(|_| ApplicationError::invalid())?;
    }
    Ok(crate::FactoryNodeExecutionStep::Completed {
      execution: None,
      record: Box::new(record),
      usage: result.usage,
      outbox: result.outbox,
    })
  }
}

/// Configurable, non-authoritative projection of a Work status into its source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryWorkStatusRequest {
  /// Idempotency key for one exact Flow action.
  pub operation_id: FactoryDigest,
  /// Current independently fenced outbox dispatch authority; absent for synchronous actions.
  /// Retries retain operation identity while receiving a new claim.
  pub outbox_claim: Option<FactoryClaim>,
  /// Internal Work identity.
  pub work_id: WorkEnvelopeId,
  /// Source identity understood only by the selected connector.
  pub external_identity: ExternalWorkIdentity,
  /// Exact admitted Project, Repository and revision.
  pub subject: ExactSubject,
  /// Operator-chosen status; no research-outcome vocabulary is imposed.
  pub status: FactorySafeText,
  /// Exact frozen authority; the connector checks hosts and credential profiles before effects.
  pub permissions: FactoryPermissionSet,
  /// Hard resource ceiling inherited from the action node.
  pub budget: BudgetLimit,
  /// Authoritative execution deadline.
  pub deadline: Timestamp,
}

/// Connector boundary: GitHub projects a status to a label; Jira to a native status.
///
/// Implementations deduplicate by operation identity, retain a receipt, enforce
/// the supplied permissions and execution bounds before protected operations,
/// and return its digest plus measured resource usage. Unknown or unenforceable
/// authority fails closed; credentials come only from allowed profiles.
/// Projection failure cannot undo an accepted node result or grant authority.
#[async_trait]
pub trait FactoryWorkStatusProjector: Send + Sync {
  /// Observes/stops the existing projection without replaying writes or creating it.
  /// A terminal observation returns measured usage; None requires another observation.
  async fn cancel_status(
    &self,
    request: FactoryWorkStatusRequest,
    requested_at: Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError>;
  /// Projects the arbitrary configured value using trusted connector credentials.
  async fn set_status(
    &self,
    request: FactoryWorkStatusRequest,
  ) -> Result<(FactoryDigest, BudgetUsage), ApplicationError>;
}

/// One reusable action plugin; additional actions implement the same generic port.
pub struct FactoryWorkStatusAction<P> {
  identity: ImmutableReference,
  projector: Arc<P>,
  outcome: FlowOutcomeDefinition,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StatusParameters {
  pub(crate) status: FactorySafeText,
  #[serde(default)]
  pub(crate) source_update: Option<crate::FactoryStageSourceUpdate>,
  #[serde(default)]
  pub(crate) outbox: Option<FactoryReporterRetryPolicy>,
}
impl StatusParameters {
  pub(crate) fn from_binding(binding: &FlowActionBinding) -> Result<Self, ApplicationError> {
    let parameters: Self = serde_json::from_value(binding.parameters().clone())
      .map_err(|_| crate::FactoryValidationDiagnostics::invalid("/action/parameters"))?;
    parameters.validate(binding)?;
    Ok(parameters)
  }
  pub(crate) fn validate(&self, binding: &FlowActionBinding) -> Result<(), ApplicationError> {
    if let Some(policy) = self.outbox {
      policy.validate()?;
    }
    if let Some(update) = &self.source_update {
      update.validate()?;
      if self.outbox.is_none()
        || update.effects.status != self.status.as_str()
        || &update.reporter.capability != binding.plugin()
      {
        return Err(ApplicationError::invalid());
      }
    }
    Ok(())
  }
}
/// Published bounded retry policy for non-authoritative source projection.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryReporterRetryPolicy {
  /// Maximum actual dispatches, including the first.
  pub max_attempts: u16,
  /// Delay between bounded dispatches; it cannot extend the original deadline.
  pub retry_delay_millis: u64,
}
impl FactoryReporterRetryPolicy {
  /// Maximum published projection attempts, shared with the management schema.
  pub const MAX_ATTEMPTS: u16 = 1024;
  /// Maximum published delay; retries still cannot extend the original deadline.
  pub const MAX_RETRY_DELAY_MILLIS: u64 = 3_600_000;
  pub(crate) fn validate(self) -> Result<(), ApplicationError> {
    if self.max_attempts == 0
      || self.max_attempts > Self::MAX_ATTEMPTS
      || self.retry_delay_millis == 0
      || self.retry_delay_millis > Self::MAX_RETRY_DELAY_MILLIS
    {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
}
impl<P> FactoryWorkStatusAction<P> {
  /// Binds an exact installed version, source connector and declared successful result.
  #[must_use]
  pub fn new(identity: ImmutableReference, projector: Arc<P>, outcome: FlowOutcomeDefinition) -> Self {
    Self {
      identity,
      projector,
      outcome,
    }
  }
}
#[async_trait]
impl<P: FactoryWorkStatusProjector> FactoryFlowActionPlugin for FactoryWorkStatusAction<P> {
  fn identity(&self) -> ImmutableReference {
    self.identity.clone()
  }
  fn validate(&self, binding: &FlowActionBinding, outcomes: &[FlowOutcomeDefinition]) -> Result<(), ApplicationError> {
    if binding.action().as_str() != "set_status" || !outcomes.contains(&self.outcome) {
      return Err(ApplicationError::invalid());
    }
    let parameters = serde_json::from_value::<StatusParameters>(binding.parameters().clone())
      .map_err(|_| ApplicationError::invalid())?;
    parameters.validate(binding)
  }
  async fn cancel(
    &self,
    request: FactoryFlowActionRequest<'_>,
    requested_at: Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    let parameters: StatusParameters =
      serde_json::from_value(request.binding.parameters().clone()).map_err(|_| ApplicationError::invalid())?;
    if parameters.outbox.is_some() {
      return Ok(Some(BudgetUsage::default()));
    }
    self
      .projector
      .cancel_status(
        FactoryWorkStatusRequest {
          operation_id: request.operation_id,
          outbox_claim: None,
          work_id: request.work_id,
          external_identity: request.external_identity.clone(),
          subject: request.subject.clone(),
          status: parameters.status,
          permissions: request.permissions.clone(),
          budget: request.budget,
          deadline: request.deadline,
        },
        requested_at,
      )
      .await
  }
  async fn execute(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryFlowActionObservation, ApplicationError> {
    let parameters: StatusParameters =
      serde_json::from_value(request.binding.parameters().clone()).map_err(|_| ApplicationError::invalid())?;
    if let Some(policy) = parameters.outbox {
      policy.validate()?;
      return Ok(FactoryFlowActionObservation {
        discovery: None,
        outcome: self.outcome.key().clone(),
        payload: FlowPayload::new(
          request
            .schemas
            .iter()
            .find(|schema| schema.reference() == self.outcome.schema())
            .ok_or_else(ApplicationError::invalid)?,
          serde_json::json!({"receipt":request.operation_id}),
        )
        .map_err(|_| ApplicationError::invalid())?,
        usage: BudgetUsage::default(),
        outbox: vec![crate::FactoryNodeOutboxEffect {
          operation_id: request.operation_id,
          kind: FactoryKey::new("work.status.projection").map_err(|_| ApplicationError::invalid())?,
        }],
      });
    }
    let (digest, usage) = self
      .projector
      .set_status(FactoryWorkStatusRequest {
        operation_id: request.operation_id,
        outbox_claim: None,
        work_id: request.work_id,
        external_identity: request.external_identity.clone(),
        subject: request.subject.clone(),
        status: parameters.status,
        permissions: request.permissions.clone(),
        budget: request.budget,
        deadline: request.deadline,
      })
      .await?;
    Ok(FactoryFlowActionObservation {
      discovery: None,
      outcome: self.outcome.key().clone(),
      payload: FlowPayload::new(
        request
          .schemas
          .iter()
          .find(|schema| schema.reference() == self.outcome.schema())
          .ok_or_else(ApplicationError::invalid)?,
        serde_json::json!({"receipt":digest}),
      )
      .map_err(|_| ApplicationError::invalid())?,
      usage,
      outbox: vec![],
    })
  }
}
