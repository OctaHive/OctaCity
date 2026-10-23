use super::*;
use octacity_server_domain::ProjectId;
use std::collections::BTreeMap;

/// Builds an arbitrary named model stage with three exact profile candidates.
pub fn factory_profile_selection_contract_fixture(
  candidate: Option<&str>,
) -> (PublishedFactoryAdmission, FlowNodeInput, FactoryClaimOwnership) {
  let prior = factory_node_journal_contract_fixture();
  let work = prior.admission.work.clone();
  let project: ProjectId = work.subject().project_id();
  let root = prior
    .admission
    .flow
    .closure()
    .definition(prior.admission.flow.closure().root())
    .unwrap();
  let original = &root.nodes()[0];
  let build = original.build().unwrap();
  let role = FactoryRole::publish(
    FactoryRoleId::generate(),
    project,
    FactoryRoleDefinition {
      name: FactoryText::new("Arbitrary AI specialist").unwrap(),
      instructions: "Follow Project rules".into(),
      template: None,
      icon: None,
      description: String::new(),
      concurrency_limit: None,
    },
  )
  .unwrap();
  let candidates: BTreeMap<_, _> = ["baseline", "advanced", "specialized"]
    .into_iter()
    .map(|name| {
      let definition = FactoryRoleProfileDefinition {
        name: FactoryText::new(name).unwrap(),
        instructions: "Private instructions".into(),
        template: None,
        toolchain: FactoryTaskToolchain {
          plugin: build.plugin.clone(),
          executable: build.tool.clone(),
          model: build.model_or_tool.clone(),
        },
        model_parameters: BTreeMap::new(),
        execution: Default::default(),
        skills: BTreeMap::new(),
        context_requirements: Default::default(),
        build_configuration: build.build_configuration.clone(),
        credential_purpose: FactoryCredentialPurpose::Model,
        permissions: original.permissions().clone(),
        budget: original.budget(),
      };
      (
        key(name),
        FlowProfileCandidate {
          profile: FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, definition).unwrap(),
          build: build.clone(),
        },
      )
    })
    .collect();
  let mut node = serde_json::to_value(original).unwrap();
  node.as_object_mut().unwrap().remove("build");
  node["kind"] = serde_json::json!("reasoning");
  node["profile_selection"] = serde_json::json!({"role":role.reference(),"candidates":candidates,
    "policy": FlowProfileSelectionPolicy::Manual { candidate: candidate.map(key) },
    "failure":{"kind":"manual_selection"},"selection_budget":BudgetUsage::default()});
  let node: FlowNodeDefinition = serde_json::from_value(node).unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: root.reference().id(),
    version: root.reference().version(),
    input_schema: root.input_schema().cloned(),
    entry: root.entry().clone(),
    nodes: vec![node],
    transitions: root.transitions().to_vec(),
    terminals: root.terminals().to_vec(),
    context_projections: root.context_projections().to_vec(),
    data_projections: root.data_projections().to_vec(),
    execution: root.execution().clone(),
  })
  .unwrap();
  let flow_run = FlowRun::root(&prior.admission.run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow_run).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    prior.admission.flow.limits().clone(),
    flow_run,
    cycle,
    prior.admission.flow.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(prior.admission.flow.data_schemas().to_vec())
  .unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    prior.input.node().clone(),
    prior.input.payload().clone(),
  )
  .unwrap()
  .with_context(prior.input.context().unwrap().clone(), vec![])
  .unwrap();
  (
    PublishedFactoryAdmission {
      flow: admitted,
      ..prior.admission
    },
    input,
    FactoryClaimOwnership::new(prior.attempt.owner().clone(), prior.attempt.claim()),
  )
}
