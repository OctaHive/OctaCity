//! Immutable provider-neutral installation for lease-bound tool-risk signals.
use crate::{
  AgentToolActionError, DecisionSignalDispatchAuthorization, DecisionSignalPortError, DecisionSignalReceiptPublication,
  DecisionSignalRequestObservation, DecisionSignalService, FactoryExecutionClock, LeaseBoundToolActionAuthorizer,
  LeaseBoundToolActionInput,
};
use crate::{ApplicationError, FactoryDecisionSignalRequestPlan};
use async_trait::async_trait;
use octacity_protocol::{
  FactoryToolActionDecisionSourceV3, FactoryToolActionDecisionV3, FactoryToolActionDispositionV3,
  FactoryToolControlModeV3,
};
use octacity_server_domain::ProjectId;
use octacity_server_domain::{AttemptNumber, JobId};
use octacity_server_factory::{
  BudgetLimit, DecisionSignalDigests, DecisionSignalInputMedia, DecisionSignalProviderInput,
  DecisionSignalProviderRequest, DecisionSignalPurpose, DecisionSignalRequest, DecisionSignalRequestId, FactoryDigest,
  FactoryKey, FactoryPermissionSet, FactoryText, FlowNodeInput, LocalPermissionCeiling, MAX_FACTORY_ATTEMPTS,
  MAX_FLOW_DATA_BYTES, NetworkHost, NodeAttempt, PermissionCategory, ToolRiskChoiceMapping,
};
use octacity_server_factory::{
  BuildConfigurationRef, DecisionSignalDisposition, DecisionSignalProvider, DecisionSignalReceipt,
  DecisionSignalReceiptId, FactoryClaimOwnership, FlowBuildIntent,
};
use octacity_server_job::JobState;
use octacity_server_store::{
  BuildOrigin, BuildQueryStore, FactoryBuildAcceptanceStore, FactoryDecisionSignalReceiptStore,
  FactoryDecisionSignalRequestStore, FactoryRunSnapshot, FactoryRunStore, JobExecutionStore, LeaseAccess,
  PublishFactoryDecisionSignalReceipt, ReserveFactoryDecisionSignalRequest,
};
use std::sync::Arc;

/// Exact installed workload-policy pair. No logical Flow or Node name selects
/// the authorizer; immutable task and Build Configuration identities do.
#[derive(Clone)]
pub struct FactoryToolRiskNodeProfile {
  /// Installed connector, optionally decorated with the runtime call timeout.
  pub provider: Arc<dyn DecisionSignalProvider>,
  /// Complete server-owned provider authority and consuming policy.
  pub binding: FactoryToolRiskBinding,
  /// Opaque workload contract before adding the complete provider policy pin.
  pub base_task_digest: FactoryDigest,
  /// Exact ordinary Build Configuration admitted for this workload.
  pub build_configuration: BuildConfigurationRef,
}

/// Native authority composition for broker-validated blocked actions. The Agent
/// remains responsible for proving and rechecking the exact local envelope;
/// this service never reconstructs private action paths from summary counts.
pub struct FactoryToolRiskAuthorizer<R, Q, A, L, S> {
  authority: Arc<Authority<R, Q, A, L>>,
  signals: Arc<S>,
}
struct Authority<R, Q, A, L> {
  runs: Arc<R>,
  builds: Arc<Q>,
  acceptance: Arc<A>,
  leases: Arc<L>,
  profiles: Vec<FactoryToolRiskNodeProfile>,
  clock: Arc<dyn FactoryExecutionClock>,
}
struct Proposal {
  lease: LeaseAccess,
  job_id: JobId,
  attempt: AttemptNumber,
  digest: FactoryDigest,
  summary: octacity_protocol::FactoryToolActionSummaryV3,
}
struct Context {
  intent: FlowBuildIntent,
  ownership: FactoryClaimOwnership,
  profile: usize,
}

impl<R, Q, A, L, S> FactoryToolRiskAuthorizer<R, Q, A, L, S> {
  /// Installs complete exact profiles over ordinary execution and common journals.
  /// The Signal store may be the same native adapter as the Run store.
  pub fn new(
    runs: Arc<R>,
    builds: Arc<Q>,
    acceptance: Arc<A>,
    leases: Arc<L>,
    signals: Arc<S>,
    profiles: Vec<FactoryToolRiskNodeProfile>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Result<Self, ApplicationError> {
    if profiles.is_empty() || profiles.len() > 64 {
      return Err(ApplicationError::invalid());
    }
    let mut identities = std::collections::BTreeSet::new();
    for profile in &profiles {
      let binding = &profile.binding;
      let policy = binding.policy_digest()?;
      if binding.project_id != profile.build_configuration.project_id()
        || profile.provider.capability() != &binding.plan.capability
        || !provider_granted(&binding.project_permissions, binding)
        || !provider_granted(binding.local_permissions.grants(), binding)
        || [
          PermissionCategory::PluginIdentity,
          PermissionCategory::NetworkHosts,
          PermissionCategory::SecretProfiles,
        ]
        .into_iter()
        .any(|category| !binding.local_permissions.can_enforce(category))
        || !identities.insert((
          binding.project_id,
          factory_tool_risk_task_digest(profile.base_task_digest, policy),
          profile.build_configuration.id(),
          profile.build_configuration.version(),
        ))
      {
        return Err(ApplicationError::invalid());
      }
    }
    Ok(Self {
      authority: Arc::new(Authority {
        runs,
        builds,
        acceptance,
        leases,
        profiles,
        clock,
      }),
      signals,
    })
  }
}
fn provider_granted(grants: &FactoryPermissionSet, binding: &FactoryToolRiskBinding) -> bool {
  grants.plugins().any(|p| p == binding.plan.profile.adapter())
    && grants.network_hosts().any(|host| host == &binding.network_host)
    && grants
      .secret_profiles()
      .any(|credential| credential == &binding.credential_profile)
}
impl<R: FactoryRunStore, Q: BuildQueryStore, A: FactoryBuildAcceptanceStore, L: JobExecutionStore>
  Authority<R, Q, A, L>
{
  async fn context(&self, proposal: &Proposal) -> Result<Context, AgentToolActionError> {
    let at = self.clock.now().map_err(|_| AgentToolActionError::Unavailable)?;
    self
      .leases
      .authorize_tool_action_lease(proposal.lease, proposal.job_id, proposal.attempt, at)
      .await?;
    let job = self.builds.job(proposal.job_id).await?;
    if !matches!(job.state, JobState::Leased | JobState::Running)
      || job.terminal.is_some()
      || job.assignment.is_none_or(|a| a.agent_id != proposal.lease.agent_id)
    {
      return Err(AgentToolActionError::Unavailable);
    }
    let attempt = self.builds.attempt(job.attempt_id).await?;
    if attempt.number != proposal.attempt || !attempt.jobs.iter().any(|j| j == &job) {
      return Err(AgentToolActionError::Unavailable);
    }
    let latest = self.builds.latest_attempt(attempt.build_id).await?;
    if latest.id != attempt.id {
      return Err(AgentToolActionError::Unavailable);
    }
    let build = self.builds.build(attempt.build_id).await?;
    if !matches!(
      build.state,
      octacity_server_orchestrator::BuildState::Queued | octacity_server_orchestrator::BuildState::Running
    ) {
      return Err(AgentToolActionError::Unavailable);
    }
    let BuildOrigin::Factory(origin) = &build.origin else {
      return Err(AgentToolActionError::Unavailable);
    };
    let snapshot = self.runs.factory_run_snapshot(origin.run_id).await?;
    let intent = snapshot
      .flow
      .data
      .build_intents
      .iter()
      .find(|i| i.node().id() == origin.node_attempt_id)
      .ok_or(AgentToolActionError::Unavailable)?
      .clone();
    if octacity_server_store::FactoryBuildOrigin::from_intent(&intent).map_err(|_| AgentToolActionError::Unavailable)?
      != *origin
      || build.build.project_id != intent.input().subject().project_id()
      || build.build.repository_id != intent.input().subject().repository_id()
      || build.build.configuration_id != intent.profile().build_configuration.id()
      || build.build.configuration_version != intent.profile().build_configuration.version()
      || build.build.immutable_revision != *intent.input().subject().base_revision()
      || job.job.job_spec_template.source_revision() != &build.build.immutable_revision
      || job.job.job_spec_template.execution_deadline() != Some(intent.node().deadline())
    {
      return Err(AgentToolActionError::Unavailable);
    }
    let accepted = self
      .acceptance
      .factory_build_for_operation(origin.operation_id)
      .await?
      .ok_or(AgentToolActionError::Unavailable)?;
    if accepted.build_id != build.build.id || accepted.effective_policy_digest != intent.permissions().digest() {
      return Err(AgentToolActionError::Unavailable);
    }
    // Initial hook calls may arrive before the owner appends FlowBuildExecution.
    // The authoritative acceptance bridge already committed the complete DAG.
    if attempt.number == AttemptNumber::FIRST {
      if accepted.attempt_id != attempt.id || !accepted.job_ids.contains(&job.id()) {
        return Err(AgentToolActionError::Unavailable);
      }
    } else {
      // Retry hooks require an actual retained execution link and exact ancestry.
      if !snapshot.flow.data.build_executions.iter().any(|e| {
        e.operation_id() == origin.operation_id
          && e.attempt_id() == attempt.id
          && e.build_id() == build.build.id
          && e.jobs().contains(&job.id())
      }) {
        return Err(AgentToolActionError::Unavailable);
      }
      let mut predecessor = attempt.clone();
      let mut reached = false;
      for _ in 0..MAX_FACTORY_ATTEMPTS {
        if predecessor.id == accepted.attempt_id {
          reached = true;
          break;
        }
        let id = predecessor
          .retry_of_attempt_id
          .ok_or(AgentToolActionError::Unavailable)?;
        let prior = self.builds.attempt(id).await?;
        if prior.build_id != build.build.id || prior.number >= predecessor.number {
          return Err(AgentToolActionError::Unavailable);
        }
        predecessor = prior;
      }
      if !reached {
        return Err(AgentToolActionError::Unavailable);
      }
    }
    let managed = job
      .job
      .job_spec_template
      .managed_intent()
      .ok_or(AgentToolActionError::Unavailable)?;
    let control = managed
      .execution()
      .tool_control
      .as_ref()
      .ok_or(AgentToolActionError::Unavailable)?;
    let input = intent.input();
    let causality = managed.factory();
    if control.mode != FactoryToolControlModeV3::ToolRisk
      || control.plugin != intent.profile().plugin.identity().as_str()
      || causality.factory_run_id != input.factory_run_id().to_string()
      || causality.factory_configuration_id != input.configuration().id().to_string()
      || causality.factory_configuration_version != input.configuration().version().get()
      || causality.flow_run_id != input.flow_run_id().to_string()
      || causality.workflow_cycle_id != input.cycle_id().to_string()
      || causality.node_attempt_id != intent.node().id().to_string()
      || causality.input_digest != origin.input_digest.to_string()
      || causality.subject_digest != input.subject().digest().to_string()
      || managed.permissions()
        != &octacity_server_store::factory_job_permissions(intent.permissions())
          .map_err(|_| AgentToolActionError::Unavailable)?
    {
      return Err(AgentToolActionError::Unavailable);
    }
    let profile = self
      .profiles
      .iter()
      .position(|p| {
        p.binding.project_id == input.subject().project_id()
          && p.build_configuration == intent.profile().build_configuration
          && p
            .binding
            .policy_digest()
            .is_ok_and(|pin| factory_tool_risk_task_digest(p.base_task_digest, pin) == intent.profile().task_digest)
      })
      .ok_or(AgentToolActionError::Unavailable)?;
    let binding = &self.profiles[profile].binding;
    if !provider_granted(snapshot.admitted_flow.limits().permissions(), binding)
      || managed
        .permissions()
        .secret_profiles
        .iter()
        .any(|secret| secret == binding.credential_profile.as_str())
    {
      return Err(AgentToolActionError::Unavailable);
    }
    let ownership = current_ownership(&snapshot, &intent, at, true)?;
    Ok(Context {
      intent,
      ownership,
      profile,
    })
  }
}
fn current_ownership(
  snapshot: &FactoryRunSnapshot,
  intent: &FlowBuildIntent,
  at: octacity_server_domain::Timestamp,
  dispatch: bool,
) -> Result<FactoryClaimOwnership, AgentToolActionError> {
  let node = intent.node();
  let input = intent.input();
  if snapshot.run.id() != input.factory_run_id()
    || snapshot.work.subject() != input.subject()
    || snapshot.run.configuration() != input.configuration()
    || !snapshot.flow.data.inputs.iter().any(|i| i == input)
    || !snapshot.flow.attempts.iter().any(|n| n == node)
    || snapshot
      .flow
      .completions
      .iter()
      .any(|c| c.node_attempt_id() == node.id())
    || snapshot
      .flow
      .data
      .stops
      .iter()
      .any(|stop| stop.node_attempt_id() == node.id())
  {
    return Err(AgentToolActionError::Unavailable);
  }
  if dispatch
    && (snapshot.run.state().is_terminal()
      || at >= node.deadline()
      || snapshot
        .lifecycle_checkpoints
        .iter()
        .find(|c| c.id == snapshot.current.lifecycle_checkpoint_id)
        .is_none_or(|c| c.cancellation_requested)
      || snapshot
        .flow
        .cycles
        .iter()
        .filter(|c| c.flow_run_id() == input.flow_run_id())
        .max_by_key(|c| c.number())
        .is_none_or(|c| c.id() != input.cycle_id())
      || snapshot
        .flow
        .attempts
        .iter()
        .filter(|n| {
          n.flow_run_id() == node.flow_run_id()
            && n.workflow_cycle_id() == node.workflow_cycle_id()
            && n.node_key() == node.node_key()
        })
        .max_by_key(|n| n.number())
        .is_none_or(|n| n.id() != node.id()))
  {
    return Err(AgentToolActionError::Unavailable);
  }
  let claim = snapshot
    .current_claim
    .as_ref()
    .ok_or(AgentToolActionError::Unavailable)?;
  let ownership = FactoryClaimOwnership::new(claim.owner.clone(), claim.claim);
  node
    .verify_observer(&ownership, at)
    .map_err(|_| AgentToolActionError::Unavailable)?;
  Ok(ownership)
}
struct Ports<R, Q, A, L, S> {
  authority: Arc<Authority<R, Q, A, L>>,
  signals: Arc<S>,
  proposal: Proposal,
  intent: FlowBuildIntent,
  profile: usize,
}
#[async_trait]
impl<
  R: FactoryRunStore,
  Q: BuildQueryStore,
  A: FactoryBuildAcceptanceStore,
  L: JobExecutionStore,
  S: FactoryDecisionSignalRequestStore,
> DecisionSignalDispatchAuthorization for Ports<R, Q, A, L, S>
{
  async fn was_reserved(&self, r: &DecisionSignalProviderRequest) -> Result<bool, DecisionSignalPortError> {
    self
      .signals
      .factory_decision_signal_request_reserved(r)
      .await
      .map_err(port_error)
  }
  async fn authorize_dispatch(
    &self,
    r: &DecisionSignalProviderRequest,
  ) -> Result<octacity_server_store::FactoryDecisionSignalRequestDisposition, DecisionSignalPortError> {
    let context = self
      .authority
      .context(&self.proposal)
      .await
      .map_err(|_| DecisionSignalPortError::Conflict)?;
    if context.profile != self.profile || context.intent != self.intent {
      return Err(DecisionSignalPortError::Conflict);
    }
    self
      .signals
      .reserve_factory_decision_signal_request(ReserveFactoryDecisionSignalRequest {
        lease_guard: Some(octacity_server_store::FactoryDecisionSignalLeaseGuard {
          lease: self.proposal.lease,
          job_id: self.proposal.job_id,
          attempt: self.proposal.attempt,
        }),
        request: r.clone(),
        ownership: context.ownership,
        observed_at: self
          .authority
          .clock
          .now()
          .map_err(|_| DecisionSignalPortError::Unavailable)?,
        reservation_limit: Some(
          self.authority.profiles[self.profile]
            .binding
            .signal_allowance()
            .map_err(|_| DecisionSignalPortError::Conflict)?,
        ),
      })
      .await
      .map_err(port_error)
  }
}
#[async_trait]
impl<R: Send + Sync, Q: Send + Sync, A: Send + Sync, L: Send + Sync, S: FactoryDecisionSignalReceiptStore>
  DecisionSignalRequestObservation for Ports<R, Q, A, L, S>
{
  async fn observe_request(
    &self,
    r: &DecisionSignalProviderRequest,
  ) -> Result<Option<DecisionSignalReceipt>, DecisionSignalPortError> {
    self
      .signals
      .factory_decision_signal_receipt(r)
      .await
      .map_err(port_error)
  }
}
#[async_trait]
impl<R: FactoryRunStore, Q: Send + Sync, A: Send + Sync, L: Send + Sync, S: FactoryDecisionSignalReceiptStore>
  DecisionSignalReceiptPublication for Ports<R, Q, A, L, S>
{
  async fn publish_receipt(
    &self,
    receipt: DecisionSignalReceipt,
  ) -> Result<DecisionSignalReceipt, DecisionSignalPortError> {
    let at = self
      .authority
      .clock
      .now()
      .map_err(|_| DecisionSignalPortError::Unavailable)?;
    let snapshot = self
      .authority
      .runs
      .factory_run_snapshot(self.intent.input().factory_run_id())
      .await
      .map_err(port_error)?;
    // Publication settles the reserved external usage even after ordinary Job
    // cancellation. It grants no action or new provider-dispatch authority.
    let ownership =
      current_ownership(&snapshot, &self.intent, at, false).map_err(|_| DecisionSignalPortError::Conflict)?;
    self
      .signals
      .publish_factory_decision_signal_receipt(PublishFactoryDecisionSignalReceipt {
        receipt: receipt.clone(),
        ownership,
        observed_at: at,
      })
      .await
      .map_err(port_error)?;
    Ok(receipt)
  }
}
fn port_error(error: octacity_server_store::StoreError) -> DecisionSignalPortError {
  match error {
    octacity_server_store::StoreError::Unavailable => DecisionSignalPortError::Unavailable,
    _ => DecisionSignalPortError::Conflict,
  }
}
#[async_trait]
impl<
  R: FactoryRunStore + 'static,
  Q: BuildQueryStore + 'static,
  A: FactoryBuildAcceptanceStore + 'static,
  L: JobExecutionStore + 'static,
  S: FactoryDecisionSignalRequestStore + FactoryDecisionSignalReceiptStore + 'static,
> LeaseBoundToolActionAuthorizer for FactoryToolRiskAuthorizer<R, Q, A, L, S>
{
  async fn authorize(
    &self,
    input: LeaseBoundToolActionInput,
  ) -> Result<FactoryToolActionDecisionV3, AgentToolActionError> {
    let digest =
      FactoryDigest::from_lower_hex(&input.proposal_sha256).map_err(|_| AgentToolActionError::InvalidRequest)?;
    input
      .proposal_summary
      .validate()
      .map_err(|_| AgentToolActionError::InvalidRequest)?;
    let proposal = Proposal {
      lease: input.lease,
      job_id: input.job_id,
      attempt: input.attempt,
      digest,
      summary: input.proposal_summary,
    };
    let context = self.authority.context(&proposal).await?;
    let profile = &self.authority.profiles[context.profile];
    let request = profile
      .binding
      .prepare_request(
        context.intent.input(),
        context.intent.node(),
        proposal.digest,
        &proposal.summary,
      )
      .map_err(|_| AgentToolActionError::Unavailable)?;
    let ports = Arc::new(Ports {
      authority: self.authority.clone(),
      signals: self.signals.clone(),
      proposal,
      intent: context.intent,
      profile: context.profile,
    });
    let service = DecisionSignalService::for_provider_with_factory_clock(
      profile.provider.clone(),
      ports.clone(),
      ports.clone(),
      ports.clone(),
      self.authority.clock.clone(),
    );
    let id = DecisionSignalReceiptId::from_uuid(uuid::Uuid::new_v5(&request.id().as_uuid(), b"consumed-receipt:v1"))
      .map_err(|_| AgentToolActionError::Unavailable)?;
    let receipt = service
      .assess_tool_risk(id, &request, &profile.binding.mapping)
      .await
      .map_err(|_| AgentToolActionError::Unavailable)?;
    // Receipt replay and same-poll provider completion cannot revive a cancelled
    // Job, replaced fence or completed common Node.
    self.authority.context(&ports.proposal).await?;
    let disposition = match receipt.consumption().final_disposition() {
      DecisionSignalDisposition::Allow => FactoryToolActionDispositionV3::Allow,
      DecisionSignalDisposition::Escalate => FactoryToolActionDispositionV3::Escalate,
      _ => FactoryToolActionDispositionV3::Deny,
    };
    FactoryToolActionDecisionV3::new(
      digest.to_string(),
      disposition,
      FactoryToolActionDecisionSourceV3::DecisionSignal,
      Some(receipt.receipt_digest().to_string()),
    )
    .map_err(|_| AgentToolActionError::Unavailable)
  }
}

/// Installed server-only provider policy. Private question instructions and
/// credential selections are deliberately excluded from Debug formatting.
#[derive(Clone)]
pub struct FactoryToolRiskBinding {
  /// Project owning the provider capability and logical credential.
  pub project_id: ProjectId,
  /// Exact provider, model, questions, calibration and redaction contract.
  /// Tool-risk projects only the broker's redacted summary, never task input.
  pub plan: FactoryDecisionSignalRequestPlan,
  /// Complete finite mapping that may only preserve allow or narrow authority.
  pub mapping: ToolRiskChoiceMapping,
  /// Maximum externally charged calls reserved for one common node. A rejection
  /// before external I/O does not invent a charged call.
  pub max_calls: u16,
  /// Dedicated server-side provider credential, never projected to a Job.
  pub credential_profile: FactoryKey,
  /// Exact installed provider destination.
  pub network_host: NetworkHost,
  /// Project-owned provider authority independent of Agent execution grants.
  pub project_permissions: FactoryPermissionSet,
  /// Independently enforceable server deployment ceiling.
  pub local_permissions: LocalPermissionCeiling,
}

impl FactoryToolRiskBinding {
  /// Prepares one stable proposal-bound request from the redacted broker facts.
  /// Preparation grants no dispatch authority; native lease/fence checks and a
  /// durable reservation are still required before a provider is contacted.
  pub fn prepare_request(
    &self,
    input: &FlowNodeInput,
    node: &NodeAttempt,
    proposal_digest: FactoryDigest,
    summary: &octacity_protocol::FactoryToolActionSummaryV3,
  ) -> Result<DecisionSignalProviderRequest, ApplicationError> {
    summary.validate().map_err(|_| ApplicationError::invalid())?;
    let policy_digest = self.policy_digest()?;
    if input.subject().project_id() != self.project_id || !self.signal_allowance()?.fits_within(node.budget()) {
      return Err(ApplicationError::invalid());
    }
    let redacted = serde_json::json!({
      "schema": "octacity.factory.tool-action.v1",
      "proposal_sha256": proposal_digest.to_string(),
      "summary": summary,
    });
    let projected = self
      .plan
      .state_projection
      .project_value(&redacted, &self.plan.state_schema)
      .map_err(|_| ApplicationError::invalid())?;
    let state = serde_json::to_string(projected.value()).map_err(|_| ApplicationError::invalid())?;
    let provider_input = DecisionSignalProviderInput::new(
      DecisionSignalInputMedia::CanonicalJson,
      FactoryText::new(state).map_err(|_| ApplicationError::invalid())?,
      self.plan.questions.clone(),
      self.plan.consuming_question.clone(),
      None,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let identity = FactoryDigest::sha256(
      "octacity.factory.tool-risk-request.v1",
      &[
        &input.digest().map_err(|_| ApplicationError::invalid())?.as_bytes(),
        &policy_digest.as_bytes(),
        &proposal_digest.as_bytes(),
      ],
    );
    let id = DecisionSignalRequestId::from_uuid(uuid::Uuid::new_v5(&node.id().as_uuid(), &identity.as_bytes()))
      .map_err(|_| ApplicationError::invalid())?;
    let request = DecisionSignalRequest::new(
      id,
      input,
      node,
      DecisionSignalPurpose::ToolRisk,
      DecisionSignalDigests::new(provider_input.digest(), self.plan.profile.policy().digest()),
      self.plan.call_budget,
    )
    .map_err(|_| ApplicationError::invalid())?;
    DecisionSignalProviderRequest::new(
      request,
      &self.plan.profile,
      &self.plan.capability,
      provider_input,
      self.plan.threshold.clone(),
      node.deadline(),
    )
    .map_err(|_| ApplicationError::invalid())
  }

  /// Pins the complete installed policy and finite allowance into a task identity.
  /// Identical installation is stable; policy or authority replacement receives
  /// a new pin and cannot silently alter an already admitted Build task.
  pub fn policy_digest(&self) -> Result<FactoryDigest, ApplicationError> {
    self.signal_allowance()?;
    if self.plan.profile.purpose() != DecisionSignalPurpose::ToolRisk {
      return Err(ApplicationError::invalid());
    }
    let enforcement = PermissionCategory::ALL
      .into_iter()
      .filter(|category| self.local_permissions.can_enforce(*category))
      .collect::<Vec<_>>();
    let document = serde_json::to_vec(&(
      self.project_id,
      &self.plan,
      &self.mapping,
      self.max_calls,
      &self.credential_profile,
      self.network_host.as_str(),
      &self.project_permissions,
      self.local_permissions.grants(),
      enforcement,
    ))
    .map_err(|_| ApplicationError::invalid())?;
    if document.len() > MAX_FLOW_DATA_BYTES {
      return Err(ApplicationError::invalid());
    }
    Ok(FactoryDigest::sha256(
      "octacity.factory.tool-risk-policy.v1",
      &[&document],
    ))
  }

  /// Returns the finite provider reservation that must be separated from the
  /// ordinary worker budget and enforced atomically by the Signal journal.
  pub fn signal_allowance(&self) -> Result<BudgetLimit, ApplicationError> {
    let calls = u64::from(self.max_calls);
    let budget = self.plan.call_budget;
    if calls == 0 || calls > u64::from(MAX_FACTORY_ATTEMPTS) || budget.max_attempts() != 1 {
      return Err(ApplicationError::invalid());
    }
    let multiply = |value: u64| value.checked_mul(calls).ok_or_else(ApplicationError::invalid);
    BudgetLimit::new(
      u32::from(self.max_calls),
      multiply(budget.max_elapsed_millis())?,
      multiply(budget.max_tokens())?,
      multiply(budget.max_cost_micro_units())?,
      multiply(budget.max_output_bytes())?,
    )
    .map_err(|_| ApplicationError::invalid())
  }
}

/// Binds a capability's opaque base task contract to an exact tool-risk policy.
/// Neither provider wire formats nor logical Flow/node names participate here.
pub fn factory_tool_risk_task_digest(base_task_digest: FactoryDigest, policy_digest: FactoryDigest) -> FactoryDigest {
  FactoryDigest::sha256(
    "octacity.factory.tool-risk-task-definition.v1",
    &[&base_task_digest.as_bytes(), &policy_digest.as_bytes()],
  )
}
