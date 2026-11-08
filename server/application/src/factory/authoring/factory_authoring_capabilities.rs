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
    crate::factory::profiles::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
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
  /// Static command's eligible step kind. An omitted kind denotes an ordinary command.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub node_kind: Option<FlowNodeKind>,
  /// Exact compiler facts without an implied logical outcome.
  pub execution: FactoryCommandExecutionContract,
  /// Exact installed primary output schema, when declared.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub output_schema: Option<ImmutableReference>,
  /// Canonical schema when owned by the installed command.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub output_definition: Option<FlowDataSchema>,
  /// Independent verifier selected by the installed primary output.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub result_tool: Option<ImmutableReference>,
  /// Other installed output contracts required when selecting this primary output.
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub evidence: Vec<EvidenceRequirement>,
  /// Installed command request ceiling, for review under ordinary Flow policy.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub permissions: Option<FactoryPermissionSet>,
}
/// Exact public selection of an installed protected-test author.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryVerificationAuthorChoice {
  /// Ordinary Build configuration of the author.
  pub build_configuration: BuildConfigurationRef,
  /// Exact executable capability.
  pub tool: ImmutableReference,
  /// Exact plugin capability.
  pub plugin: ImmutableReference,
  /// Exact author model or command identity.
  pub model_or_tool: ImmutableReference,
  /// Required task digest for a static command. Role/Profile tasks bind separately.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub task_digest: Option<FactoryDigest>,
  /// Public handoff output.
  pub result_output: FactoryKey,
  /// Exact private inventory output.
  pub inventory: EvidenceRequirement,
}
/// Technical installation facts only; Source/Reporter choices retain action authority.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryAuthoringCapabilities {
  /// Commands explicitly declared by visible installed compilers; an empty list grants nothing.
  pub commands: Vec<FactoryCommandAuthoringContract>,
  /// Visible installed authors that can freeze supplemental verification inputs.
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub verification_authors: Vec<FactoryVerificationAuthorChoice>,
  /// Exact installed model identities supporting the lease-bound validation tool.
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub scoped_validation_profiles: Vec<ImmutableReference>,
  /// Exact qualified executions. A shared AI model alone does not identify its step mode.
  pub scoped_validation_executions: Vec<FactoryCommandExecutionContract>,
  /// Exact installed commands that verify a frozen protected test inventory.
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub verification_runners: Vec<ImmutableReference>,
  /// Visible installed publication targets, independent of previously published Flows.
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub candidate_publications: Vec<FactoryCandidatePublicationChoice>,
  /// Present when the Project has an installed local writer and active mapped labels.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub issue_classification: Option<FactoryIssueClassificationChoices>,
  /// Common deterministic interpreter, requiring no installed external policy adapter.
  pub deterministic_gate: FactoryDeterministicGateContract,
  /// Common durable human responses, requiring no external executor.
  pub human_gates: Vec<FactoryHumanGateContract>,
  /// Source modes supported by the ordinary Build/Job contract, independently
  /// of a selected compiler's configuration or current Agent readiness.
  pub build_resource_modes: Vec<&'static str>,
  /// Unsupported protocol prerequisite, if any. Selected Build and Agent support
  /// are still checked independently during validation and placement.
  pub source_free_incompatibility: Option<&'static str>,
}

/// Current, visible Project classification settings for authoring and review.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryIssueClassificationChoices {
  /// Exact installed local Issue action.
  pub plugin: ImmutableReference,
  /// Stable policy shared by the Project's Flows.
  pub policy: FactoryKey,
  /// Inclusive maximum accepted numeric priority. The minimum is zero.
  pub priority_maximum: u8,
  /// Canonical result schema for newly configured local writes.
  pub receipt_schema: FlowDataSchema,
  /// Serialized size of the canonical result, including its schema reference.
  pub receipt_output_bytes: u64,
  /// Active labels and their corresponding assessment size keys.
  pub size_labels: Vec<FactoryIssueClassificationSizeLabel>,
}

/// One installed mapping row resolved through the Issue catalog.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryIssueClassificationSizeLabel {
  /// Accepted size key.
  pub size: FactoryKey,
  /// Stable Project label identity.
  pub label: FactoryKey,
  /// Current display name of the label.
  pub name: String,
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
    crate::factory::profiles::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
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
    let (
      commands,
      verification_authors,
      scoped_validation_profiles,
      scoped_validation_executions,
      verification_runners,
      candidate_publications,
      issue_classification,
    ) = match crate::factory::authoring::factory_configuration_choices::visible_installed_factory_choices(
      self.store.as_ref(),
      self.policy.as_ref(),
      context,
      query.project_id,
    )
    .await
    {
      Ok(choices) => {
        let entries = choices.entries();
        let publications = choices
          .candidate_publications()
          .iter()
          .filter(|choice| {
            choice.receipt_schema.as_ref().is_some_and(|schema| {
              crate::factory::artifacts::factory_candidate_publication::publication_receipt_is_compatible(
                schema,
                choice.review_url,
              )
            }) && super::factory_configuration_choices::permissions_selectable(&choice.permissions, &entries.references)
          })
          .cloned()
          .collect();
        let classification = crate::factory::reporting::factory_issue_classification_authoring::current_choices(
          &choices,
          self.issue_properties.as_deref(),
          self.policy.as_ref(),
          context,
          query.project_id,
        )
        .await?;
        (
          command_contracts(&choices),
          verification_authors(&choices),
          choices
            .unambiguous_build_pipeline_contracts()
            .filter(|contract| {
              contract
                .compilation
                .as_ref()
                .is_some_and(|compiler| compiler.scoped_validation)
            })
            .map(|contract| contract.model_or_tool.clone())
            .collect(),
          choices
            .unambiguous_build_pipeline_contracts()
            .filter_map(|contract| {
              let compiler = contract
                .compilation
                .as_ref()
                .filter(|compiler| compiler.scoped_validation)?;
              Some(FactoryCommandExecutionContract {
                build_configuration: contract.build_configuration.clone(),
                tool: contract.tool.clone(),
                plugin: contract.plugin.clone(),
                model_or_tool: contract.model_or_tool.clone(),
                task_digest: compiler.task_digest,
                result_output: compiler.result_output.clone(),
                result_output_kind: compiler.result_output_kind,
              })
            })
            .collect(),
          choices
            .unambiguous_build_pipeline_contracts()
            .filter(|contract| {
              matches!(
                contract.protected_tests,
                Some(FactoryProtectedTestBuildContract::Verifier { .. })
              )
            })
            .map(|contract| contract.model_or_tool.clone())
            .collect(),
          publications,
          classification,
        )
      }
      Err(ApplicationError::CapabilityUnavailable) => (vec![], vec![], vec![], vec![], vec![], vec![], None),
      Err(error) => return Err(error),
    };
    Ok(FactoryAuthoringCapabilities {
      commands,
      verification_authors,
      scoped_validation_profiles,
      scoped_validation_executions,
      verification_runners,
      candidate_publications,
      issue_classification,
      deterministic_gate: FactoryDeterministicGateContract {
        policy: gate.policy().clone(),
        parameter_contract: "declarative_gate_v1",
        max_parameter_bytes: MAX_FLOW_GATE_PARAMETER_BYTES,
        resource_modes: vec!["none"],
      },
      human_gates,
      build_resource_modes: vec!["none", "repository"],
      source_free_incompatibility: None,
    })
  }
}

fn verification_authors(choices: &FactoryConfigurationChoices) -> Vec<FactoryVerificationAuthorChoice> {
  choices
    .unambiguous_build_pipeline_contracts()
    .filter_map(|installed| {
      let Some(FactoryProtectedTestBuildContract::Author {
        result_output,
        inventory,
      }) = &installed.protected_tests
      else {
        return None;
      };
      Some(FactoryVerificationAuthorChoice {
        build_configuration: installed.build_configuration.clone(),
        tool: installed.tool.clone(),
        plugin: installed.plugin.clone(),
        model_or_tool: installed.model_or_tool.clone(),
        task_digest: installed.command.as_ref().map(|command| command.task_digest),
        result_output: result_output.clone(),
        inventory: inventory.clone(),
      })
    })
    .collect()
}

fn command_contracts(choices: &FactoryConfigurationChoices) -> Vec<FactoryCommandAuthoringContract> {
  let entries = choices.entries();
  choices
    .unambiguous_build_pipeline_contracts()
    .filter_map(|installed| {
      let metadata = entries
        .references
        .iter()
        .filter(|entry| entry.kind == FactoryChoiceKind::NodeCapability && entry.reference == installed.tool)
        .filter_map(|entry| choices.choice_metadata(entry.kind, &entry.alias))
        .find(|metadata| {
          metadata.node_kinds.contains(&FlowNodeKind::BuildCommand)
            || metadata.node_kinds.contains(&FlowNodeKind::Reasoning)
        })?;
      let node_kind = (!metadata.node_kinds.contains(&FlowNodeKind::BuildCommand)).then_some(FlowNodeKind::Reasoning);
      let alias = entries
        .build_configurations
        .iter()
        .find(|(_, reference)| reference == &installed.build_configuration)?
        .0
        .clone();
      let execution = |task_digest, result_output, result_output_kind| FactoryCommandExecutionContract {
        build_configuration: installed.build_configuration.clone(),
        tool: installed.tool.clone(),
        plugin: installed.plugin.clone(),
        model_or_tool: installed.model_or_tool.clone(),
        task_digest,
        result_output,
        result_output_kind,
      };
      if let Some(native) = &installed.command {
        if !super::factory_configuration_choices::permissions_selectable(&native.permissions, &entries.references)
          || native.outputs.iter().any(|output| {
            [output.tool(), output.plugin()].iter().any(|reference| {
              !entries
                .references
                .iter()
                .any(|entry| entry.kind == FactoryChoiceKind::NodeCapability && &entry.reference == *reference)
            })
          })
        {
          return None;
        }
        return Some(
          native
            .outputs
            .iter()
            .filter_map(|output| {
              if node_kind.is_some() && output.output_kind() != EvidenceOutputKind::Report {
                return None;
              }
              if metadata
                .command_output
                .as_ref()
                .is_some_and(|shape| shape.reference() != output.schema())
              {
                return None;
              }
              if !octacity_server_factory::FactoryBuildCommandContract::accepts_primary_output(
                native.observation_schema.as_ref().map(FlowDataSchema::reference),
                output.output_kind(),
                output.schema(),
              ) {
                return None;
              }
              Some(FactoryCommandAuthoringContract {
                alias: alias.clone(),
                node_kind,
                execution: execution(native.task_digest, output.kind().clone(), output.output_kind()),
                output_schema: Some(output.schema().clone()),
                output_definition: native.observation_schema.clone().or_else(|| {
                  metadata
                    .command_output
                    .as_ref()
                    .filter(|schema| schema.reference() == output.schema())
                    .cloned()
                }),
                result_tool: (output.tool() != &installed.tool).then(|| output.tool().clone()),
                evidence: native
                  .outputs
                  .iter()
                  .filter(|other| other.kind() != output.kind())
                  .cloned()
                  .collect(),
                permissions: Some(native.permissions.clone()),
              })
            })
            .collect::<Vec<_>>(),
        );
      }
      let compiler = installed.legacy_command()?;
      Some(vec![FactoryCommandAuthoringContract {
        alias,
        node_kind,
        execution: execution(
          compiler.task_digest,
          compiler.result_output.clone(),
          compiler.result_output_kind,
        ),
        output_schema: None,
        output_definition: None,
        result_tool: None,
        evidence: vec![],
        permissions: None,
      }])
    })
    .flatten()
    .take(MAX_CONFIGURATION_CHOICES_PER_KIND)
    .collect()
}
