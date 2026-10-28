//! Editable configuration choices compiled into exact immutable Flow definitions.
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// Bounds alias expansion before allocating a concrete immutable schema.
const MAX_EXPANDED_SCHEMA_NODES: usize = 4096;

/// Named data shape for an editor or supplied template; publication computes its digest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowTemplateSchema {
  /// Operator-owned contract name.
  pub identity: FactoryKey,
  /// Operator-owned immutable contract version.
  pub version: FactoryKey,
  /// Closed bounded data shape.
  pub shape: FlowValueSchema,
}
/// Finite outcome referring to a named schema choice in the same template.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowTemplateOutcome {
  /// Configured result key.
  pub key: FactoryKey,
  /// Success or failure category, without business semantics.
  pub kind: FlowOutcomeKind,
  /// Schema choice resolved at publication.
  pub schema: FactoryKey,
  /// Explicitly disclose only this outcome's bounded payload to authorized Run readers.
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub public_result: bool,
}
/// An inline policy or explicit local choice resolved before immutable publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum FlowTemplateDeliverablePolicy {
  /// Complete policy owned by this editable node.
  Inline(CandidateDeliverablePolicy),
  /// Shared operator-owned policy; never a runtime lookup or an implicit grant.
  Reference {
    /// Exact local key in the enclosing template's policy catalogue.
    reference: FactoryKey,
  },
}
/// Editable acceptance selection and public policy, compiled to one frozen binding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowTemplateDeliverableBinding {
  /// Explicit projection of the verified accepted Work identity.
  pub accepted_contract: FlowDataMapping,
  /// Inline policy or explicitly selected shared local policy.
  pub policy: FlowTemplateDeliverablePolicy,
}
/// Editable node choices. Execution profiles are exact trusted choices supplied at publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowTemplateNode {
  /// Arbitrary logical node name.
  pub key: FactoryKey,
  /// Closed primitive capability.
  pub kind: FlowNodeKind,
  /// Schema choice for the explicitly projected input.
  pub input_schema: FactoryKey,
  /// Declared finite typed results.
  pub outcomes: Vec<FlowTemplateOutcome>,
  /// Hard per-attempt ceiling.
  pub budget: BudgetLimit,
  /// Additional technical executions requiring an exact accepted retry intent.
  #[serde(default)]
  pub max_execution_retries: u16,
  /// Authority narrowed at admission.
  #[serde(default)]
  pub permissions: FactoryPermissionSet,
  /// Whether this node must be reachable.
  pub required: bool,
  /// Explicit sources and input mapping.
  pub input_binding: FlowInputBinding,
  /// Optional closed context contract; only exact declared sources can be dispatched.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub context_binding: Option<crate::FlowContextBinding>,
  /// Installed command/reasoning profile choice.
  #[serde(default)]
  pub build_profile: Option<FactoryKey>,
  /// Exact reviewed Role/Profile candidates and selection policy, using the same
  /// ordinary reasoning-node contract as immutable definitions. No live aliases.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub profile_selection: Option<crate::FlowProfileSelectionBinding>,
  /// Optional disjoint writable directory scope for capture jobs; narrows the selected profile.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub capture_write_paths: Option<Vec<crate::FactoryRepositoryPath>>,
  /// Optional configured public-deliverable policy, frozen into the selected profile.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub capture_deliverables: Option<FlowTemplateDeliverableBinding>,
  /// Declarative deterministic program.
  #[serde(default)]
  pub gate: Option<FlowGateProgram>,
  /// Finite gate outcomes that freeze one exact Work contract.
  #[serde(default)]
  pub accepted_work_outcomes: Vec<FactoryKey>,
  /// Installed plugin action and parameters.
  #[serde(default)]
  pub action: Option<FlowActionBinding>,
  /// Configured arbitrary ready-pool choice.
  #[serde(default)]
  pub pool: Option<FactoryKey>,
  /// Exact independently published nested definition.
  #[serde(default)]
  pub subflow: Option<FlowDefinitionRef>,
}
/// Generic editable state-machine configuration; no node or route name is interpreted by code.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowDefinitionTemplate {
  /// Schema choice for incoming data to this Flow.
  pub input_schema: FactoryKey,
  /// Declared first node.
  pub entry: FactoryKey,
  /// Local reusable schema choices.
  pub schemas: BTreeMap<FactoryKey, FlowTemplateSchema>,
  /// Optional reusable policies; all references resolve before publication.
  #[serde(default)]
  pub deliverable_policies: BTreeMap<FactoryKey, CandidateDeliverablePolicy>,
  /// Declared nodes and their configured execution.
  pub nodes: Vec<FlowTemplateNode>,
  /// Finite conditional transitions selected only by accepted outcomes.
  pub transitions: Vec<FlowTransition>,
  /// Terminal keys and their schema choices.
  pub terminals: BTreeMap<FactoryKey, FactoryKey>,
  /// Optional exact context projections.
  #[serde(default)]
  pub context_projections: Vec<FlowContextProjection>,
  /// Optional exact typed data projections.
  #[serde(default)]
  pub data_projections: Vec<FlowDataProjection>,
  /// Enclosing hard ceilings.
  pub execution: FlowExecutionPolicy,
}
/// Publication result ready to include in a pinned definition closure and schema catalogue.
pub struct PublishedFlowTemplate {
  /// Exact content-addressed executable definition.
  pub definition: FlowDefinition,
  /// Exact schema choices ordered by content reference.
  pub schemas: Vec<FlowDataSchema>,
}
impl FlowDefinitionTemplate {
  /// Expands local schema choices under depth, node-count and encoded-size ceilings.
  /// Consumers get concrete content-addressed contracts; aliases grant no authority.
  pub fn data_schemas(&self) -> Result<BTreeMap<FactoryKey, FlowDataSchema>, FactoryError> {
    if self.schemas.is_empty() || self.schemas.len() > MAX_FLOW_SCHEMA_ENTRIES {
      return Err(invalid());
    }
    self
      .schemas
      .iter()
      .map(|(choice, schema)| {
        let mut remaining = MAX_EXPANDED_SCHEMA_NODES;
        Ok((
          choice.clone(),
          FlowDataSchema::new(
            schema.identity.clone(),
            schema.version.clone(),
            schema.shape.resolve(&self.schemas, 0, &mut remaining)?,
          )?,
        ))
      })
      .collect::<Result<BTreeMap<_, _>, FactoryError>>()
  }
  /// Compiles editable choices into an immutable definition, rejecting unknown or invalid choices.
  /// A UI can author this same document; supplied example flows have no special compiler path.
  pub fn instantiate(
    &self,
    id: FlowDefinitionId,
    version: FlowDefinitionVersion,
    profiles: &BTreeMap<FactoryKey, FlowBuildBinding>,
  ) -> Result<PublishedFlowTemplate, FactoryError> {
    if self.schemas.is_empty()
      || self.schemas.len() > MAX_FLOW_SCHEMA_ENTRIES
      || self.nodes.len() > MAX_FLOW_DEFINITION_NODES
      || self.deliverable_policies.len() > MAX_FLOW_SCHEMA_ENTRIES
      || serde_json::to_vec(self).map_err(|_| invalid())?.len() > MAX_FLOW_DATA_BYTES
    {
      return Err(invalid());
    }
    let schemas = self.data_schemas()?;
    let resolve = |choice: &FactoryKey| {
      schemas
        .get(choice)
        .map(|schema| schema.reference().clone())
        .ok_or_else(invalid)
    };
    let nodes = self
      .nodes
      .iter()
      .map(|choice| {
        let mut node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
          key: choice.key.clone(),
          kind: choice.kind,
          input_schema: Some(resolve(&choice.input_schema)?),
          outcomes: choice
            .outcomes
            .iter()
            .map(|outcome| {
              Ok(
                FlowOutcomeDefinition::new(outcome.key.clone(), outcome.kind, resolve(&outcome.schema)?)
                  .with_public_result(outcome.public_result),
              )
            })
            .collect::<Result<Vec<_>, FactoryError>>()?,
          budget: choice.budget,
          permissions: choice.permissions.clone(),
          required: choice.required,
          subflow: choice.subflow,
        })?
        .with_input_binding(choice.input_binding.clone())?
        .with_execution_retries(choice.max_execution_retries)?;
        if let Some(binding) = &choice.context_binding {
          node = node.with_context_binding(binding.clone())?;
        }
        if let Some(action) = &choice.action {
          node = node.with_action(action.clone())?;
        }
        if let Some(profile) = &choice.build_profile {
          let mut binding = profiles.get(profile).ok_or_else(invalid)?.clone();
          if let Some(contract) = &choice.capture_deliverables {
            let policy = match &contract.policy {
              FlowTemplateDeliverablePolicy::Inline(policy) => policy,
              FlowTemplateDeliverablePolicy::Reference { reference } => {
                self.deliverable_policies.get(reference).ok_or_else(invalid)?
              }
            };
            if binding
              .capture_deliverables
              .as_ref()
              .is_some_and(|ceiling| !policy.is_no_broader_than(&ceiling.policy))
            {
              return Err(invalid());
            }
            binding.capture_deliverables = Some(CandidateDeliverableBinding {
              accepted_contract: contract.accepted_contract.clone(),
              policy: policy.clone(),
            });
          }
          if let Some(paths) = &choice.capture_write_paths {
            if binding.capture_write_paths.as_ref().is_some_and(|ceiling| {
              paths
                .iter()
                .any(|path| !ceiling.iter().any(|parent| path.is_within(parent)))
            }) {
              return Err(invalid());
            }
            binding.capture_write_paths = Some(paths.clone());
          }
          node = node.with_build(binding)?;
        }
        if let Some(selection) = &choice.profile_selection {
          node = node.with_profile_selection(selection.clone())?;
        }
        if let Some(program) = &choice.gate {
          node = node.with_gate(program.binding()?)?;
        }
        if !choice.accepted_work_outcomes.is_empty() {
          node = node.with_accepted_work_outcomes(choice.accepted_work_outcomes.clone())?;
        }
        if let Some(pool) = &choice.pool {
          node = node.with_phase_pool(pool.clone());
        }
        if (choice.capture_write_paths.is_some() || choice.capture_deliverables.is_some())
          && choice.build_profile.is_none()
        {
          return Err(invalid());
        }
        if matches!(choice.kind, FlowNodeKind::BuildCommand | FlowNodeKind::Reasoning)
          && choice.build_profile.is_none()
          && choice.profile_selection.is_none()
          || choice.kind == FlowNodeKind::DeterministicGate && choice.gate.is_none()
          || matches!(choice.kind, FlowNodeKind::TrustedAction | FlowNodeKind::DecisionSignal)
            && choice.action.is_none()
        {
          return Err(invalid());
        }
        Ok(node)
      })
      .collect::<Result<Vec<_>, FactoryError>>()?;
    let definition = FlowDefinition::new(FlowDefinitionInput {
      id,
      version,
      input_schema: Some(resolve(&self.input_schema)?),
      entry: self.entry.clone(),
      nodes,
      transitions: self.transitions.clone(),
      terminals: self
        .terminals
        .iter()
        .map(|(key, choice)| Ok(FlowTerminalDefinition::new(key.clone(), resolve(choice)?)))
        .collect::<Result<Vec<_>, FactoryError>>()?,
      context_projections: self.context_projections.clone(),
      data_projections: self.data_projections.clone(),
      execution: self.execution.clone(),
    })?;
    let mut schemas = schemas.into_values().collect::<Vec<_>>();
    schemas.sort_by_key(|schema| schema.reference().clone());
    Ok(PublishedFlowTemplate { definition, schemas })
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow configuration template",
  }
}
