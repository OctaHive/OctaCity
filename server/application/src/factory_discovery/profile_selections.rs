//! An explicit allowlist over retained choices, existing capabilities and ordinary Build links.
use crate::ApplicationError;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::{FactoryRoleInvocationStore, FactoryRunSnapshot};
use serde::{Deserialize, Serialize};

/// Safe identity of an eligible exact profile; it grants no execution authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProfileCandidateProjection {
  /// Logical name in the admitted finite set.
  pub candidate: FactoryKey,
  /// Exact retained profile.
  pub profile: FactoryRoleProfileRef,
}
/// Safe ordinary operation and retained package correlation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProfileBuildProjection {
  /// Exact ordinary operation, including committed but not yet acknowledged dispatch.
  pub operation_id: FactoryDigest,
  /// Exact owning execution attempt.
  pub node_attempt_id: NodeAttemptId,
  /// Content identity of the retained package, absent until publication.
  pub package_digest: Option<FactoryDigest>,
  /// Actual Build acknowledgements, absent before dispatch is observed.
  pub executions: Vec<octacity_server_store::FactoryBuildExecutionDiagnostic>,
}
/// Provider receipt linkage with no questions, observations or response prose.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProfileSignalProjection {
  /// Exact logical operation identity.
  pub operation_id: DecisionSignalRequestId,
  /// Original receipt content identity, absent while unresolved.
  pub receipt_digest: Option<FactoryDigest>,
  /// Original finite state, absent before a receipt exists.
  pub state: Option<DecisionSignalState>,
  /// Whether the receipt controlled selection, retained its baseline, or invoked explicit fallback.
  pub consumption: Option<DecisionSignalConsumptionKind>,
  /// The original operation cannot be observed and cannot be repeated.
  pub unobservable: bool,
}
/// Latest accepted intent, distinct from consumption and subsequent Build dispatch.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProfileControlProjection {
  /// Exact immutable accepted control.
  pub id: FactoryDigest,
  /// Aggregate version produced by acceptance.
  pub version: FactoryRunVersion,
  /// Exact requested candidate profile.
  pub profile: FactoryRoleProfileRef,
  /// Trusted original acceptance time.
  pub recorded_at: Timestamp,
}
/// Safe selection history for one immutable node occurrence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProfileSelectionProjection {
  /// Canonical selection revision identity.
  pub id: FactoryDigest,
  /// Frozen node occurrence, shared with the control endpoint.
  pub input_digest: FactoryDigest,
  /// Owning nested or root execution.
  pub flow_run_id: FlowRunId,
  /// Owning immutable cycle.
  pub cycle_id: WorkflowCycleId,
  /// Caller-defined logical node.
  pub node: FactoryKey,
  /// Ordered selection revision.
  pub generation: u32,
  /// Retained predecessor, if replaced or recovered.
  pub predecessor: Option<FactoryDigest>,
  /// Accepted manual control consumed by this revision, if present.
  pub control_id: Option<FactoryDigest>,
  /// Configured mode: manual, rules or decision_api.
  pub method: String,
  /// Exact selector policy, only for Decision API mode.
  pub policy: Option<ImmutableReference>,
  /// Deterministic ordered-rule disposition.
  pub rules: Option<FlowProfileRuleOutcome>,
  /// Safe original provider receipt linkage.
  pub signal: Option<FactoryProfileSignalProjection>,
  /// Exact common role identity.
  pub role: FactoryRoleRef,
  /// Chosen finite candidate; absent while pending.
  pub candidate: Option<FactoryKey>,
  /// Chosen exact profile; absent while pending.
  pub profile: Option<FactoryRoleProfileRef>,
  /// Exact runtime metadata of the selected profile.
  pub runtime: Option<ImmutableReference>,
  /// Exact model metadata of the selected profile.
  pub model: Option<ImmutableReference>,
  /// Display text of the explicitly declared safe scalar reasoning setting.
  pub thinking_level: Option<String>,
  /// Original selector failure without input content.
  pub selector_error: Option<FlowProfileSelectionError>,
  /// Explicit recovery disposition; no implicit alternate model.
  pub recovery: Option<FlowProfileSelectionRecovery>,
  /// Latest unconsumed operator intent; it is not an executed choice.
  pub pending_control: Option<FactoryProfileControlProjection>,
  /// Original wait origin.
  pub started_at: Timestamp,
  /// Original exclusive deadline, never refreshed by polling.
  pub deadline: Timestamp,
  /// Whether a later immutable revision superseded this row.
  pub superseded: bool,
  /// Currently qualified choices while this occurrence remains actionable.
  pub eligible_candidates: Vec<FactoryProfileCandidateProjection>,
  /// Ordinary Build and retained package provenance.
  pub builds: Vec<FactoryProfileBuildProjection>,
}

pub(super) async fn project(
  row: &FlowProfileSelectionRecord,
  snapshot: &FactoryRunSnapshot,
  choices: Option<&FactoryConfigurationChoices>,
  packages: Option<&dyn FactoryRoleInvocationStore>,
  now: Timestamp,
) -> Result<FactoryProfileSelectionProjection, ApplicationError> {
  let invalid = ApplicationError::invalid;
  let input = snapshot
    .flow
    .data
    .inputs
    .iter()
    .find(|input| input.digest().ok() == Some(row.input_digest()))
    .ok_or_else(invalid)?;
  let binding = snapshot
    .admitted_flow
    .closure()
    .definition(input.definition())
    .and_then(|d| d.node(input.node()))
    .and_then(|n| n.profile_selection())
    .ok_or_else(invalid)?;
  let (method, policy) = match &binding.policy {
    FlowProfileSelectionPolicy::Manual { .. } => ("manual", None),
    FlowProfileSelectionPolicy::Rules { .. } => ("rules", None),
    FlowProfileSelectionPolicy::DecisionApi { policy, .. } => ("decision_api", Some(policy.clone())),
  };
  let selected = row.candidate().and_then(|key| binding.candidates.get(key));
  let id = row.digest().map_err(|_| invalid())?;
  let superseded = snapshot
    .flow
    .data
    .current_profile_selection(row.input_digest())
    .is_none_or(|current| current.digest().ok() != Some(id));
  let pending_control = octacity_server_store::latest_factory_profile_control(&snapshot.controls, row.input_digest())
    .filter(|control| !superseded && row.manual_control() != Some(control.id))
    .and_then(|control| match &control.intent {
      octacity_server_store::FactoryRunControlIntent::SelectProfile { profile, .. } => {
        Some(FactoryProfileControlProjection {
          id: control.id,
          version: control.run_version,
          profile: profile.clone(),
          recorded_at: control.recorded_at,
        })
      }
      _ => None,
    });
  let mut eligible_candidates = Vec::new();
  // Reuse the command's exact eligibility checks, including deadline, cancellation,
  // pending control, cycle and commitment. Discovery does not reserve the choice.
  if !superseded {
    for (key, candidate) in &binding.candidates {
      if octacity_server_store::factory_profile_choice_is_open(
        snapshot,
        row.input_digest(),
        candidate.profile.reference(),
        now,
      ) && choices.is_some_and(|choices| candidate.validate_capabilities(choices).is_ok())
      {
        eligible_candidates.push(FactoryProfileCandidateProjection {
          candidate: key.clone(),
          profile: candidate.profile.reference().clone(),
        });
      }
    }
  }
  let mut builds = Vec::new();
  for intent in snapshot
    .flow
    .data
    .build_intents
    .iter()
    .filter(|intent| intent.selection_digest() == Some(id))
  {
    let operation_id = intent.operation_id().map_err(|_| invalid())?;
    let package_digest = match packages {
      Some(packages) => packages
        .factory_role_invocation(operation_id)
        .await?
        .map(|package| {
          package.verify_intent(intent)?;
          package.package.digest().map_err(|_| invalid())
        })
        .transpose()?,
      None => None,
    };
    builds.push(FactoryProfileBuildProjection {
      operation_id,
      node_attempt_id: intent.node().id(),
      package_digest,
      executions: snapshot
        .flow
        .data
        .build_executions
        .iter()
        .filter(|execution| execution.operation_id() == operation_id)
        .map(Into::into)
        .collect(),
    });
  }
  Ok(FactoryProfileSelectionProjection {
    id,
    input_digest: row.input_digest(),
    flow_run_id: input.flow_run_id(),
    cycle_id: input.cycle_id(),
    node: input.node().clone(),
    generation: row.generation(),
    predecessor: row.pending_digest(),
    control_id: row.manual_control(),
    method: method.into(),
    policy,
    rules: row.rules().cloned(),
    signal: row.signal().map(|signal| FactoryProfileSignalProjection {
      operation_id: signal.request().id(),
      receipt_digest: signal.receipt().map(DecisionSignalReceipt::receipt_digest),
      state: signal.receipt().map(DecisionSignalReceipt::state),
      consumption: signal.receipt().map(|receipt| receipt.consumption().kind()),
      unobservable: signal.unobservable(),
    }),
    role: binding.role.clone(),
    candidate: row.candidate().cloned(),
    profile: selected.map(|c| c.profile.reference().clone()),
    runtime: selected.map(|c| c.profile.definition().toolchain.plugin.clone()),
    model: selected.map(|c| c.profile.definition().toolchain.model.clone()),
    thinking_level: selected
      .and_then(|candidate| {
        let definition = candidate.profile.definition();
        definition
          .thinking_parameter
          .as_ref()
          .and_then(|key| definition.model_parameters.get(key))
      })
      .map(|value| value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string())),
    selector_error: row.selector_error(),
    recovery: row.recovery(),
    started_at: row.started_at(),
    deadline: row.deadline(),
    superseded,
    eligible_candidates,
    builds,
    pending_control,
  })
}
