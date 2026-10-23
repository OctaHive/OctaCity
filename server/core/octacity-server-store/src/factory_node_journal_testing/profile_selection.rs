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
