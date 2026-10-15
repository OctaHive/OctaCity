//! Installed routing Signal executor over common typed node records.
use crate::*;
use async_trait::async_trait;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::{FactoryDecisionSignalReceiptStore, FactoryDecisionSignalRequestStore};
use std::sync::Arc;
use uuid::Uuid;

/// One explicit mapping from code-owned consumption to a declared node result.
#[derive(Clone)]
pub struct FactoryDecisionSignalNodeOutput {
  /// Typed final disposition, never provider prose or raw probability.
  pub disposition: DecisionSignalDisposition,
  /// Outcome declared by the immutable Flow node.
  pub outcome: FactoryKey,
  /// Schema-checked projection of safe retained Signal facts.
  pub projection: FlowDataMapping,
}
/// Exact installed routing action and independently narrowed authority.
#[derive(Clone)]
pub struct FactoryDecisionSignalNodeBinding {
  /// Exact plugin, operation and parameters selected by the node.
  pub action: FlowActionBinding,
  /// Project owning the installed capability and credentials.
  pub project_id: ProjectId,
  /// Frozen provider preparation and redaction contract.
  pub plan: FactoryDecisionSignalRequestPlan,
  /// Baseline preserved by shadow and advisory modes.
  pub baseline_route: FactoryKey,
  /// Dedicated logical Decision Signal credential profile.
  pub credential_profile: FactoryKey,
  /// Exact trusted provider host authorized by the deployment.
  pub network_host: octacity_server_factory::NetworkHost,
  /// Project-owned Factory authority ceiling.
  pub project_permissions: FactoryPermissionSet,
  /// Independently enforceable deployment ceiling.
  pub local_permissions: LocalPermissionCeiling,
  /// Explicit finite outcome mappings; missing dispositions fail closed.
  pub outputs: Vec<FactoryDecisionSignalNodeOutput>,
}
const POLICY_DIGEST_PARAMETER: &str = "installed_policy_digest";
/// One exact installed action with its independently credential-resolved provider.
#[derive(Clone)]
pub struct FactoryDecisionSignalNodeProfile {
  /// Trusted provider instance; different Projects may use distinct credentials.
  pub provider: Arc<dyn DecisionSignalProvider>,
  /// Complete pinned action and authority selected by the admitted node.
  pub binding: FactoryDecisionSignalNodeBinding,
}
impl FactoryDecisionSignalNodeBinding {
  /// Pins the complete installed plan, consumption mapping and authority into
  /// the action selected when publishing a Flow. Old and new policies must
  /// have different action parameters and can remain installed together.
  pub fn pin_policy(mut self) -> Result<Self, ApplicationError> {
    let digest = self.policy_digest()?;
    let mut parameters = self.action.parameters().clone();
    parameters
      .as_object_mut()
      .ok_or_else(ApplicationError::invalid)?
      .insert(POLICY_DIGEST_PARAMETER.to_owned(), serde_json::json!(digest));
    self.action = FlowActionBinding::new(self.action.plugin().clone(), self.action.action().clone(), parameters)
      .map_err(|_| ApplicationError::invalid())?;
    Ok(self)
  }
  fn policy_digest(&self) -> Result<FactoryDigest, ApplicationError> {
    if self.outputs.is_empty() || self.outputs.len() > MAX_DECISION_SIGNAL_CHOICES + 2 {
      return Err(ApplicationError::invalid());
    }
    let mut parameters = self.action.parameters().clone();
    parameters
      .as_object_mut()
      .ok_or_else(ApplicationError::invalid)?
      .remove(POLICY_DIGEST_PARAMETER);
    let outputs = self
      .outputs
      .iter()
      .map(|output| (&output.disposition, &output.outcome, &output.projection))
      .collect::<Vec<_>>();
    let enforcement = PermissionCategory::ALL
      .into_iter()
      .filter(|category| self.local_permissions.can_enforce(*category))
      .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&(
      self.action.plugin(),
      self.action.action(),
      parameters,
      self.project_id,
      &self.plan,
      &self.baseline_route,
      &self.credential_profile,
      self.network_host.as_str(),
      &self.project_permissions,
      self.local_permissions.grants(),
      enforcement,
      outputs,
    ))
    .map_err(|_| ApplicationError::invalid())?;
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(ApplicationError::invalid());
    }
    Ok(FactoryDigest::sha256(
      "octacity.factory.signal-node-policy.v1",
      &[&bytes],
    ))
  }
}
/// Dispatches an exact installed routing profile under durable native authorization.
/// Tool-risk execution requires its separately verified deterministic authorization.
pub struct FactoryDecisionSignalNodeExecutor<S> {
  store: Arc<S>,
  profiles: Vec<FactoryDecisionSignalNodeProfile>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S> FactoryDecisionSignalNodeExecutor<S> {
  /// Installs routing only; no Flow or node name selects a provider or outcome.
  pub fn new(
    store: Arc<S>,
    profiles: Vec<FactoryDecisionSignalNodeProfile>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Result<Self, ApplicationError> {
    if profiles.is_empty() || profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS {
      return Err(ApplicationError::invalid());
    }
    for (index, profile) in profiles.iter().enumerate() {
      let binding = &profile.binding;
      let digest = binding.policy_digest()?;
      if profiles[..index]
        .iter()
        .any(|previous| previous.binding.project_id == binding.project_id && previous.binding.action == binding.action)
        || binding.plan.profile.purpose() != DecisionSignalPurpose::Routing
        || *profile.provider.capability() != binding.plan.capability
        || binding.action.plugin() != profile.provider.capability().adapter()
        || binding.action.parameters().get(POLICY_DIGEST_PARAMETER) != Some(&serde_json::json!(digest))
        || binding
          .plan
          .profile
          .routes()
          .is_none_or(|routes| !routes.choices().contains(&binding.baseline_route))
      {
        return Err(ApplicationError::invalid());
      }
      for (index, output) in binding.outputs.iter().enumerate() {
        if binding.outputs[..index]
          .iter()
          .any(|previous| previous.disposition == output.disposition)
        {
          return Err(ApplicationError::invalid());
        }
      }
    }
    Ok(Self { store, profiles, clock })
  }
  fn request<'a>(
    &'a self,
    request: &FactoryNodeExecutionRequest<'_>,
  ) -> Result<(&'a FactoryDecisionSignalNodeProfile, DecisionSignalProviderRequest), ApplicationError> {
    let node = request
      .admitted
      .closure()
      .definition(request.input.definition())
      .and_then(|definition| definition.node(request.input.node()))
      .ok_or_else(ApplicationError::invalid)?;
    let profile = self
      .profiles
      .iter()
      .find(|profile| {
        node.action() == Some(&profile.binding.action)
          && request.input.subject().project_id() == profile.binding.project_id
      })
      .ok_or_else(ApplicationError::invalid)?;
    let binding = &profile.binding;
    let permissions = node.permissions();
    if request.run_id != request.input.factory_run_id()
      || request.input.subject().project_id() != binding.project_id
      || node.kind() != FlowNodeKind::DecisionSignal
      || request.attempt.node_kind() != node.kind()
      || node.action() != Some(&binding.action)
      || request.attempt.execution() != &NodeExecutionIdentity::External(binding.action.plugin().clone())
      || !permissions.plugins().any(|plugin| plugin == binding.action.plugin())
      || !permissions
        .secret_profiles()
        .any(|profile| profile == &binding.credential_profile)
      || !permissions.network_hosts().any(|host| host == &binding.network_host)
    {
      return Err(ApplicationError::invalid());
    }
    let effective = resolve_factory_permissions(
      &binding.project_permissions,
      request.admitted.limits().permissions(),
      permissions,
      &binding.local_permissions,
    )
    .map_err(|_| ApplicationError::invalid())?;
    if &effective != permissions {
      return Err(ApplicationError::invalid());
    }
    for output in &binding.outputs {
      if node
        .outcome(&output.outcome)
        .and_then(|outcome| request.admitted.data_schema(outcome.schema()))
        .is_none()
      {
        return Err(ApplicationError::invalid());
      }
    }
    let prepared = binding
      .plan
      .prepare(request.input, request.attempt)
      .map_err(|_| ApplicationError::invalid())?;
    Ok((profile, prepared))
  }
}
#[async_trait]
impl<S: FactoryDecisionSignalReceiptStore + FactoryDecisionSignalRequestStore + 'static> FactoryNodeExecutor
  for FactoryDecisionSignalNodeExecutor<S>
{
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    let (profile, prepared) = self.request(&request)?;
    let binding = &profile.binding;
    let now = request.observed_at.max(self.clock.now()?);
    request
      .attempt
      .verify_observer(&request.ownership, now)
      .map_err(|_| ApplicationError::invalid())?;
    let receipts = Arc::new(FactoryDecisionSignalReceipts::new(
      self.store.clone(),
      request.ownership.clone(),
      self.clock.clone(),
    ));
    let service = DecisionSignalService::for_provider_with_factory_clock(
      profile.provider.clone(),
      receipts.clone(),
      receipts.clone(),
      receipts,
      self.clock.clone(),
    );
    let receipt_id = DecisionSignalReceiptId::from_uuid(Uuid::new_v5(&prepared.id().as_uuid(), b"consumed-receipt:v1"))
      .map_err(|_| ApplicationError::invalid())?;
    let receipt = match service
      .route(receipt_id, &prepared, binding.baseline_route.clone())
      .await
    {
      Ok(receipt) => receipt,
      Err(DecisionSignalApplicationError::AwaitingProviderObservation) => {
        return Ok(FactoryNodeExecutionStep::Waiting { execution: None });
      }
      Err(_) => return Err(ApplicationError::unavailable()),
    };
    let output = binding
      .outputs
      .iter()
      .find(|output| &output.disposition == receipt.consumption().final_disposition())
      .ok_or_else(ApplicationError::invalid)?;
    let definition = request
      .admitted
      .closure()
      .definition(request.input.definition())
      .ok_or_else(ApplicationError::invalid)?;
    let schema = definition
      .node(request.input.node())
      .and_then(|node| node.outcome(&output.outcome))
      .and_then(|outcome| request.admitted.data_schema(outcome.schema()))
      .ok_or_else(ApplicationError::invalid)?;
    let facts = FlowDiscoveryFacts::decision_signal(&receipt);
    let value = serde_json::json!({"signal": facts});
    let payload = output
      .projection
      .project_value(&value, schema)
      .map_err(|_| ApplicationError::invalid())?;
    let observed_at = now.max(self.clock.now()?);
    let record = FlowNodeRecord::new(
      request.input,
      request.attempt,
      definition,
      FlowRecordObservation {
        outcome: output.outcome.clone(),
        payload,
        producer: request.attempt.execution().clone(),
        observed_at,
      },
    )
    .and_then(|record| record.with_discovery(facts))
    .map_err(|_| ApplicationError::invalid())?;
    Ok(FactoryNodeExecutionStep::Completed {
      record: Box::new(record),
      usage: receipt.usage(),
      execution: None,
      outbox: vec![],
    })
  }
  async fn cancel(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
    _: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    let (_, prepared) = self.request(&request)?;
    let now = request.observed_at.max(self.clock.now()?);
    request
      .attempt
      .verify_observer(&request.ownership, now)
      .map_err(|_| ApplicationError::invalid())?;
    if let Some(receipt) = self.store.factory_decision_signal_receipt(&prepared).await? {
      return Ok(FactoryNodeCancellationStep::Stopped {
        execution: None,
        usage: BudgetUsage {
          attempts: 0,
          ..receipt.usage()
        },
      });
    }
    if !self.store.factory_decision_signal_request_reserved(&prepared).await? {
      return Ok(FactoryNodeCancellationStep::Stopped {
        execution: None,
        usage: BudgetUsage::default(),
      });
    }
    Ok(FactoryNodeCancellationStep::Waiting { execution: None })
  }
}
