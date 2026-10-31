//! Characterizes the ordinary typed graph path with no business catalogue or saved node.
use super::*;
use octacity_server_factory::*;

fn authoring_queries()
-> AuthorizedQueryHandler<crate::FactoryFlowDefinitionHandlers<super::profile_selection::PublicationChoices>> {
  let store = Arc::new(super::profile_selection::PublicationChoices(std::sync::Mutex::new(
    FactoryConfigurationChoices::try_new(Default::default()).unwrap(),
  )));
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(crate::FactoryFlowDefinitionHandlers::new(store, policy)),
  )
}

#[test]
fn root_budget_above_admission_names_the_resource_request_and_applicable_maximum() {
  run_ready(async {
    let mut draft = layout_draft();
    let mut execution = serde_json::to_value(&draft.root.execution).unwrap();
    execution["budget"]["max_tokens"] = serde_json::json!(1001);
    draft.root.execution = serde_json::from_value(execution).unwrap();
    let error = authoring_queries()
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: ProjectId::generate(),
          draft,
        },
      )
      .await
      .unwrap_err();
    let ManagementHandlerError::Application(crate::ApplicationError::FactoryValidation(report)) = error else {
      panic!("root budget failure must retain its owner: {error:?}")
    };
    let report = serde_json::to_value(report).unwrap();
    assert_eq!(report["errors"][0]["code"], "budget_exceeds_ceiling");
    assert_eq!(report["errors"][0]["field"], "/execution/budget");
    assert_eq!(report["errors"][0]["ceiling"], "flow_admission");
    assert_eq!(report["errors"][0]["resource"], "tokens");
    assert_eq!(report["errors"][0]["requested"], 1001);
    assert_eq!(report["errors"][0]["maximum"], 1000);
  });
}

#[test]
fn definition_capacity_above_run_wip_names_its_admission_ceiling() {
  run_ready(async {
    let mut draft = layout_draft();
    let mut execution = serde_json::to_value(&draft.root.execution).unwrap();
    execution["max_active_nodes"] = serde_json::json!(2);
    draft.root.execution = serde_json::from_value(execution).unwrap();
    let error = authoring_queries()
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: ProjectId::generate(),
          draft,
        },
      )
      .await
      .unwrap_err();
    let ManagementHandlerError::Application(crate::ApplicationError::FactoryValidation(report)) = error else {
      panic!("Flow capacity failure must retain its owner: {error:?}")
    };
    let report = serde_json::to_value(report).unwrap();
    assert_eq!(report["errors"][0]["code"], "concurrency_exceeds_ceiling");
    assert_eq!(report["errors"][0]["field"], "/execution/max_active_nodes");
    assert_eq!(report["errors"][0]["ceiling"], "flow_admission");
    assert_eq!(report["errors"][0]["resource"], "active_nodes");
    assert_eq!(report["errors"][0]["requested"], 2);
    assert_eq!(report["errors"][0]["maximum"], 1);
  });
}

#[test]
fn over_ceiling_step_budget_names_the_enclosing_owner_without_silently_clamping() {
  run_ready(async {
    let project = ProjectId::generate();
    let queries = authoring_queries();
    let mut draft = layout_draft();
    let mut node = serde_json::to_value(&draft.root.nodes[0]).unwrap();
    node["budget"]["max_tokens"] = serde_json::json!(1001);
    draft.root.nodes[0] = serde_json::from_value(node).unwrap();
    let error = queries
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: project,
          draft: draft.clone(),
        },
      )
      .await
      .unwrap_err();
    let ManagementHandlerError::Application(crate::ApplicationError::FactoryValidation(report)) = error else {
      panic!("budget failure must name its policy owner: {error:?}")
    };
    let report = serde_json::to_value(report).unwrap();
    assert_eq!(report["errors"][0]["code"], "budget_exceeds_ceiling");
    assert_eq!(report["errors"][0]["field"], "/budget");
    assert_eq!(report["errors"][0]["node"], "measure_layout");
    assert_eq!(report["errors"][0]["ceiling"], "flow_definition");
    assert_eq!(report["errors"][0]["resource"], "tokens");
    assert_eq!(report["errors"][0]["requested"], 1001);
    assert_eq!(report["errors"][0]["maximum"], 1000);
    assert_eq!(draft.root.nodes[0].budget().max_tokens(), 1001);
  });
}

fn layout_draft() -> crate::FactoryFlowDefinitionDraft {
  let document = FlowDataSchema::new(
    key("layout.document"),
    key("v1"),
    FlowValueSchema::Object {
      fields: [
        (
          key("caption"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 1,
            max_bytes: 256,
          }),
        ),
        (
          key("private_notes"),
          FlowFieldSchema::optional(FlowValueSchema::String {
            min_bytes: 0,
            max_bytes: 1024,
          }),
        ),
      ]
      .into(),
    },
  )
  .unwrap();
  let caption = FlowDataSchema::new(
    key("layout.caption"),
    key("v1"),
    FlowValueSchema::String {
      min_bytes: 1,
      max_bytes: 256,
    },
  )
  .unwrap();
  let permissions = FactoryPermissionSet::default();
  let mut nodes = vec![];
  for (index, (name, outcome, schema)) in [
    ("measure_layout", "measured", &document),
    ("proof_caption", "proofed", &caption),
    ("package_caption", "packaged", &caption),
  ]
  .into_iter()
  .enumerate()
  {
    let program = FlowGateProgram::new(vec![], key(outcome))
      .unwrap()
      .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap())
      .unwrap();
    let mut node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: key(name),
      kind: FlowNodeKind::DeterministicGate,
      input_schema: Some(schema.reference().clone()),
      outcomes: vec![FlowOutcomeDefinition::new(
        key(outcome),
        FlowOutcomeKind::Success,
        schema.reference().clone(),
      )],
      budget: budget(1),
      permissions: permissions.clone(),
      required: true,
      subflow: None,
    })
    .unwrap()
    .with_gate(program.binding().unwrap())
    .unwrap();
    if index > 0 {
      let (predecessor, accepted, path) = if index == 1 {
        ("measure_layout", "measured", "/caption")
      } else {
        ("proof_caption", "proofed", "")
      };
      node = node
        .with_input_binding(
          FlowInputBinding::new(
            [(
              key("selected"),
              FlowInputSource::Record {
                node: key(predecessor),
                outcome: key(accepted),
                view: FlowRecordView::Payload,
                path: path.into(),
              },
            )]
            .into(),
            FlowDataMapping::new(FlowValueMapping::Pointer {
              path: "/selected".into(),
            })
            .unwrap(),
          )
          .unwrap(),
        )
        .unwrap();
    }
    nodes.push(node);
  }
  crate::FactoryFlowDefinitionDraft {
    root: crate::FactoryFlowRootDraft {
      code_sources: None,
      process: None,
      input_schema: Some(document.reference().clone()),
      entry: key("measure_layout"),
      nodes,
      transitions: vec![
        FlowTransition::new(
          key("measure_layout"),
          key("measured"),
          FlowTransitionTarget::Node(key("proof_caption")),
        ),
        FlowTransition::new(
          key("proof_caption"),
          key("proofed"),
          FlowTransitionTarget::Node(key("package_caption")),
        ),
        FlowTransition::new(
          key("package_caption"),
          key("packaged"),
          FlowTransitionTarget::Terminal(key("caption_ready")),
        ),
      ],
      terminals: vec![FlowTerminalDefinition::new(
        key("caption_ready"),
        caption.reference().clone(),
      )],
      context_projections: vec![],
      data_projections: vec![],
      execution: FlowExecutionPolicy::new(budget(10), permissions.clone(), 1).unwrap(),
    },
    limits: FlowAdmissionLimits::product_defaults(1, budget(10), permissions).unwrap(),
    data_schemas: vec![document, caption],
    pools: Default::default(),
  }
}

#[test]
fn unrelated_three_step_graph_accepts_custom_structural_results_and_explicit_field_handoffs() {
  run_ready(async {
    let project = ProjectId::generate();
    let queries = authoring_queries();
    let draft = layout_draft();
    let flow = queries
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: project,
          draft: draft.clone(),
        },
      )
      .await
      .unwrap();
    let definition = flow.closure.definition(flow.closure.root()).unwrap();
    assert_eq!(definition.nodes().len(), 3);
    assert!(definition.node(&key("package_caption")).is_some());
    let handoff =
      serde_json::to_value(definition.node(&key("proof_caption")).unwrap().input_binding().unwrap()).unwrap();
    assert_eq!(handoff["sources"]["selected"]["path"], "/caption");
    assert!(!handoff.to_string().contains("private_notes"));
    let mut invalid = serde_json::to_value(draft).unwrap();
    invalid["root"]["nodes"][1]["input_binding"]["sources"]["selected"]["outcome"] =
      serde_json::json!("model_claimed_verified");
    let invalid: crate::FactoryFlowDefinitionDraft = serde_json::from_value(invalid).unwrap();
    assert!(
      queries
        .handle_query(
          &context(),
          crate::ValidateFactoryFlowDefinitionQuery {
            id: FlowDefinitionId::generate(),
            project_id: project,
            draft: invalid
          }
        )
        .await
        .is_err()
    );
  });
}

#[test]
fn ordinary_document_context_keeps_exact_instruction_and_reference_roles_without_permissions_or_sibling_attachment() {
  run_ready(async {
    let artifact = FactoryArtifactReference::new(
      octacity_server_domain::ArtifactId::generate(),
      FactoryDigest::from_bytes([41; 32]),
      20,
    )
    .unwrap();
    let mut draft = layout_draft();
    let mut entries = std::collections::BTreeMap::new();
    for (name, kind) in [
      ("reviewed-guidance", ContextSourceKind::Policy),
      ("reference-material", ContextSourceKind::Specification),
    ] {
      entries.insert(
        key(name),
        FlowContextSelection {
          when_present: None,
          source_kind: kind,
          source: FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::json!({"kind":"artifact", "value":artifact}),
          })
          .unwrap(),
        },
      );
    }
    draft.root.nodes[0] = draft.root.nodes[0]
      .clone()
      .with_context_binding(FlowContextBinding::new(None, entries).unwrap())
      .unwrap();
    let flow = authoring_queries()
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: ProjectId::generate(),
          draft,
        },
      )
      .await
      .unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    let selected = root.node(&key("measure_layout")).unwrap();
    let binding = selected.context_binding().unwrap();
    let wire = serde_json::to_value(binding).unwrap();
    assert_eq!(wire["entries"]["reviewed-guidance"]["source_kind"], "policy");
    assert_eq!(wire["entries"]["reference-material"]["source_kind"], "specification");
    for (_, source) in binding.static_sources() {
      assert_eq!(source.source, Some(FactoryContextReference::Artifact(artifact.clone())));
    }
    assert_eq!(selected.permissions(), &FactoryPermissionSet::default());
    assert!(root.node(&key("proof_caption")).unwrap().context_binding().is_none());
  });
}

#[test]
fn common_process_inputs_freeze_exact_publications_only_for_explicit_step_recipients() {
  run_ready(async {
    let goals = serde_json::json!({"artifact_id":octacity_server_domain::ArtifactId::generate(),"content_digest":"31".repeat(32),"encoded_size":80});
    let criteria = serde_json::json!({"artifact_id":octacity_server_domain::ArtifactId::generate(),"content_digest":"42".repeat(32),"encoded_size":120});
    let process = serde_json::json!({
      "goals":{"artifact":goals,"recipients":["measure_layout"]},
      "criteria":{"artifact":criteria,"recipients":["measure_layout","proof_caption"]}
    });
    let mut wire = serde_json::to_value(layout_draft()).unwrap();
    wire["root"]["process"] = process.clone();
    let draft: crate::FactoryFlowDefinitionDraft = serde_json::from_value(wire).unwrap();
    let flow = authoring_queries()
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: ProjectId::generate(),
          draft,
        },
      )
      .await
      .unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    let root_wire = serde_json::to_value(root).unwrap();
    assert_eq!(root_wire["process"], process);
    let selected = serde_json::to_value(root.node(&key("measure_layout")).unwrap()).unwrap();
    assert_eq!(
      selected["context_binding"]["entries"]["process.goals"]["source"]["value"]["value"]["value"],
      goals
    );
    assert_eq!(
      selected["context_binding"]["entries"]["process.criteria"]["source_kind"],
      "specification"
    );
    let second = serde_json::to_value(root.node(&key("proof_caption")).unwrap()).unwrap();
    assert!(second["context_binding"]["entries"]["process.goals"].is_null());
    assert_eq!(
      second["context_binding"]["entries"]["process.criteria"]["source"]["value"]["value"]["value"],
      criteria
    );
    assert!(root.node(&key("package_caption")).unwrap().context_binding().is_none());
    assert_eq!(
      root.node(&key("measure_layout")).unwrap().permissions(),
      &FactoryPermissionSet::default()
    );
    let restored: FactoryFlowConfiguration = serde_json::from_value(serde_json::to_value(&flow).unwrap()).unwrap();
    assert_eq!(restored.closure.root(), flow.closure.root());
  });
}

#[test]
fn explicitly_clearing_process_inputs_detaches_only_owned_entries_and_preserves_the_old_flow() {
  run_ready(async {
    let id = FlowDefinitionId::generate();
    let project = ProjectId::generate();
    let mut wire = serde_json::to_value(layout_draft()).unwrap();
    wire["root"]["process"] = serde_json::json!({"criteria":{
      "artifact":{"artifact_id":octacity_server_domain::ArtifactId::generate(),"content_digest":"42".repeat(32),"encoded_size":120},
      "recipients":["measure_layout"]
    }});
    let original = authoring_queries()
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id,
          project_id: project,
          draft: serde_json::from_value(wire).unwrap(),
        },
      )
      .await
      .unwrap();
    let before = serde_json::to_value(&original).unwrap();
    let mut root = serde_json::to_value(original.closure.definition(original.closure.root()).unwrap()).unwrap();
    root.as_object_mut().unwrap().remove("reference");
    root["process"] = serde_json::json!({});
    let mut draft = layout_draft();
    draft.root = serde_json::from_value(root).unwrap();
    let cleared = authoring_queries()
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id,
          project_id: project,
          draft,
        },
      )
      .await
      .unwrap();
    let definition = cleared.closure.definition(cleared.closure.root()).unwrap();
    assert!(
      definition
        .node(&key("measure_layout"))
        .unwrap()
        .context_binding()
        .is_none()
    );
    assert_ne!(cleared.closure.root(), original.closure.root());
    assert_eq!(serde_json::to_value(&original).unwrap(), before);
  });
}

#[test]
fn editing_process_publications_changes_only_the_new_flow_and_rejects_unknown_recipients() {
  run_ready(async {
    let id = FlowDefinitionId::generate();
    let project = ProjectId::generate();
    let mut wire = serde_json::to_value(layout_draft()).unwrap();
    let first = serde_json::json!({"artifact_id":octacity_server_domain::ArtifactId::generate(),"content_digest":"42".repeat(32),"encoded_size":120});
    let second = serde_json::json!({"artifact_id":octacity_server_domain::ArtifactId::generate(),"content_digest":"53".repeat(32),"encoded_size":140});
    wire["root"]["process"] = serde_json::json!({"criteria":{"artifact":first,"recipients":["measure_layout"]}});
    let original = authoring_queries()
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id,
          project_id: project,
          draft: serde_json::from_value(wire).unwrap(),
        },
      )
      .await
      .unwrap();
    let retained = serde_json::to_value(&original).unwrap();
    let mut root = serde_json::to_value(original.closure.definition(original.closure.root()).unwrap()).unwrap();
    root.as_object_mut().unwrap().remove("reference");
    root["process"]["criteria"]["artifact"] = second.clone();
    let mut draft = layout_draft();
    draft.root = serde_json::from_value(root.clone()).unwrap();
    let edited = authoring_queries()
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id,
          project_id: project,
          draft,
        },
      )
      .await
      .unwrap();
    let edited = serde_json::to_value(edited.closure.definition(edited.closure.root()).unwrap()).unwrap();
    let node = edited["nodes"]
      .as_array()
      .unwrap()
      .iter()
      .find(|node| node["key"] == "measure_layout")
      .unwrap();
    assert_eq!(
      node["context_binding"]["entries"]["process.criteria"]["source"]["value"]["value"]["value"],
      second
    );
    assert_eq!(serde_json::to_value(&original).unwrap(), retained);
    root["process"]["criteria"]["recipients"] = serde_json::json!(["undeclared-step"]);
    let mut draft = layout_draft();
    draft.root = serde_json::from_value(root).unwrap();
    assert!(
      authoring_queries()
        .handle_query(
          &context(),
          crate::ValidateFactoryFlowDefinitionQuery {
            id,
            project_id: project,
            draft,
          }
        )
        .await
        .is_err()
    );
  });
}

#[path = "factory_code_authoring_tests.rs"]
mod code_authoring;
