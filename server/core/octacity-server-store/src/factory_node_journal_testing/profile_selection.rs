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
  factory_profile_selection_failure_contract_fixture(policy, FlowProfileSelectionFailure::ManualSelection)
}

/// Uses the same immutable stage with an explicit selector-failure policy.
pub fn factory_profile_selection_failure_contract_fixture(
  policy: FlowProfileSelectionPolicy,
  failure: FlowProfileSelectionFailure,
) -> (PublishedFactoryAdmission, FlowNodeInput, FactoryClaimOwnership) {
  selection_fixture(policy, failure, None)
}

/// Trusted installed profile declarations, shared by selection-recovery contracts.
pub fn factory_profile_selection_capabilities(admitted: &AdmittedFlow) -> FactoryConfigurationChoices {
  let candidates: Vec<_> = admitted
    .closure()
    .definitions()
    .iter()
    .flat_map(|d| d.nodes())
    .filter_map(|n| n.profile_selection())
    .flat_map(|s| s.candidates.values())
    .collect();
  let mut entries = FactoryConfigurationChoiceEntries::default();
  for (index, candidate) in candidates.iter().enumerate() {
    entries.references.push(FactoryReferenceChoice {
      kind: FactoryChoiceKind::NodeCapability,
      alias: key(&format!("profile.{index}")),
      reference: candidate.build.tool.clone(),
    });
  }
  for (index, candidate) in candidates.iter().enumerate() {
    for (name, reference) in [
      ("verifier", candidate.build.result_tool()),
      ("plugin", &candidate.build.plugin),
      ("model", &candidate.build.model_or_tool),
    ] {
      entries.references.push(FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: key(&format!("profile.{index}.{name}")),
        reference: reference.clone(),
      });
    }
    entries.build_configurations.push((
      key(&format!("profile.{index}.build")),
      candidate.build.build_configuration.clone(),
    ));
    for (evidence_index, evidence) in candidate.build.evidence.iter().enumerate() {
      for (name, reference) in [("tool", evidence.tool()), ("plugin", evidence.plugin())] {
        entries.references.push(FactoryReferenceChoice {
          kind: FactoryChoiceKind::NodeCapability,
          alias: key(&format!("profile.{index}.evidence.{evidence_index}.{name}")),
          reference: reference.clone(),
        });
      }
    }
  }
  for candidate in &candidates {
    for (kind, names) in [
      (
        FactoryChoiceKind::SecretProfile,
        candidate
          .profile
          .definition()
          .permissions
          .secret_profiles()
          .collect::<Vec<_>>(),
      ),
      (
        FactoryChoiceKind::WorkloadIdentityProfile,
        candidate
          .profile
          .definition()
          .permissions
          .workload_identity_profiles()
          .collect::<Vec<_>>(),
      ),
    ] {
      for name in names {
        if !entries
          .references
          .iter()
          .any(|entry| entry.kind == kind && &entry.alias == name)
        {
          entries.references.push(FactoryReferenceChoice {
            kind,
            alias: name.clone(),
            reference: ImmutableReference::new(name.clone(), key("v1"), digest(7)),
          });
        }
      }
    }
  }
  let mut choices = FactoryConfigurationChoices::try_new(entries).unwrap();
  for (index, candidate) in candidates.iter().enumerate() {
    choices = choices
      .with_role_profile_capability(
        key(&format!("profile.{index}")),
        FactoryRoleProfileCapability {
          ceiling: candidate.profile.definition().clone(),
          parameters: FlowDataSchema::new(
            key("parameters"),
            key("v1"),
            FlowValueSchema::Object {
              fields: Default::default(),
            },
          )
          .unwrap(),
          public_environment: Default::default(),
          local_permissions: LocalPermissionCeiling::fully_enforced(candidate.profile.definition().permissions.clone()),
        },
      )
      .unwrap();
  }
  choices
}

fn selection_fixture(
  policy: FlowProfileSelectionPolicy,
  failure: FlowProfileSelectionFailure,
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
    "failure":failure,"selection_budget":BudgetUsage::default()});
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

  fn nested_override_fixture() -> (PublishedFactoryAdmission, FlowPayload, FactoryClaimOwnership) {
    let (mut admission, input, ownership) = factory_profile_selection_contract_fixture(Some("advanced"));
    let prior = &admission.flow.closure().definitions()[0];
    let budget = BudgetLimit::new(32, 32_000, 32_000, 32_000, 320_000).unwrap();
    let permissions = admission.flow.limits().permissions().clone();
    let binding = FlowInputBinding::new(
      Default::default(),
      FlowDataMapping::new(FlowValueMapping::Constant {
        value: input.payload().value().clone(),
      })
      .unwrap(),
    )
    .unwrap();
    let child_node = prior.nodes()[0].clone().with_input_binding(binding.clone()).unwrap();
    let child = FlowDefinition::new(FlowDefinitionInput {
      id: prior.reference().id(),
      version: prior.reference().version(),
      input_schema: prior.input_schema().cloned(),
      entry: prior.entry().clone(),
      nodes: vec![child_node],
      transitions: prior.transitions().to_vec(),
      terminals: prior.terminals().to_vec(),
      context_projections: prior.context_projections().to_vec(),
      data_projections: prior.data_projections().to_vec(),
      execution: prior.execution().clone(),
    })
    .unwrap();
    let returned = child.terminals()[0].key().clone();
    let result_schema = child.terminals()[0].schema().clone();
    let calls = ["left", "right"]
      .into_iter()
      .map(|name| {
        FlowNodeDefinition::new(FlowNodeDefinitionInput {
          key: key(name),
          kind: FlowNodeKind::SubflowCall,
          input_schema: Some(input.payload().schema().clone()),
          outcomes: vec![FlowOutcomeDefinition::new(
            returned.clone(),
            FlowOutcomeKind::Success,
            result_schema.clone(),
          )],
          budget,
          permissions: permissions.clone(),
          required: true,
          subflow: Some(child.reference()),
        })
        .unwrap()
        .with_input_binding(binding.clone())
        .unwrap()
      })
      .collect();
    let root = FlowDefinition::new(FlowDefinitionInput {
      id: FlowDefinitionId::generate(),
      version: FlowDefinitionVersion::INITIAL,
      input_schema: Some(input.payload().schema().clone()),
      entry: key("left"),
      nodes: calls,
      transitions: vec![
        FlowTransition::new(key("left"), returned.clone(), FlowTransitionTarget::Node(key("right"))),
        FlowTransition::new(key("right"), returned, FlowTransitionTarget::Terminal(key("done"))),
      ],
      terminals: vec![FlowTerminalDefinition::new(key("done"), result_schema)],
      context_projections: vec![],
      data_projections: vec![],
      execution: FlowExecutionPolicy::new(budget, permissions.clone(), 1).unwrap(),
    })
    .unwrap();
    let root_run = FlowRun::root(&admission.run, root.reference()).unwrap();
    let cycle = WorkflowCycle::initial(&root_run).unwrap();
    admission.flow = AdmittedFlow::new(
      PinnedFlowDefinitionClosure::new(root.reference(), vec![root, child]).unwrap(),
      FlowAdmissionLimits::product_defaults(2, budget, permissions).unwrap(),
      root_run,
      cycle,
      FactoryWipLimits::new(4, 8).unwrap(),
    )
    .unwrap()
    .with_data_schemas(admission.flow.data_schemas().to_vec())
    .unwrap();
    (admission, input.payload().clone(), ownership)
  }

  #[test]
  fn admission_override_distinguishes_nested_call_sites_and_survives_input_recovery_and_retry() {
    let (admission, payload, ownership) = nested_override_fixture();
    let root = admission
      .flow
      .closure()
      .definition(admission.flow.closure().root())
      .unwrap();
    let child = admission
      .flow
      .closure()
      .definition(root.node(&key("left")).unwrap().subflow_definition().unwrap())
      .unwrap();
    let selected = child.nodes()[0].profile_selection().unwrap().candidates[&key("specialized")]
      .profile
      .reference();
    let overrides = serde_json::from_value(
      json!([{"stage":{"root":root.reference(),"calls":["left"],"node":child.entry()},"profile":selected}]),
    )
    .unwrap();
    let work = admission
      .work
      .with_profile_overrides(overrides, admission.flow.closure())
      .unwrap();
    let calls: Vec<_> = ["left", "right"]
      .into_iter()
      .enumerate()
      .map(|(index, name)| {
        NodeAttempt::new(
          admission.flow.root_run(),
          admission.flow.initial_cycle(),
          root,
          NodeAttemptInput {
            id: NodeAttemptId::generate(),
            node_key: key(name),
            node_kind: FlowNodeKind::SubflowCall,
            number: NodeAttemptNumber::new(index as u64 + 1).unwrap(),
            input_digest: FlowNodeInput::new(
              &work,
              &admission.flow,
              admission.flow.root_run(),
              admission.flow.initial_cycle(),
              key(name),
              payload.clone(),
            )
            .unwrap()
            .digest()
            .unwrap(),
            budget: root.node(&key(name)).unwrap().budget(),
            started_at: time(11),
            deadline: time(99),
            execution: NodeExecutionIdentity::BuiltIn,
            ownership: ownership.clone(),
          },
        )
        .unwrap()
      })
      .collect();
    let children: Vec<_> = calls
      .iter()
      .map(|call| {
        FlowRun::nested(
          FlowRunId::generate(),
          admission.run.id(),
          child.reference(),
          FlowRunParent::new(call.flow_run_id(), call.id()),
        )
      })
      .collect();
    let mut flows = vec![admission.flow.root_run().clone()];
    flows.extend(children.clone());
    let mut cycles = vec![admission.flow.initial_cycle().clone()];
    cycles.extend(children.iter().map(|flow| WorkflowCycle::initial(flow).unwrap()));
    let history = FlowRuntimeHistory {
      stops: &[],
      flow_runs: &flows,
      cycles: &cycles,
      attempts: &calls,
      completions: &[],
    };
    for (index, expected) in [(0, "specialized"), (1, "advanced")] {
      let input = FlowNodeInput::new(
        &work,
        &admission.flow,
        &children[index],
        &cycles[index + 1],
        child.entry().clone(),
        payload.clone(),
      )
      .unwrap()
      .with_admission_profile(&work, &admission.flow, history)
      .unwrap();
      let previous = NodeAttempt::new(
        &children[index],
        &cycles[index + 1],
        child,
        NodeAttemptInput {
          id: NodeAttemptId::generate(),
          node_key: child.entry().clone(),
          node_kind: FlowNodeKind::Reasoning,
          number: NodeAttemptNumber::INITIAL,
          input_digest: input.digest().unwrap(),
          budget: child.nodes()[0].budget(),
          started_at: time(12),
          deadline: time(99),
          execution: NodeExecutionIdentity::External(
            child.nodes()[0].profile_selection().unwrap().candidates[&key(expected)]
              .build
              .tool
              .clone(),
          ),
          ownership: ownership.clone(),
        },
      )
      .unwrap();
      // Selection scope is independent of the cycle; the ordinary journal still owns restart eligibility and bounds.
      let next = WorkflowCycle::restart(WorkflowCycleId::generate(), &cycles[index + 1], &previous).unwrap();
      let repeated = FlowNodeInput::new(
        &work,
        &admission.flow,
        &children[index],
        &next,
        child.entry().clone(),
        payload.clone(),
      )
      .unwrap()
      .with_admission_profile(&work, &admission.flow, history)
      .unwrap();
      for (input, cycle) in [
        (input.clone(), &cycles[index + 1]),
        (input.retry().unwrap(), &cycles[index + 1]),
        (repeated, &next),
      ] {
        let restored = FlowNodeInput::restore(
          &serde_json::to_vec(&input).unwrap(),
          &work,
          &admission.flow,
          &children[index],
          cycle,
          admission.flow.data_schema(payload.schema()).unwrap(),
          input.digest().unwrap(),
        )
        .unwrap();
        let choice = FlowProfileSelectionRecord::select(&restored, &admission.flow, time(12)).unwrap();
        assert_eq!(
          choice.candidate(),
          Some(&key(expected)),
          "the same child definition has independent call sites"
        );
      }
    }
  }

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
      let recovered = selection.recover(&input, &admission.flow, None).unwrap();
      assert_eq!(
        recovered.recovery(),
        if candidate.is_none() {
          Some(FlowProfileSelectionRecovery::ManualSelection)
        } else {
          None
        }
      );
      assert_eq!(recovered.rules(), selection.rules());
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
    let (small, small_input, _) = selection_fixture(
      policy.clone(),
      FlowProfileSelectionFailure::ManualSelection,
      Some((schema.clone(), json!([[true]]))),
    );
    assert_eq!(
      FlowProfileSelectionRecord::select(&small_input, &small.flow, time(11))
        .unwrap()
        .candidate(),
      Some(&key("advanced"))
    );
    let (admission, input, _) = selection_fixture(
      policy,
      FlowProfileSelectionFailure::ManualSelection,
      Some((schema, json!(vec![vec![true; 128]; 128]))),
    );
    let selection = FlowProfileSelectionRecord::select(&input, &admission.flow, time(11)).unwrap();
    assert_eq!(selection.candidate(), None);
    let recovered = selection.recover(&input, &admission.flow, None).unwrap();
    assert_eq!(
      recovered.recovery(),
      Some(FlowProfileSelectionRecovery::ManualSelection)
    );
    assert_eq!(recovered.rules(), selection.rules());
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
  #[test]
  fn selector_failure_uses_only_the_pinned_reserve_and_retains_its_original_limits() {
    let (admission, input, _) = factory_profile_selection_failure_contract_fixture(
      rules(compare("/complexity", json!(2))),
      FlowProfileSelectionFailure::ReserveProfile {
        candidate: key("specialized"),
      },
    );
    let original = FlowProfileSelectionRecord::select(&input, &admission.flow, time(11)).unwrap();
    let choices = factory_profile_selection_capabilities(&admission.flow);
    let recovered = original.recover(&input, &admission.flow, Some(&choices)).unwrap();
    assert_eq!(recovered.candidate(), Some(&key("specialized")));
    assert_eq!(
      serde_json::to_value(&recovered).unwrap()["recovery"],
      json!("reserve_profile")
    );
    assert_eq!(recovered.rules(), original.rules());
    assert_eq!(recovered.input_digest(), original.input_digest());
    assert_eq!(recovered.started_at(), time(11));
    assert_eq!(recovered.deadline(), time(1011));
    assert_eq!(
      FlowProfileSelectionRecord::restore(
        &serde_json::to_vec(&recovered).unwrap(),
        &input,
        &admission.flow,
        recovered.digest().unwrap()
      )
      .unwrap(),
      recovered
    );
    assert_eq!(
      recovered.recover(&input, &admission.flow, None).unwrap(),
      recovered,
      "recovery cannot switch or recurse after commitment"
    );
  }

  #[test]
  fn selector_failure_waits_manually_without_a_usable_configured_reserve() {
    let empty = FactoryConfigurationChoices::try_new(Default::default()).unwrap();
    for (policy, choices, reason) in [
      (FlowProfileSelectionFailure::ManualSelection, None, "manual_selection"),
      (
        FlowProfileSelectionFailure::ReserveProfile {
          candidate: key("specialized"),
        },
        None,
        "reserve_unavailable",
      ),
      (
        FlowProfileSelectionFailure::ReserveProfile {
          candidate: key("specialized"),
        },
        Some(&empty),
        "reserve_unavailable",
      ),
    ] {
      let (admission, input, _) =
        factory_profile_selection_failure_contract_fixture(rules(compare("/complexity", json!(2))), policy);
      let original = FlowProfileSelectionRecord::select(&input, &admission.flow, time(11)).unwrap();
      let recovered = original.recover(&input, &admission.flow, choices).unwrap();
      assert_eq!(recovered.candidate(), None);
      assert_eq!(serde_json::to_value(&recovered).unwrap()["recovery"], json!(reason));
      assert_eq!(recovered.rules(), original.rules());
      assert_eq!(recovered.deadline(), original.deadline());
      assert_eq!(
        recovered
          .recover(
            &input,
            &admission.flow,
            Some(&factory_profile_selection_capabilities(&admission.flow))
          )
          .unwrap(),
        recovered,
        "manual wait cannot trigger another automatic attempt"
      );
      assert_eq!(
        FlowProfileSelectionRecord::restore(
          &serde_json::to_vec(&recovered).unwrap(),
          &input,
          &admission.flow,
          recovered.digest().unwrap()
        )
        .unwrap(),
        recovered
      );
    }
  }

  #[test]
  fn no_matching_rule_uses_the_same_explicit_failure_policy() {
    for (failure, expected, disposition) in [
      (FlowProfileSelectionFailure::ManualSelection, None, "manual_selection"),
      (
        FlowProfileSelectionFailure::ReserveProfile {
          candidate: key("specialized"),
        },
        Some(key("specialized")),
        "reserve_profile",
      ),
    ] {
      let policy = serde_json::from_value(
        json!({"mode":"rules", "rules":[{"when":compare("/count",json!(10)),"candidate":"advanced"}]}),
      )
      .unwrap();
      let (admission, input, _) = factory_profile_selection_failure_contract_fixture(policy, failure);
      let raw = FlowProfileSelectionRecord::select(&input, &admission.flow, time(11)).unwrap();
      assert_eq!(raw.candidate(), None);
      assert_eq!(serde_json::to_value(&raw).unwrap()["rules"], json!({"kind":"no_match"}));
      let selection = raw
        .recover(
          &input,
          &admission.flow,
          Some(&factory_profile_selection_capabilities(&admission.flow)),
        )
        .unwrap();
      assert_eq!(selection.candidate(), expected.as_ref());
      assert_eq!(
        serde_json::to_value(&selection).unwrap()["recovery"],
        json!(disposition)
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
  }
  #[test]
  fn admission_override_precedes_configured_rules_and_provider_selection_without_extra_budget() {
    for policy in [
      FlowProfileSelectionPolicy::Manual {
        candidate: Some(key("advanced")),
      },
      rules(compare("/complexity", json!(2))),
      FlowProfileSelectionPolicy::DecisionApi {
        policy: ImmutableReference::new(key("selection-policy"), key("v1"), FactoryDigest::from_bytes([44; 32])),
        input: FlowDataMapping::new(FlowValueMapping::Constant { value: json!(true) }).unwrap(),
      },
    ] {
      let (admission, original, _) = factory_profile_selection_policy_contract_fixture(policy);
      let node = &admission.flow.closure().definitions()[0].nodes()[0];
      let overrides: FlowProfileAdmissionOverrides = serde_json::from_value(json!([{
        "stage":{"root":admission.flow.closure().root(),"calls":[],"node":node.key()},
        "profile": node.profile_selection().unwrap().candidates[&key("specialized")].profile.reference(),
      }]))
      .unwrap();
      let work = admission
        .work
        .with_profile_overrides(overrides, admission.flow.closure())
        .unwrap();
      let input = FlowNodeInput::new(
        &work,
        &admission.flow,
        admission.flow.root_run(),
        admission.flow.initial_cycle(),
        node.key().clone(),
        original.payload().clone(),
      )
      .unwrap();
      let choice = FlowProfileSelectionRecord::select(&input, &admission.flow, time(11)).unwrap();
      assert_eq!(
        choice.candidate(),
        Some(&key("specialized")),
        "admission choice precedes every automatic mode"
      );
      assert_eq!(choice.rules(), None);
      assert_eq!(choice.recovery(), None);
      assert_eq!(choice.started_at(), time(11));
      assert_eq!(choice.deadline(), time(1011));
      assert_eq!(
        FlowProfileSelectionRecord::restore(
          &serde_json::to_vec(&choice).unwrap(),
          &input,
          &admission.flow,
          choice.digest().unwrap()
        )
        .unwrap(),
        choice
      );
    }
  }

  #[test]
  fn work_admission_retains_exact_profile_override_intent_without_rewriting_the_flow() {
    let (admission, _, _) = factory_profile_selection_contract_fixture(Some("advanced"));
    let definition = &admission.flow.closure().definitions()[0];
    let node = &definition.nodes()[0];
    let profile = node.profile_selection().unwrap().candidates[&key("specialized")]
      .profile
      .reference();
    let mut wire = serde_json::to_value(&admission.work).unwrap();
    wire["profile_overrides"] = json!([{
      "stage": {"root":admission.flow.closure().root(), "calls":[], "node":node.key()},
      "profile":profile,
    }]);
    let work: WorkEnvelope =
      serde_json::from_value(wire.clone()).expect("admission supports explicit profile overrides");
    work
      .profile_overrides()
      .validate(admission.flow.closure(), work.subject().project_id())
      .unwrap();
    assert_eq!(serde_json::to_value(&work).unwrap(), wire);
    assert_eq!(work.profile_overrides().entries()[0].profile, *profile);
    assert!(matches!(&node.profile_selection().unwrap().policy,
      FlowProfileSelectionPolicy::Manual {candidate:Some(candidate)} if candidate == &key("advanced")));
    assert!(
      serde_json::to_value(&admission.work)
        .unwrap()
        .get("profile_overrides")
        .is_none(),
      "legacy Work bytes stay unchanged"
    );
    for (path, value) in [
      ("/profile_overrides/0/stage/node", json!("undeclared")),
      ("/profile_overrides/0/stage/calls", json!([node.key()])),
      (
        "/profile_overrides/0/stage/root/digest",
        json!(FactoryDigest::from_bytes([99; 32])),
      ),
      ("/profile_overrides/0/profile/project_id", json!(ProjectId::generate())),
      (
        "/profile_overrides/0/profile/digest",
        json!(FactoryDigest::from_bytes([99; 32])),
      ),
    ] {
      let mut invalid = wire.clone();
      *invalid.pointer_mut(path).unwrap() = value;
      let invalid: WorkEnvelope = serde_json::from_value(invalid).unwrap();
      assert!(
        invalid
          .profile_overrides()
          .validate(admission.flow.closure(), work.subject().project_id())
          .is_err(),
        "{path}"
      );
    }
    let mut duplicate = wire.clone();
    duplicate["profile_overrides"]
      .as_array_mut()
      .unwrap()
      .push(wire["profile_overrides"][0].clone());
    assert!(serde_json::from_value::<WorkEnvelope>(duplicate).is_err());
    let mut oversized = wire.clone();
    oversized["profile_overrides"] = json!(vec![
      wire["profile_overrides"][0].clone();
      MAX_FLOW_DEFINITION_NODES + 1
    ]);
    assert!(serde_json::from_value::<WorkEnvelope>(oversized).is_err());
    let mut too_deep = wire;
    too_deep["profile_overrides"][0]["stage"]["calls"] =
      json!(vec![node.key(); usize::from(MAX_FLOW_NESTING_DEPTH) + 1]);
    assert!(serde_json::from_value::<WorkEnvelope>(too_deep).is_err());
  }

  #[test]
  fn selected_profile_dependencies_must_remain_available_without_requiring_unused_options() {
    let (admission, _, _) = factory_profile_selection_contract_fixture(Some("advanced"));
    let choices = factory_profile_selection_capabilities(&admission.flow);
    let mut candidate = admission.flow.closure().definitions()[0].nodes()[0]
      .profile_selection()
      .unwrap()
      .candidates[&key("advanced")]
      .clone();
    let project = candidate.profile.reference().project_id;
    let role = FactoryRole::publish(
      FactoryRoleId::generate(),
      project,
      FactoryRoleDefinition {
        name: FactoryText::new("Dependency qualification").unwrap(),
        instructions: "Use the selected dependencies".into(),
        template: None,
        icon: None,
        description: String::new(),
        concurrency_limit: None,
      },
    )
    .unwrap();
    let dependency = |name| ImmutableReference::new(key(name), key("v1"), FactoryDigest::from_bytes([42; 32]));
    let selected = [
      dependency("selected.skill"),
      dependency("selected.mcp"),
      dependency("selected.tool"),
    ];
    let mut definition = candidate.profile.definition().clone();
    definition.skills.insert(key("testing"), selected[0].clone());
    definition.execution.mcp_servers.insert(
      key("tools"),
      FactoryRoleMcpBinding {
        configuration: selected[1].clone(),
        integration: None,
      },
    );
    definition.permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
      tools: vec![selected[2].clone()],
      ..Default::default()
    })
    .unwrap();
    candidate.profile =
      FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, definition.clone()).unwrap();
    let mut capability = choices.role_profile_capabilities().values().next().unwrap().clone();
    capability.ceiling = definition.clone();
    capability.local_permissions = LocalPermissionCeiling::fully_enforced(definition.permissions);
    capability
      .ceiling
      .skills
      .insert(key("unused"), dependency("unused.skill"));
    capability.ceiling.execution.mcp_servers.insert(
      key("unused"),
      FactoryRoleMcpBinding {
        configuration: dependency("unused.mcp"),
        integration: None,
      },
    );
    let mut entries = choices.entries();
    entries
      .references
      .extend(selected.iter().map(|reference| FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: reference.identity().clone(),
        reference: reference.clone(),
      }));
    let installed = FactoryConfigurationChoices::try_new(entries.clone())
      .unwrap()
      .with_role_profile_capability(key("profile.0"), capability.clone())
      .unwrap();
    candidate
      .validate_capabilities(&installed)
      .expect("unused optional dependencies do not hide a usable profile");
    for reference in selected {
      let mut revoked = entries.clone();
      revoked.references.retain(|entry| entry.reference != reference);
      let revoked = FactoryConfigurationChoices::try_new(revoked)
        .unwrap()
        .with_role_profile_capability(key("profile.0"), capability.clone())
        .unwrap();
      assert!(
        candidate.validate_capabilities(&revoked).is_err(),
        "revoked selected dependency: {}",
        reference.identity()
      );
    }
  }

  #[test]
  fn an_installed_profile_cannot_hide_a_revoked_build_dependency_or_missing_enforcement() {
    let (admission, _, _) = factory_profile_selection_contract_fixture(Some("advanced"));
    let choices = factory_profile_selection_capabilities(&admission.flow);
    let candidate = &admission.flow.closure().definitions()[0].nodes()[0]
      .profile_selection()
      .unwrap()
      .candidates[&key("advanced")];
    candidate.validate_capabilities(&choices).unwrap();
    let mut entries = choices.entries();
    entries.build_configurations.clear();
    let mut revoked = FactoryConfigurationChoices::try_new(entries).unwrap();
    for (alias, capability) in choices.role_profile_capabilities() {
      revoked = revoked
        .with_role_profile_capability(alias.clone(), capability.clone())
        .unwrap();
    }
    assert!(
      candidate.validate_capabilities(&revoked).is_err(),
      "a retained runtime declaration cannot substitute a revoked Build configuration"
    );
    let mut unenforced = FactoryConfigurationChoices::try_new(choices.entries()).unwrap();
    for (alias, capability) in choices.role_profile_capabilities() {
      let mut capability = capability.clone();
      capability.local_permissions = LocalPermissionCeiling::try_new(FactoryPermissionSet::deny_all(), vec![]).unwrap();
      unenforced = unenforced
        .with_role_profile_capability(alias.clone(), capability)
        .unwrap();
    }
    assert!(
      candidate.validate_capabilities(&unenforced).is_err(),
      "unavailable enforcement cannot be relaxed to keep a candidate usable"
    );
  }
}
