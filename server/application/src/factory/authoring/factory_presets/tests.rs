//! Resolver conformance with independently installed capabilities and a published predecessor.
use super::*;
use crate::factory::admission::tests::factory_admission_tests::{repository, run_ready};
use crate::factory::authoring::tests::factory_configuration_cqrs_tests::PublicationChoices;
use crate::factory::execution::factory_test_support::installed_preset_choices;
use crate::factory_node_test_support::{fixtures::key, probe_observation_contract};
use octacity_server_domain::{PipelineNodeId, ProjectId, RepositoryId, Timestamp};
use octacity_server_store::{
  PublishedRepository,
  testing::{InMemoryFactoryConfigurationStore, MutationEvidenceProbe, role_profile_contract_intent},
};
use std::collections::BTreeMap;

#[test]
fn resolved_delivery_is_an_editable_ordinary_flow_with_no_implicit_activation() {
  run_ready(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let [delivery, triage] = RELEASES;
    let (template, preset) = delivery.load().unwrap();
    let (mut triage, _) = triage.load().unwrap();
    // Successful triage reaches this completion only after a bounded repeat.
    for outcome in triage.nodes.iter_mut().flat_map(|node| &mut node.outcomes) {
      outcome.issue_presentation = None;
    }
    let route = triage
      .transitions
      .iter_mut()
      .find(|route| route.outcome() == &key("requirements"))
      .unwrap();
    *route = FlowTransition::repeated(
      route.predecessor().clone(),
      route.outcome().clone(),
      FlowTransitionTarget::Node(route.predecessor().clone()),
      1,
      key("triaged"),
    )
    .unwrap();
    let (installed, definition) = installed_preset_choices(project);
    let build = FlowBuildBinding {
      tool: definition.toolchain.executable.clone(),
      plugin: definition.toolchain.plugin.clone(),
      model_or_tool: definition.toolchain.model.clone(),
      build_configuration: definition.build_configuration.clone(),
      task_digest: FactoryDigest::from_bytes([34; 32]),
      result_output: key("report"),
      result_output_kind: EvidenceOutputKind::Report,
      result_outcome: key("observed"),
      result_tool: None,
      capture_write_paths: None,
      capture_deliverables: None,
      evidence: vec![],
    };
    let predecessor = triage
      .instantiate(
        FlowDefinitionId::generate(),
        FlowDefinitionVersion::INITIAL,
        &[
          (key("eligibility"), build.clone()),
          (key("classification"), build.clone()),
        ]
        .into(),
      )
      .unwrap();
    let predecessor_ref = predecessor.definition.reference();
    let predecessor = FactoryFlowConfiguration::new(
      PinnedFlowDefinitionClosure::new(predecessor_ref, vec![predecessor.definition]).unwrap(),
      FlowAdmissionLimits::product_defaults(
        triage.execution.max_active_nodes(),
        triage.execution.budget(),
        triage.execution.permissions().clone(),
      )
      .unwrap(),
      predecessor.schemas,
      Default::default(),
    )
    .unwrap();
    let repository = repository(project, RepositoryId::generate(), None);
    let repository_ref = crate::factory::execution::factory_code_sources::repository_reference(&repository).unwrap();
    let published_at = Timestamp::from_unix_millis(1).unwrap();
    let predecessor = PublishedFactoryFlowDefinition {
      project_id: project,
      flow: predecessor,
      published_at,
    };
    let reader = Arc::new(AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(Prerequisites {
        repository,
        flow: predecessor.clone(),
      }),
    ));
    let schemas = template.data_schemas().unwrap();
    let receipt_schema = schemas[&key("pull_request_receipt")].clone();
    let target = ImmutableReference::new(key("review"), key("v1"), FactoryDigest::from_bytes([44; 32]));
    let mut installed = installed
      .with_build_pipeline_contract(FactoryBuildPipelineContract {
        protected_tests: None,
        command: None,
        build_configuration: build.build_configuration.clone(),
        tool: build.tool.clone(),
        plugin: build.plugin.clone(),
        model_or_tool: build.model_or_tool.clone(),
        pipeline_node: PipelineNodeId::new("step").unwrap(),
        compilation: Some(FactoryBuildCompilationContract {
          evidence: vec![],
          task_digest: build.task_digest,
          role_profiles: true,
          requires_task_context: false,
          result_output: build.result_output.clone(),
          result_output_kind: build.result_output_kind,
          mcp_result_tool: None,
        }),
      })
      .unwrap();
    store.enable_project(project, installed.clone()).unwrap();
    let mut intent = role_profile_contract_intent(project);
    intent.profiles.truncate(1);
    intent.profiles[0].definition = definition.clone();
    let executor = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(FactoryRoleProfileHandlers::with_policy(store.clone(), policy.clone())),
    )
    .handle_command(
      &context,
      PublishFactoryRoleProfilesCommand {
        intent,
        published_at,
        idempotency_key: "delivery-profiles".parse().unwrap(),
      },
    )
    .await
    .unwrap();
    let mut profiles = BTreeMap::new();
    let mut commands = BTreeMap::new();
    for node in template.nodes.iter().filter(|node| node.build_profile.is_some()) {
      let alias = node.build_profile.clone().unwrap();
      if node.kind == FlowNodeKind::BuildCommand {
        let result = schemas
          .get(&node.outcomes[0].schema)
          .cloned()
          .unwrap_or_else(|| probe_observation_contract().report_schema().unwrap());
        let mut command = build.clone();
        if node.key == key("protected_validation") {
          command.evidence.push(EvidenceRequirement::new(
            key("validation_facts"),
            EvidenceOutputKind::Report,
            schemas[&key("protected_validation_facts")].reference().clone(),
            command.tool.clone(),
            command.plugin.clone(),
          ));
        }
        command.model_or_tool = ImmutableReference::new(
          alias.clone(),
          key("v1"),
          FactoryDigest::content_sha256(alias.as_str().as_bytes()),
        );
        installed = installed
          .with_reference_choice(FactoryReferenceChoice {
            alias: alias.clone(),
            kind: FactoryChoiceKind::NodeCapability,
            reference: command.model_or_tool.clone(),
          })
          .unwrap()
          .with_build_pipeline_contract(FactoryBuildPipelineContract {
            protected_tests: None,
            command: Some(FactoryBuildCommandContract {
              task_digest: command.task_digest,
              outputs: std::iter::once(EvidenceRequirement::new(
                command.result_output.clone(),
                command.result_output_kind,
                result.reference().clone(),
                command.tool.clone(),
                command.plugin.clone(),
              ))
              .chain(command.evidence.iter().cloned())
              .collect(),
              observation_schema: Some(result.clone()),
              permissions: Default::default(),
            }),
            build_configuration: command.build_configuration.clone(),
            tool: command.tool.clone(),
            plugin: command.plugin.clone(),
            model_or_tool: command.model_or_tool.clone(),
            pipeline_node: PipelineNodeId::new("step").unwrap(),
            compilation: None,
          })
          .unwrap();
        commands.insert(
          alias,
          FactoryPresetCommandSelection {
            build: command,
            result,
            permissions: Default::default(),
          },
        );
      } else {
        profiles.insert(
          alias,
          FactoryPresetProfileSelection {
            role: executor.role.as_ref().unwrap().reference().clone(),
            profile: executor.profiles[0].reference().clone(),
            build: Some(build.clone()),
          },
        );
      }
    }
    let base_choices = installed.clone();
    let installed = installed
      .with_candidate_publication(FactoryCandidatePublicationChoice {
        plugin: build.plugin.clone(),
        target: target.clone(),
        repository_id: repository_ref.id,
        outcome: FlowOutcomeDefinition::new(
          key("published"),
          FlowOutcomeKind::Success,
          receipt_schema.reference().clone(),
        ),
        receipt_schema: Some(receipt_schema),
        review_url: true,
        permissions: Default::default(),
      })
      .unwrap();
    store.enable_project(project, installed.clone()).unwrap();
    let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
      plugins: vec![build.plugin.clone()],
      executables: definition.permissions.executables().cloned().collect(),
      commands: definition.permissions.commands().cloned().collect(),
      ..Default::default()
    })
    .unwrap();
    let flow_id = FlowDefinitionId::generate();
    let selection = FactoryPresetResolutionRequest {
      preset,
      flow_only: false,
      flow_id: Some(flow_id),
      configuration_id: Some(FactoryConfigurationId::generate()),
      profiles,
      repository: Some(repository_ref),
      context: Default::default(),
      execution: Some(FlowExecutionPolicy::new(template.execution.budget(), permissions.clone(), 1).unwrap()),
      configuration: Some(FactoryPresetConfigurationParameters {
        admission_policy: key("admission"),
        permission_ceiling: key("permissions"),
        wip_limits: FactoryWipLimits::new(1, 1).unwrap(),
        hard_budget: template.execution.budget(),
      }),
      input: None,
      effects: Default::default(),
      commands,
      publication: Some(FactoryPresetPublicationSelection {
        plugin: build.plugin,
        target,
        permissions,
      }),
      accepted_predecessor: Some(FlowAcceptedPredecessor {
        definition: predecessor_ref,
        terminal: key("triaged"),
        outcomes: [key("requirements")].into(),
      }),
    };
    let resolver = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(
        FactoryPresetHandlers::new(store.clone(), policy)
          .with_repositories(reader.clone())
          .with_flows(reader),
      ),
    );
    let before = store.mutation_evidence_counts().await;
    let mut role_only = selection.clone();
    role_only
      .commands
      .get_mut(&key("reproduction"))
      .unwrap()
      .build
      .model_or_tool = build.model_or_tool.clone();
    let rejected = resolver
      .handle_query(
        &context,
        ResolveFactoryPresetQuery {
          project_id: project,
          selection: role_only,
        },
      )
      .await
      .unwrap();
    assert!(rejected.flow.is_none());
    assert_eq!(rejected.diagnostics[0].path, "/commands/reproduction");
    let result = resolver
      .handle_query(
        &context,
        ResolveFactoryPresetQuery {
          project_id: project,
          selection: selection.clone(),
        },
      )
      .await
      .unwrap();
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert!(result.input.is_none());
    let draft = result.flow.unwrap();
    let flow_only = resolver
      .handle_query(
        &context,
        ResolveFactoryPresetQuery {
          project_id: project,
          selection: FactoryPresetResolutionRequest {
            flow_only: true,
            configuration_id: None,
            configuration: None,
            repository: None,
            ..selection
          },
        },
      )
      .await
      .unwrap();
    assert!(flow_only.diagnostics.is_empty(), "{:?}", flow_only.diagnostics);
    let mut editable = flow_only.flow.unwrap();
    assert_eq!(editable, draft);
    assert!(editable.root.code_sources.as_ref().is_some_and(BTreeMap::is_empty));
    assert!(
      editable
        .root
        .nodes
        .iter()
        .all(|node| node.code() == Some(&FlowCodeBinding::None))
    );
    assert!(flow_only.configuration.is_none() && flow_only.input.is_none());
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert!(
      draft
        .root
        .nodes
        .iter()
        .filter(|node| node.kind() == FlowNodeKind::Reasoning)
        .all(|node| node.task().is_some())
    );
    assert_eq!(
      draft.root.name.as_ref().map(FactoryText::as_str),
      Some("Ticket delivery")
    );
    let configuration = result.configuration.unwrap();
    assert!(!configuration.enabled);
    let pinned = configuration.flow.clone().unwrap();
    assert_eq!(pinned.closure.root().id(), flow_id);
    assert_eq!(draft.accepted_predecessor, pinned.accepted_predecessor);
    let root = pinned.closure.definition(pinned.closure.root()).unwrap();
    assert_eq!(draft.root.nodes, root.nodes());
    assert!(
      draft
        .root
        .nodes
        .iter()
        .any(|node| node.key().as_str() == "implementation"
          && node.build_bindings().all(|build| build.capture_deliverables.is_some()))
    );

    // An inline configuration can introduce a new definition without calling Flow publication first.
    let mut nodes = root.nodes().to_vec();
    let publisher = nodes
      .iter_mut()
      .find(|node| node.key() == &key("publish_pull_request"))
      .unwrap();
    let mut wire = serde_json::to_value(&*publisher).unwrap();
    let parameters = wire["action"]["parameters"].as_object_mut().unwrap();
    parameters.remove("valid_until");
    parameters.remove("review_valid_until");
    *publisher = serde_json::from_value(wire).unwrap();
    let inline = FlowDefinition::new(FlowDefinitionInput {
      id: FlowDefinitionId::generate(),
      version: FlowDefinitionVersion::INITIAL,
      input_schema: root.input_schema().cloned(),
      entry: root.entry().clone(),
      nodes,
      transitions: root.transitions().to_vec(),
      terminals: root.terminals().to_vec(),
      context_projections: root.context_projections().to_vec(),
      data_projections: root.data_projections().to_vec(),
      execution: root.execution().clone(),
    })
    .unwrap();
    let mut inline_flow = pinned.clone();
    inline_flow.closure = PinnedFlowDefinitionClosure::new(inline.reference(), vec![inline]).unwrap();
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let configuration_validation = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(FactoryConfigurationHandlers::with_policy(store.clone(), policy)),
    );
    let mut inline_configuration = configuration;
    inline_configuration.flow = Some(inline_flow);
    let result = configuration_validation
      .handle_query(
        &context,
        ValidateFactoryConfigurationQuery {
          id: FactoryConfigurationId::generate(),
          project_id: project,
          definition_digest: FactoryDigest::from_bytes([80; 32]),
          draft: inline_configuration,
        },
      )
      .await;
    let rejected_inline_expiry = matches!(
      result, Err(ManagementHandlerError::Application(ApplicationError::FactoryValidation(report)))
        if report.errors[0].field == "/action/parameters/valid_until"
    );

    // The same publication owner also serves a draft assembled in Add pipeline.
    editable.root.name = Some(FactoryText::new("Project delivery").unwrap());
    let validation = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(FactoryFlowDefinitionHandlers::new(
        Arc::new(PublicationChoices::for_publication(installed.clone()).with_predecessor(predecessor.clone())),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let bad_receipt = FlowDataSchema::new(
      key("bad-publication-receipt"),
      key("v1"),
      FlowValueSchema::String {
        min_bytes: 1,
        max_bytes: 64,
      },
    )
    .unwrap();
    let mut target = installed.candidate_publications()[0].clone();
    target.target = ImmutableReference::new(key("wrong-receipt"), key("v1"), FactoryDigest::from_bytes([45; 32]));
    target.outcome = FlowOutcomeDefinition::new(
      key("published"),
      FlowOutcomeKind::Success,
      bad_receipt.reference().clone(),
    );
    target.receipt_schema = Some(bad_receipt.clone());
    let receipt_validation = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(FactoryFlowDefinitionHandlers::new(
        Arc::new(
          PublicationChoices::for_publication(installed.clone().with_candidate_publication(target.clone()).unwrap())
            .with_predecessor(predecessor.clone()),
        ),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let mut rejected = Vec::new();
    if rejected_inline_expiry {
      rejected.push("inline-missing-expiry");
    }
    for (case, field) in [
      ("target", "/action/target"),
      ("context", "/context_binding"),
      ("candidate", "/context_binding"),
      ("card", "/outcomes/issue_presentation"),
      ("failure-format", "/outcomes"),
      ("expiry-path", "/action/parameters/valid_until"),
      ("expiry-out-of-range", "/action/parameters/valid_until"),
      ("expiry-literal", "/action/parameters/valid_until"),
      ("expiry-input-literal", "/action/parameters/valid_until"),
      ("expiry-gate-literal", "/action/parameters/review_valid_until"),
      ("missing-validation-expiry", "/action/parameters/valid_until"),
      ("missing-review-expiry", "/action/parameters/review_valid_until"),
      ("review-expiry-type", "/action/parameters/review_valid_until"),
      ("metadata-type", "/action/parameters/review_metadata"),
      ("receipt-format", "/outcomes"),
    ] {
      let mut draft = editable.clone();
      let edited = if case == "expiry-gate-literal" {
        "code_review_gate"
      } else {
        "publish_pull_request"
      };
      let node = draft
        .root
        .nodes
        .iter_mut()
        .find(|node| node.key() == &key(edited))
        .unwrap();
      let mut wire = serde_json::to_value(&*node).unwrap();
      match case {
        "target" => wire["action"]["parameters"]["target"]["identity"] = serde_json::json!("missing-target"),
        "context" => {
          wire.as_object_mut().unwrap().remove("context_binding");
        }
        "candidate" => wire["context_binding"]["candidate_revision"] = serde_json::Value::Null,
        "expiry-path" => {
          wire["action"]["parameters"]["valid_until"]["value"]["path"] = serde_json::json!("/valid_untill")
        }
        "expiry-out-of-range" => {
          wire["action"]["parameters"]["valid_until"] = serde_json::json!({"value":{"mapping":"constant","value":-1}})
        }
        "expiry-literal" => {
          wire["action"]["parameters"]["valid_until"] =
            serde_json::json!({"value":{"mapping":"constant","value":2000000000000_i64}})
        }
        "expiry-input-literal" => {
          wire["input_binding"]["mapping"]["value"]["fields"]["valid_until"] =
            serde_json::json!({"mapping":"constant","value":2000000000000_i64})
        }
        "expiry-gate-literal" => {
          wire["input_binding"]["mapping"]["value"]["fields"]["fresh_until"] =
            serde_json::json!({"mapping":"constant","value":2000000000000_i64})
        }
        "missing-validation-expiry" | "missing-review-expiry" => {
          wire["action"]["parameters"]
            .as_object_mut()
            .unwrap()
            .remove(if case == "missing-validation-expiry" {
              "valid_until"
            } else {
              "review_valid_until"
            });
        }
        "review-expiry-type" => {
          wire["action"]["parameters"]["review_valid_until"]["value"]["path"] = serde_json::json!("/metadata/title")
        }
        "metadata-type" => {
          wire["action"]["parameters"]["review_metadata"]["value"]["path"] = serde_json::json!("/valid_until")
        }
        "receipt-format" => {
          wire["action"]["parameters"]["target"] = serde_json::to_value(&target.target).unwrap();
          let published = wire["outcomes"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|outcome| outcome["key"] == "published")
            .unwrap();
          *published = serde_json::to_value(&target.outcome).unwrap();
          let terminal = draft
            .root
            .terminals
            .iter_mut()
            .find(|terminal| terminal.key() == &key("delivered_for_review"))
            .unwrap();
          *terminal = FlowTerminalDefinition::new(terminal.key().clone(), bad_receipt.reference().clone());
          draft.data_schemas.push(bad_receipt.clone());
        }
        "failure-format" => {
          let text = FlowDataSchema::new(
            key("invalid-failure-format"),
            key("v1"),
            FlowValueSchema::String {
              min_bytes: 0,
              max_bytes: 128,
            },
          )
          .unwrap();
          for failure in wire["outcomes"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .filter(|outcome| outcome["kind"] == "failure")
          {
            failure["schema"] = serde_json::to_value(text.reference()).unwrap();
          }
          let terminal = draft
            .root
            .terminals
            .iter_mut()
            .find(|terminal| terminal.key() == &key("publication_blocked"))
            .unwrap();
          *terminal = FlowTerminalDefinition::new(terminal.key().clone(), text.reference().clone());
          draft.data_schemas.push(text);
        }
        _ => {
          let mut unrelated = serde_json::to_value(node.outcome(&key("published")).unwrap()).unwrap();
          unrelated["key"] = serde_json::json!("unrelated");
          unrelated["issue_presentation"] = serde_json::json!({"kind":"publication_v1"});
          wire["outcomes"].as_array_mut().unwrap().push(unrelated);
          let terminal = draft
            .root
            .transitions
            .iter()
            .find(|route| route.predecessor() == &key("publish_pull_request") && route.outcome() == &key("published"))
            .unwrap()
            .target()
            .clone();
          draft.root.transitions.push(FlowTransition::new(
            key("publish_pull_request"),
            key("unrelated"),
            terminal,
          ));
        }
      }
      *node = serde_json::from_value(wire).unwrap();
      let handler = if case == "receipt-format" {
        &receipt_validation
      } else {
        &validation
      };
      let result = handler
        .handle_query(
          &context,
          ValidateFactoryFlowDefinitionQuery {
            id: flow_id,
            project_id: project,
            draft,
          },
        )
        .await;
      if matches!(result, Err(ManagementHandlerError::Application(ApplicationError::FactoryValidation(report))) if report.errors[0].field == field)
      {
        rejected.push(case);
      }
    }
    assert_eq!(
      rejected,
      [
        "inline-missing-expiry",
        "target",
        "context",
        "candidate",
        "card",
        "failure-format",
        "expiry-path",
        "expiry-out-of-range",
        "expiry-literal",
        "expiry-input-literal",
        "expiry-gate-literal",
        "missing-validation-expiry",
        "missing-review-expiry",
        "review-expiry-type",
        "metadata-type",
        "receipt-format"
      ],
      "ordinary authoring must validate the installed publisher and its inputs"
    );
    let mut legacy_draft = editable.clone();
    let mut legacy = installed.candidate_publications()[0].clone();
    let FlowValueSchema::Object { mut fields } = legacy.receipt_schema.take().unwrap().shape().clone() else {
      unreachable!("the configured receipt is an object");
    };
    fields.insert(
      key("changeset_digest"),
      FlowFieldSchema::required(FlowValueSchema::Enum {
        values: vec![serde_json::json!(FactoryDigest::from_bytes([7; 32]))],
      }),
    );
    let legacy_receipt =
      FlowDataSchema::new(key("legacy-receipt"), key("v1"), FlowValueSchema::Object { fields }).unwrap();
    let node = legacy_draft
      .root
      .nodes
      .iter_mut()
      .find(|node| node.key() == &key("publish_pull_request"))
      .unwrap();
    let mut wire = serde_json::to_value(&*node).unwrap();
    let published = wire["outcomes"]
      .as_array_mut()
      .unwrap()
      .iter_mut()
      .find(|outcome| outcome["key"] == "published")
      .unwrap();
    published["schema"] = serde_json::json!(legacy_receipt.reference());
    legacy.outcome = serde_json::from_value(published.clone()).unwrap();
    wire["action"]["parameters"]
      .as_object_mut()
      .unwrap()
      .remove("review_valid_until");
    *node = serde_json::from_value(wire).unwrap();
    let terminal = legacy_draft
      .root
      .terminals
      .iter_mut()
      .find(|terminal| terminal.key() == &key("delivered_for_review"))
      .unwrap();
    *terminal = FlowTerminalDefinition::new(terminal.key().clone(), legacy_receipt.reference().clone());
    legacy_draft.data_schemas.push(legacy_receipt);
    let legacy_validation = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(FactoryFlowDefinitionHandlers::new(
        Arc::new(
          PublicationChoices::for_publication(base_choices.with_candidate_publication(legacy).unwrap())
            .with_predecessor(predecessor.clone()),
        ),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    // A historical installation can retain a receipt pinned to one exact changeset.
    legacy_validation
      .handle_query(
        &context,
        ValidateFactoryFlowDefinitionQuery {
          id: flow_id,
          project_id: project,
          draft: legacy_draft,
        },
      )
      .await
      .expect("legacy publication-only targets accept their exact receipt schema");
    let commands = AuthorizedCommandHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(FactoryFlowDefinitionHandlers::new(
        Arc::new(PublicationChoices::for_publication(installed).with_predecessor(predecessor)),
        Arc::new(TrustedNetworkManagementPolicy),
      )),
    );
    let published = commands
      .handle_command(
        &context,
        CreateFactoryFlowDefinitionCommand {
          project_id: project,
          id: flow_id,
          draft: editable,
          idempotency_key: "edited-delivery".parse().unwrap(),
          published_at,
        },
      )
      .await
      .unwrap();
    let saved = &published.publication.flow;
    let saved_root = saved.closure.definition(saved.closure.root()).unwrap();
    assert_eq!(saved_root.name().map(FactoryText::as_str), Some("Project delivery"));
    assert_eq!(saved_root.nodes(), root.nodes());
    assert_eq!(saved_root.transitions(), root.transitions());
    assert_eq!(saved.accepted_predecessor, pinned.accepted_predecessor);
    for published in [&pinned, saved] {
      assert_delivery_entry(published);
    }
  });
}

fn assert_delivery_entry(flow: &FactoryFlowConfiguration) {
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let run = FlowRun::root(&fixture.admission.run, flow.closure.root()).unwrap();
  let cycle = WorkflowCycle::initial(&run).unwrap();
  let admitted = AdmittedFlow::new(
    flow.closure.clone(),
    flow.limits.clone(),
    run.clone(),
    cycle.clone(),
    FactoryWipLimits::new(1, 1).unwrap(),
  )
  .unwrap()
  .with_data_schemas(flow.data_schemas.clone())
  .unwrap();
  let work = &fixture.admission.work;
  let root = flow.closure.definition(flow.closure.root()).unwrap();
  let entry = root.node(root.entry()).unwrap();
  let schema = admitted.data_schema(entry.input_schema().unwrap()).unwrap();
  let predecessor = admitted
    .data_schema(entry.input_binding().unwrap().incoming_schemas().next().unwrap())
    .unwrap();
  let triage = serde_json::json!({
    "assessment":{"priority_assessment":30,"work_kind":"feature_request","severity":"advisory",
      "size":"small","component":"server","risk":"low","dependencies":[],"recommended_route":"requirements",
      "reproducibility":"not_applicable"},
    "admission_priority":20,"admission_risk":"low","accepted":true,"evidence":"absent",
    "conclusion":"A small feature within Project scope.","decision":"eligible","duplicate_coverage":"complete",
  });
  let incoming = FlowIncomingData::new(work, &admitted, FlowPayload::new(predecessor, triage).unwrap()).unwrap();
  let input = FlowNodeInput::prepare(FlowInputPreparation {
    work,
    admitted: &admitted,
    flow: &run,
    cycle: &cycle,
    node: root.entry().clone(),
    schema,
    history: FlowRuntimeHistory {
      flow_runs: std::slice::from_ref(&run),
      cycles: std::slice::from_ref(&cycle),
      attempts: &[],
      completions: &[],
      stops: &[],
    },
    records: &[],
    inputs: &[],
    incoming: Some(&incoming),
  })
  .unwrap();
  assert_eq!(
    input.payload().value(),
    &serde_json::json!({
      "work_kind":"feature_request","reproducibility":"not_applicable","triage_accepted":true
    })
  );
  assert_eq!(
    FlowGateProgram::from_node(entry)
      .unwrap()
      .select_at(input.payload(), Timestamp::from_unix_millis(10).unwrap())
      .unwrap(),
    key("bypass"),
  );
}

struct Prerequisites {
  repository: PublishedRepository,
  flow: PublishedFactoryFlowDefinition,
}
#[async_trait::async_trait]
impl ManagementQueryUseCase<GetRepositoryQuery> for Prerequisites {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetRepositoryQuery,
  ) -> Result<RepositoryProjection, ApplicationError> {
    assert_eq!(
      (query.repository_id, query.version),
      (self.repository.id, self.repository.version)
    );
    Ok(self.repository.clone().into())
  }
}
#[async_trait::async_trait]
impl ManagementQueryUseCase<GetFactoryFlowDefinitionQuery> for Prerequisites {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetFactoryFlowDefinitionQuery,
  ) -> Result<PublishedFactoryFlowDefinition, ApplicationError> {
    let reference = self.flow.flow.closure.root();
    assert_eq!((query.id, query.version), (reference.id(), Some(reference.version())));
    Ok(self.flow.clone())
  }
}

#[test]
fn native_observation_command_is_selectable_with_its_exact_report_and_evidence() {
  run_ready(async {
    use crate::factory::commands::tests::factory_command_tests::command_fixture_with_observation;
    let contract = probe_observation_contract();
    let report = contract.report_schema().unwrap();
    let (_, admitted, mut command) = command_fixture_with_observation(Some(contract));
    let audit_schema = FlowDataSchema::new(key("test.audit"), key("v1"), FlowValueSchema::Boolean).unwrap();
    let audit = EvidenceRequirement::new(
      key("test.audit"),
      EvidenceOutputKind::Report,
      audit_schema.reference().clone(),
      command.executor.clone(),
      command.plugin.clone(),
    );
    command.outputs.push(audit.clone());
    let project = command.build_configuration.project_id();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
      references: [
        ("tool", command.executor.clone()),
        ("plugin", command.plugin.clone()),
        ("command", command.identity.clone()),
      ]
      .into_iter()
      .map(|(alias, reference)| FactoryReferenceChoice {
        alias: key(alias),
        kind: FactoryChoiceKind::NodeCapability,
        reference,
      })
      .collect(),
      build_configurations: vec![(key("check"), command.build_configuration.clone())],
    })
    .unwrap()
    .with_choice_metadata(
      FactoryChoiceKind::NodeCapability,
      key("tool"),
      FactoryChoiceMetadata {
        node_kinds: vec![FlowNodeKind::BuildCommand],
        ..Default::default()
      },
    )
    .unwrap()
    .with_build_pipeline_contract(command.pipeline_contract())
    .unwrap();
    store.seed_project(project).unwrap();
    store.enable_project(project, choices.clone()).unwrap();
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let result = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(FactoryRoleProfileHandlers::with_policy(store, policy)),
    )
    .handle_query(&context, GetFactoryAuthoringCapabilitiesQuery { project_id: project })
    .await
    .unwrap();
    assert_eq!(result.commands.len(), 1, "selectable installed observation");
    let exposed = serde_json::to_value(&result.commands[0]).unwrap();
    assert_eq!(
      exposed["execution"]["task_digest"],
      serde_json::to_value(command.task_digest()).unwrap()
    );
    assert_eq!(exposed["output_definition"], serde_json::to_value(report).unwrap());
    assert_eq!(exposed["evidence"], serde_json::to_value([audit]).unwrap());
    assert!(!exposed.to_string().contains(&command.input_destination));

    let definition = admitted.closure().definition(admitted.closure().root()).unwrap();
    let mut root = serde_json::to_value(definition).unwrap();
    root.as_object_mut().unwrap().remove("reference");
    root["nodes"][0]["build"]["task_digest"] = exposed["execution"]["task_digest"].clone();
    root["nodes"][0]["build"]["evidence"] = exposed["evidence"].clone();
    let mut exact = FactoryFlowDefinitionDraft {
      accepted_predecessor: None,
      root: serde_json::from_value(root.clone()).unwrap(),
      limits: admitted.limits().clone(),
      data_schemas: admitted.data_schemas().to_vec(),
      pools: Default::default(),
    };
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(FactoryFlowDefinitionHandlers::new(
        Arc::new(PublicationChoices::for_publication(choices.clone())),
        policy,
      )),
    );
    let validate = |draft| ValidateFactoryFlowDefinitionQuery {
      id: FlowDefinitionId::generate(),
      project_id: project,
      draft,
    };
    let missing = queries.handle_query(&context, validate(exact.clone())).await;
    assert!(
      matches!(missing, Err(ManagementHandlerError::Application(ApplicationError::FactoryValidation(report))) if report.errors[0].field == "/data_schemas"),
      "a secondary report must have an exact schema before publishing the Flow"
    );
    exact.data_schemas.push(audit_schema);
    queries.handle_query(&context, validate(exact.clone())).await.unwrap();
    let mut swapped = root.clone();
    swapped["nodes"][0]["build"]["result_output"] = serde_json::json!("test.audit");
    swapped["nodes"][0]["build"]["result_output_kind"] = serde_json::json!("report");
    swapped["nodes"][0]["build"]["evidence"] = serde_json::to_value([&command.outputs[0]]).unwrap();
    let outcome = queries
      .handle_query(
        &context,
        validate(FactoryFlowDefinitionDraft {
          root: serde_json::from_value(swapped).unwrap(),
          ..exact.clone()
        }),
      )
      .await;
    assert!(
      matches!(outcome, Err(ManagementHandlerError::Application(ApplicationError::FactoryValidation(report))) if report.errors[0].field == "/build"),
      "the verified observation report must remain the primary result"
    );
    root["nodes"][0]["build"]["evidence"] = serde_json::json!([]);
    let incomplete = FactoryFlowDefinitionDraft {
      root: serde_json::from_value(root).unwrap(),
      ..exact.clone()
    };
    let outcome = queries.handle_query(&context, validate(incomplete)).await;
    assert!(
      matches!(outcome, Err(ManagementHandlerError::Application(ApplicationError::FactoryValidation(report))) if report.errors[0].field == "/build"),
      "a selected native command must keep all installed evidence outputs"
    );

    let mut competing = command.pipeline_contract();
    competing.command.as_mut().unwrap().task_digest = FactoryDigest::from_bytes([91; 32]);
    let both_commands = choices.clone().with_build_pipeline_contract(competing).unwrap();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, both_commands.clone()).unwrap();
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let options = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(FactoryRoleProfileHandlers::with_policy(store, policy.clone())),
    )
    .handle_query(&context, GetFactoryAuthoringCapabilitiesQuery { project_id: project })
    .await
    .unwrap();
    let outcome = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(FactoryFlowDefinitionHandlers::new(
        Arc::new(PublicationChoices::for_publication(both_commands)),
        policy,
      )),
    )
    .handle_query(&context, validate(exact.clone()))
    .await;
    assert_eq!(
      (options.commands.len(), outcome.is_ok()),
      (2, true),
      "both installed commands must remain selectable and the exact task digest must validate"
    );

    let mut role_only = command.pipeline_contract();
    role_only.command = None;
    role_only.compilation = Some(FactoryBuildCompilationContract {
      evidence: vec![],
      task_digest: command.task_digest(),
      role_profiles: true,
      requires_task_context: false,
      result_output: command.outputs[0].kind().clone(),
      result_output_kind: EvidenceOutputKind::Report,
      mcp_result_tool: None,
    });
    let role_only = FactoryConfigurationChoices::try_new(choices.entries())
      .unwrap()
      .with_build_pipeline_contract(role_only)
      .unwrap();
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let outcome = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(FactoryFlowDefinitionHandlers::new(
        Arc::new(PublicationChoices::for_publication(role_only)),
        policy,
      )),
    )
    .handle_query(&context, validate(exact))
    .await;
    assert!(
      matches!(outcome, Err(ManagementHandlerError::Application(ApplicationError::FactoryValidation(report))) if report.errors[0].field == "/build"),
      "a role-only Build must not be accepted as a command"
    );
  });
}
