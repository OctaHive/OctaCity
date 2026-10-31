//! Read-only contracts of common interpreter primitives, independent of saved Flows.
use crate::*;
use octacity_server_factory::*;
use octacity_server_store::FactoryConfigurationStore;

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
/// Exact ordinary command compiler facts. Routing is authored separately in the Flow.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryCommandExecutionContract {
  /// Exact installed ordinary Build configuration.
  pub build_configuration: BuildConfigurationRef,
  /// Installed executable capability.
  pub tool: ImmutableReference,
  /// Installed task plugin.
  pub plugin: ImmutableReference,
  /// Installed command configuration, without argv or credential values.
  pub model_or_tool: ImmutableReference,
  /// Native compiler task identity; clients do not derive it.
  pub task_digest: FactoryDigest,
  /// Exact primary result output.
  pub result_output: FactoryKey,
  /// Exact publication category of the output.
  pub result_output_kind: EvidenceOutputKind,
}
/// A visible installed command choice, independent of saved Flow nodes.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryCommandAuthoringContract {
  /// Existing authorized Build configuration alias.
  pub alias: FactoryKey,
  /// Exact compiler facts without an implied logical outcome.
  pub execution: FactoryCommandExecutionContract,
}
/// Technical installation facts only; Source/Reporter choices retain action authority.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryAuthoringCapabilities {
  /// Commands explicitly declared by visible installed compilers; an empty list grants nothing.
  pub commands: Vec<FactoryCommandAuthoringContract>,
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
impl<S: FactoryConfigurationStore> ManagementQueryUseCase<GetFactoryAuthoringCapabilitiesQuery>
  for FactoryRoleProfileHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
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
    let commands = match crate::factory_configuration_choices::visible_installed_factory_choices(
      self.store.as_ref(),
      self.policy.as_ref(),
      context,
      query.project_id,
    )
    .await
    {
      Ok(choices) => command_contracts(&choices),
      Err(ApplicationError::CapabilityUnavailable) => vec![],
      Err(error) => return Err(error),
    };
    Ok(FactoryAuthoringCapabilities {
      commands,
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

fn command_contracts(choices: &FactoryConfigurationChoices) -> Vec<FactoryCommandAuthoringContract> {
  let entries = choices.entries();
  choices
    .build_pipeline_contracts()
    .iter()
    .filter_map(|installed| {
      let compiler = installed
        .compilation
        .as_ref()
        .filter(|compiler| !compiler.role_profiles)?;
      let declared = entries.references.iter().any(|entry| {
        entry.kind == FactoryChoiceKind::NodeCapability
          && entry.reference == installed.tool
          && choices
            .choice_metadata(entry.kind, &entry.alias)
            .is_some_and(|metadata| metadata.node_kinds.contains(&FlowNodeKind::BuildCommand))
      });
      if !declared {
        return None;
      }
      let alias = entries
        .build_configurations
        .iter()
        .find(|(_, reference)| reference == &installed.build_configuration)?
        .0
        .clone();
      Some(FactoryCommandAuthoringContract {
        alias,
        execution: FactoryCommandExecutionContract {
          build_configuration: installed.build_configuration.clone(),
          tool: installed.tool.clone(),
          plugin: installed.plugin.clone(),
          model_or_tool: installed.model_or_tool.clone(),
          task_digest: compiler.task_digest,
          result_output: compiler.result_output.clone(),
          result_output_kind: compiler.result_output_kind,
        },
      })
    })
    .collect()
}
