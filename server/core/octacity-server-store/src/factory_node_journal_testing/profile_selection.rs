use super::*;
use octacity_server_domain::ProjectId;
use std::collections::BTreeMap;

/// Builds an arbitrary named model stage with three exact profile candidates.
pub fn factory_profile_selection_contract_fixture(
  candidate: Option<&str>,
) -> (PublishedFactoryAdmission, FlowNodeInput, FactoryClaimOwnership) {
  factory_profile_selection_policy_contract_fixture(FlowProfileSelectionPolicy::Manual {
    candidate: candidate.map(key),
  })
}

/// Reuses the same frozen stage contract with an explicitly configured selection policy.
pub fn factory_profile_selection_policy_contract_fixture(
  policy: FlowProfileSelectionPolicy,
) -> (PublishedFactoryAdmission, FlowNodeInput, FactoryClaimOwnership) {
  selection_fixture(policy, None)
}

fn selection_fixture(
  policy: FlowProfileSelectionPolicy,
  data: Option<(FlowDataSchema, serde_json::Value)>,
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
  if let Some((schema, _)) = &data {
    node["input_schema"] = serde_json::to_value(schema.reference()).unwrap();
  }
  node["kind"] = serde_json::json!("reasoning");
  node["profile_selection"] = serde_json::json!({"role":role.reference(),"candidates":candidates,
    "policy": policy,
    "failure":{"kind":"manual_selection"},"selection_budget":BudgetUsage::default()});
  let node: FlowNodeDefinition = serde_json::from_value(node).unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: root.reference().id(),
    version: root.reference().version(),
    input_schema: data
      .as_ref()
      .map(|(schema, _)| schema.reference().clone())
      .or_else(|| root.input_schema().cloned()),
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
  let mut schemas = prior.admission.flow.data_schemas().to_vec();
  if let Some((schema, _)) = &data {
    schemas.push(schema.clone());
  }
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    prior.admission.flow.limits().clone(),
    flow_run,
    cycle,
    prior.admission.flow.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(schemas)
  .unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    prior.input.node().clone(),
    data.map_or_else(
      || prior.input.payload().clone(),
      |(schema, value)| FlowPayload::new(&schema, value).unwrap(),
    ),
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

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  fn rules(when: serde_json::Value) -> FlowProfileSelectionPolicy {
    serde_json::from_value(
      json!({"mode":"rules", "rules":[{"when":when,"candidate":"advanced"}],"otherwise":"baseline"}),
    )
    .unwrap()
  }
  fn compare(path: &str, value: serde_json::Value) -> serde_json::Value {
    json!({"operation":"compare","left":{"source":"path","path":path},"comparison":"greater","right":{"source":"literal","value":value}})
  }
  #[test]
  fn rules_distinguish_false_missing_and_invalid_types_without_implicit_selection() {
    for (predicate, candidate, outcome) in [
      (
        compare("/count", json!(10)),
        Some(key("baseline")),
        json!({"kind":"catch_all"}),
      ),
      (
        compare("/complexity", json!(2)),
        None,
        json!({"kind":"failed","reason":"missing_input"}),
      ),
      (
        compare("/count", json!("2")),
        None,
        json!({"kind":"failed","reason":"invalid_type"}),
      ),
    ] {
      let (admission, input, _) = factory_profile_selection_policy_contract_fixture(rules(predicate));
      let selection = FlowProfileSelectionRecord::select(&input, &admission.flow, time(11)).unwrap();
      assert_eq!(selection.candidate(), candidate.as_ref());
      assert_eq!(serde_json::to_value(&selection).unwrap()["rules"], outcome);
      assert_eq!(
        FlowProfileSelectionRecord::restore(
          &serde_json::to_vec(&selection).unwrap(),
          &input,
          &admission.flow,
          selection.digest().unwrap()
        )
        .unwrap(),
        selection
      );
    }
  }
  #[test]
  fn rules_bound_nested_evaluation_before_selecting_a_candidate() {
    let schema = FlowDataSchema::new(
      key("criteria.matrix"),
      key("v1"),
      FlowValueSchema::Array {
        item: Box::new(FlowValueSchema::Array {
          item: Box::new(FlowValueSchema::Boolean),
          min_items: 0,
          max_items: 128,
        }),
        min_items: 0,
        max_items: 128,
      },
    )
    .unwrap();
    let policy = rules(
      json!({"operation":"for_each","collection":{"source":"path","path":""},"quantifier":"every", "predicate":{
        "operation":"for_each","collection":{"source":"item","path":""},"quantifier":"every","predicate":{"operation":"exists","operand":{"source":"item","path":""}}
      }}),
    );
    let (small, small_input, _) = selection_fixture(policy.clone(), Some((schema.clone(), json!([[true]]))));
    assert_eq!(
      FlowProfileSelectionRecord::select(&small_input, &small.flow, time(11))
        .unwrap()
        .candidate(),
      Some(&key("advanced"))
    );
    let (admission, input, _) = selection_fixture(policy, Some((schema, json!(vec![vec![true; 128]; 128]))));
    let selection = FlowProfileSelectionRecord::select(&input, &admission.flow, time(11)).unwrap();
    assert_eq!(selection.candidate(), None);
    assert_eq!(
      serde_json::to_value(&selection).unwrap()["rules"],
      json!({"kind":"failed","reason":"evaluation_bound_exceeded"})
    );
    assert_eq!(
      FlowProfileSelectionRecord::restore(
        &serde_json::to_vec(&selection).unwrap(),
        &input,
        &admission.flow,
        selection.digest().unwrap()
      )
      .unwrap(),
      selection
    );
  }

  #[test]
  fn rules_skip_false_conditions_and_allow_an_explicit_absence_guard() {
    for (first, expected, index) in [
      (compare("/count", json!(10)), "specialized", 1),
      (json!({"operation":"absent","path":"/complexity"}), "advanced", 0),
    ] {
      let policy = serde_json::from_value(json!({"mode":"rules", "rules":[
        {"when":first,"candidate":"advanced"},
        {"when":compare("/count",json!(2)),"candidate":"specialized"},
        {"when":compare("/complexity",json!(2)),"candidate":"advanced"}
      ],"otherwise":"baseline"}))
      .unwrap();
      let (admission, input, _) = factory_profile_selection_policy_contract_fixture(policy);
      let selection = FlowProfileSelectionRecord::select(&input, &admission.flow, time(11)).unwrap();
      assert_eq!(selection.candidate(), Some(&key(expected)));
      assert_eq!(
        serde_json::to_value(&selection).unwrap()["rules"],
        json!({"kind":"matched","rule":index})
      );
      let schema = admission
        .flow
        .data_schemas()
        .iter()
        .find(|schema| schema.reference() == input.payload().schema())
        .unwrap();
      assert!(
        FlowPayload::new(schema, json!({"count":"3"})).is_err(),
        "criteria cannot coerce schema-invalid observations"
      );
      assert!(
        FlowPayload::new(schema, json!({})).is_err(),
        "schema-required observations cannot be omitted"
      );
    }
  }
}
