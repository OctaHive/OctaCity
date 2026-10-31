//! Read-only contracts of common interpreter primitives, independent of saved Flows.
use crate::*;
use octacity_server_factory::*;

/// Normalizes an operator-owned finite program through the common evaluator contract.
#[derive(Clone, Debug)]
pub struct ValidateFactoryGateProgramQuery {
  /// Owning visible Project.
  pub project_id: octacity_server_domain::ProjectId,
  /// Closed bounded rules, fallback and explicit projections; no client policy digest.
  pub program: FlowGateProgram,
}
impl Query for ValidateFactoryGateProgramQuery {
  type Outcome = FlowGateBinding;
}
impl ManagementAuthorizationTarget for ValidateFactoryGateProgramQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleProfileQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
#[async_trait::async_trait]
impl<S: Send + Sync> ManagementQueryUseCase<ValidateFactoryGateProgramQuery> for FactoryRoleProfileHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ValidateFactoryGateProgramQuery,
  ) -> Result<FlowGateBinding, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
    query
      .program
      .binding()
      .map_err(|_| FactoryValidationDiagnostics::invalid("/program"))
  }
}

/// Project-authorized discovery; this never qualifies an external executor or grants access.
#[derive(Clone, Debug)]
pub struct GetFactoryAuthoringCapabilitiesQuery {
  /// Project whose operator is constructing a Flow.
  pub project_id: octacity_server_domain::ProjectId,
}
impl Query for GetFactoryAuthoringCapabilitiesQuery {
  type Outcome = FactoryAuthoringCapabilities;
}
impl ManagementAuthorizationTarget for GetFactoryAuthoringCapabilitiesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleProfileQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
/// Exact built-in evaluator and its existing bounded parameter grammar.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryDeterministicGateContract {
  /// Interpreter-owned immutable evaluator; never derived by a browser.
  pub policy: ImmutableReference,
  /// Versioned typed parameter grammar, not arbitrary executable code.
  pub parameter_contract: &'static str,
  /// Existing evaluator's encoded parameter ceiling.
  pub max_parameter_bytes: usize,
  /// This in-process interpreter never checks out or mounts a code repository.
  pub resource_modes: Vec<&'static str>,
}
/// One permitted management response and the exact observation shape it produces.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryHumanResponseContract {
  /// Technical response; the operator still declares the corresponding Flow outcome.
  pub disposition: FlowHumanDisposition,
  /// Schema supplied by the response producer, not model-authored evidence.
  pub shape: FlowValueSchema,
}
/// Bounded choices for one native human operation.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryHumanGateContract {
  /// Native operation with its independent authorization semantics.
  pub operation: FlowHumanOperation,
  /// Only responses accepted by this operation.
  pub responses: Vec<FactoryHumanResponseContract>,
  /// Human responses consume retained input without a code workspace.
  pub resource_modes: Vec<&'static str>,
}
/// Technical installation facts only; Source/Reporter choices retain action authority.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryAuthoringCapabilities {
  /// Common deterministic interpreter, requiring no installed external policy adapter.
  pub deterministic_gate: FactoryDeterministicGateContract,
  /// Common durable human responses, requiring no external executor.
  pub human_gates: Vec<FactoryHumanGateContract>,
  /// Source modes currently supported by the ordinary Build/Job contract.
  pub build_resource_modes: Vec<&'static str>,
  /// Explicit missing prerequisite rather than advertising unqualified source-free execution.
  pub source_free_incompatibility: Option<&'static str>,
}
#[async_trait::async_trait]
impl<S: Send + Sync> ManagementQueryUseCase<GetFactoryAuthoringCapabilitiesQuery> for FactoryRoleProfileHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetFactoryAuthoringCapabilitiesQuery,
  ) -> Result<FactoryAuthoringCapabilities, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
    let invalid = |_| ApplicationError::invalid();
    let fallback = FactoryKey::new("fallback").map_err(invalid)?;
    let gate = FlowGateProgram::new(vec![], fallback.clone())
      .map_err(invalid)?
      .binding()
      .map_err(invalid)?;
    let mut human_gates = vec![];
    for operation in [
      FlowHumanOperation::Escalation,
      FlowHumanOperation::Delivery,
      FlowHumanOperation::Promotion,
    ] {
      let dispositions: Vec<_> = [
        FlowHumanDisposition::Acknowledge,
        FlowHumanDisposition::Resume,
        FlowHumanDisposition::Abort,
        FlowHumanDisposition::Confirm,
      ]
      .into_iter()
      .filter(|disposition| operation.permits(*disposition))
      .collect();
      let binding = FlowHumanGateBinding::new(
        operation,
        dispositions
          .iter()
          .map(|disposition| (*disposition, fallback.clone()))
          .collect(),
      )
      .map_err(invalid)?;
      let responses = dispositions
        .into_iter()
        .map(|disposition| {
          Ok(FactoryHumanResponseContract {
            disposition,
            shape: binding.response_shape(disposition).map_err(invalid)?,
          })
        })
        .collect::<Result<_, ApplicationError>>()?;
      human_gates.push(FactoryHumanGateContract {
        operation,
        responses,
        resource_modes: vec!["none"],
      });
    }
    Ok(FactoryAuthoringCapabilities {
      deterministic_gate: FactoryDeterministicGateContract {
        policy: gate.policy().clone(),
        parameter_contract: "declarative_gate_v1",
        max_parameter_bytes: MAX_FLOW_GATE_PARAMETER_BYTES,
        resource_modes: vec!["none"],
      },
      human_gates,
      build_resource_modes: vec!["repository"],
      source_free_incompatibility: Some("executor_support_required"),
    })
  }
}
