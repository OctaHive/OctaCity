use super::*;
pub(crate) use crate::factory_node_test_support::publication::PublicationChoices;
use octacity_server_factory::*;
use octacity_server_store::*;
use std::collections::BTreeMap;

mod issue_snapshot;
mod ticket_delivery;

fn selected_fixture(project: ProjectId) -> Fixture {
  selected_fixture_with_profiles(project, |_, _| {})
}
fn selected_fixture_with_profiles(
  project: ProjectId,
  mut edit: impl FnMut(&str, &mut FactoryRoleProfileDefinition),
) -> Fixture {
  let mut fixture = fixture(project);
  let prior = fixture.draft.flow.as_ref().unwrap();
  let root = prior.closure.definition(prior.closure.root()).unwrap();
  let original = root.node(root.entry()).unwrap();
  let base_build = FlowBuildBinding {
    execution_permissions: None,
    tool: original.action().unwrap().plugin().clone(),
    result_tool: None,
    plugin: original.action().unwrap().plugin().clone(),
    model_or_tool: original.action().unwrap().plugin().clone(),
    task_digest: digest(9),
    build_configuration: fixture.choices.entries().build_configurations[0].1.clone(),
    capture_write_paths: None,
    capture_deliverables: None,
    result_output: key("observations"),
    result_output_kind: EvidenceOutputKind::Report,
    result_outcome: key("succeeded"),
    evidence: vec![],
  };
  let role = FactoryRole::publish(
    FactoryRoleId::generate(),
    project,
    FactoryRoleDefinition {
      avatar: None,
      skills: Default::default(),
      name: FactoryText::new("Stage owner").unwrap(),
      instructions: "Private stage instructions".into(),
      template: None,
      icon: None,
      description: String::new(),
      concurrency_limit: None,
    },
  )
  .unwrap();
  let verifier = exact("selection.verifier", "v1", 72);
  let mut entries = fixture.choices.entries();
  entries.references.push(FactoryReferenceChoice {
    kind: FactoryChoiceKind::NodeCapability,
    alias: key("selection.verifier"),
    reference: verifier.clone(),
  });
  let mut candidates = BTreeMap::new();
  let mut caps = Vec::new();
  for (name, byte) in [("default", 73), ("pro", 74), ("specialized", 75)] {
    let mut build = base_build.clone();
    build.result_tool = Some(verifier.clone());
    build.model_or_tool = exact(name, "v1", byte);
    let mut definition = FactoryRoleProfileDefinition {
      execution_binding: Default::default(),
      model_identifier: None,
      thinking_parameter: None,
      name: FactoryText::new(name).unwrap(),
      instructions: "Private profile instructions".into(),
      template: None,
      toolchain: FactoryTaskToolchain {
        plugin: build.plugin.clone(),
        executable: build.tool.clone(),
        model: build.model_or_tool.clone(),
      },
      execution: Default::default(),
      model_parameters: BTreeMap::new(),
      skills: BTreeMap::new(),
      context_requirements: Default::default(),
      build_configuration: build.build_configuration.clone(),
      credential_purpose: FactoryCredentialPurpose::Model,
      permissions: FactoryPermissionSet::deny_all(),
      budget: original.budget(),
    };
    edit(name, &mut definition);
    entries.references.extend([
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: key(name),
        reference: build.tool.clone(),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: key(&format!("model.{name}")),
        reference: build.model_or_tool.clone(),
      },
    ]);
    caps.push((
      key(name),
      FactoryRoleProfileCapability {
        mcp_required_environment: Default::default(),
        model_catalog: None,
        ceiling: definition.clone(),
        parameters: FlowDataSchema::new(
          key("parameters"),
          key("v1"),
          FlowValueSchema::Object {
            fields: BTreeMap::new(),
          },
        )
        .unwrap(),
        public_environment: BTreeMap::new(),
        local_permissions: LocalPermissionCeiling::fully_enforced(definition.permissions.clone()),
      },
    ));
    candidates.insert(
      key(name),
      FlowProfileCandidate {
        profile: FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, definition).unwrap(),
        build,
      },
    );
  }
  let mut choices = FactoryConfigurationChoices::try_new(entries).unwrap();
  for (alias, cap) in caps {
    choices = choices.with_role_profile_capability(alias, cap).unwrap();
  }
  let mut wire = serde_json::to_value(original).unwrap();
  wire.as_object_mut().unwrap().remove("action");
  wire["kind"] = serde_json::json!("reasoning");
  let selected = serde_json::from_value::<FlowNodeDefinition>(wire)
    .unwrap()
    .with_profile_selection(FlowProfileSelectionBinding {
      role: role.reference().clone(),
      candidates,
      policy: FlowProfileSelectionPolicy::Manual { candidate: None },
      failure: FlowProfileSelectionFailure::ManualSelection,
      selection_budget: BudgetUsage::default(),
    })
    .unwrap();
  let nodes = root
    .nodes()
    .iter()
    .map(|node| {
      if node.key() == selected.key() {
        selected.clone()
      } else {
        node.clone()
      }
    })
    .collect();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: root.reference().id(),
    version: root.reference().version(),
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
  fixture.draft.flow = Some(
    FactoryFlowConfiguration::new(
      PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
      prior.limits.clone(),
      prior.data_schemas.clone(),
      prior.pools.clone(),
    )
    .unwrap(),
  );
  fixture.choices = choices;
  fixture
}

#[test]
fn selection_validation_and_diagnostics_include_every_candidate_build_contract() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = selected_fixture(project);
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices.clone()).unwrap();
    let handlers = FactoryConfigurationHandlers::new(store.clone());
    let query_input = ValidateFactoryConfigurationQuery {
      id: FactoryConfigurationId::generate(),
      project_id: project,
      definition_digest: digest(20),
      draft: fixture.draft,
    };
    let validation = query(&handlers, query_input.clone()).await.unwrap();
    let models: Vec<_> = validation
      .diagnostics
      .required_agent_capabilities
      .iter()
      .map(|reference| reference.identity().as_str())
      .collect();
    assert!(
      models.contains(&"default") && models.contains(&"pro") && models.contains(&"specialized"),
      "candidate models absent from diagnostics: {models:?}"
    );
    let mut deleted_choices = fixture.choices.clone();
    let role = query_input
      .draft
      .flow
      .as_ref()
      .unwrap()
      .closure
      .definitions()
      .iter()
      .flat_map(|definition| definition.nodes())
      .find_map(|node| node.profile_selection())
      .unwrap()
      .role
      .id;
    deleted_choices.exclude_role(role);
    store.enable_project(project, deleted_choices).unwrap();
    assert!(
      query(&handlers, query_input.clone()).await.is_err(),
      "authorization filtering must preserve deleted-role exclusions"
    );
    let mut entries = fixture.choices.entries();
    entries
      .references
      .retain(|entry| entry.alias != key("selection.verifier"));
    let mut choices = FactoryConfigurationChoices::try_new(entries).unwrap();
    for (alias, cap) in fixture.choices.role_profile_capabilities() {
      choices = choices
        .with_role_profile_capability(alias.clone(), cap.clone())
        .unwrap();
    }
    store.enable_project(project, choices).unwrap();
    let error = query(&handlers, query_input)
      .await
      .expect_err("candidate verifiers must be installed and visible");
    assert_eq!(error.classification(), ApplicationFailure::Invalid);
  });
}

#[test]
fn constraints_explain_each_exact_profile_allowance_instead_of_claiming_the_node_reservation() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = selected_fixture_with_profiles(project, |name, definition| {
      if name == "default" {
        definition.budget = BudgetLimit::new(1, 10_000, 17, 10_000, 10_000).unwrap();
      }
    });
    let flow = fixture.draft.flow.as_ref().unwrap();
    let selected_node = flow
      .closure
      .definitions()
      .iter()
      .flat_map(|d| d.nodes())
      .find(|n| n.profile_selection().is_some())
      .unwrap();
    let profile = selected_node.profile_selection().unwrap().candidates[&key("default")]
      .profile
      .reference()
      .clone();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let validation = query(
      &FactoryConfigurationHandlers::new(store),
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id: project,
        definition_digest: digest(20),
        draft: fixture.draft,
      },
    )
    .await
    .unwrap();
    let diagnostics = serde_json::to_value(validation.diagnostics).unwrap();
    let budgets = diagnostics["constraints"]["node_budgets"].as_array().unwrap();
    let selected = budgets
      .iter()
      .find(|budget| budget["profile"] == serde_json::json!(profile))
      .expect("each exact candidate must retain its own effective allowance");
    assert_eq!(selected["requested"]["max_tokens"], 1000);
    assert_eq!(selected["effective"]["max_tokens"], 17);
    assert_eq!(selected["sources"][0]["source"], "role_profile");
    assert_eq!(selected["sources"][0]["scope"], "node_execution");
  });
}

#[test]
fn standalone_flow_publication_uses_the_same_profile_compatibility_validation() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = selected_fixture(project);
    let flow = fixture.draft.flow.unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    let draft = crate::FactoryFlowDefinitionDraft {
      accepted_predecessor: None,
      issue_snapshot: None,
      root: crate::FactoryFlowRootDraft {
        name: None,
        code_sources: None,
        process: root.process().cloned(),
        input_schema: root.input_schema().cloned(),
        entry: root.entry().clone(),
        nodes: root.nodes().to_vec(),
        transitions: root.transitions().to_vec(),
        terminals: root.terminals().to_vec(),
        context_projections: root.context_projections().to_vec(),
        data_projections: root.data_projections().to_vec(),
        execution: root.execution().clone(),
      },
      limits: flow.limits.clone(),
      data_schemas: flow.data_schemas.clone(),
      pools: flow.pools.clone(),
    };
    let mut choices = FactoryConfigurationChoices::try_new(fixture.choices.entries()).unwrap();
    for (alias, cap) in fixture.choices.role_profile_capabilities() {
      if alias != &key("specialized") {
        choices = choices
          .with_role_profile_capability(alias.clone(), cap.clone())
          .unwrap();
      }
    }
    let store = Arc::new(PublicationChoices::for_validation(choices));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(crate::FactoryFlowDefinitionHandlers::new(store.clone(), policy)),
    );
    let input = crate::ValidateFactoryFlowDefinitionQuery {
      id: FlowDefinitionId::generate(),
      project_id: project,
      draft,
    };
    assert!(
      handlers.handle_query(&context(), input.clone()).await.is_err(),
      "standalone Flow publication must reject an unavailable candidate too"
    );
    store.replace_choices(fixture.choices);
    assert!(
      handlers
        .handle_query(
          &context(),
          crate::ValidateFactoryFlowDefinitionQuery {
            project_id: ProjectId::generate(),
            ..input.clone()
          },
        )
        .await
        .is_err(),
      "installed capability choices do not make another Project's Role and Profiles selectable"
    );
    assert!(handlers.handle_query(&context(), input.clone()).await.is_ok());
    let mut input = input;
    let node = input
      .draft
      .root
      .nodes
      .iter_mut()
      .find(|node| node.kind() == octacity_server_factory::FlowNodeKind::Reasoning)
      .unwrap();
    *node = node
      .clone()
      .with_protected_verification(
        octacity_server_factory::FactoryArtifactReference::new(
          octacity_server_domain::ArtifactId::generate(),
          octacity_server_factory::FactoryDigest::content_sha256(b"unknown"),
          7,
        )
        .unwrap(),
      )
      .unwrap();
    assert!(
      handlers.handle_query(&context(), input).await.is_err(),
      "a caller cannot attach arbitrary bytes to the protected verifier"
    );
  });
}

#[test]
fn standalone_flow_requires_compatible_verification_and_current_retention() {
  #[derive(Clone)]
  struct Input(PublishedProjectInput);
  #[async_trait::async_trait]
  impl ProjectInputQueryStore for Input {
    async fn project_input_artifact(
      &self,
      project: ProjectId,
      artifact: octacity_server_domain::ArtifactId,
    ) -> Result<Option<PublishedProjectInput>, StoreError> {
      Ok((self.0.project_id == project && self.0.object.artifact.artifact_id() == artifact).then(|| self.0.clone()))
    }
  }
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = selected_fixture(project);
    let flow = fixture.draft.flow.unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    let reference =
      FactoryArtifactReference::new(octacity_server_domain::ArtifactId::generate(), digest(27), 100).unwrap();
    let mut wire = serde_json::to_value(root).unwrap();
    wire.as_object_mut().unwrap().remove("reference");
    let mut draft: crate::FactoryFlowRootDraft = serde_json::from_value(wire).unwrap();
    let step = draft
      .nodes
      .iter_mut()
      .find(|node| node.kind() == FlowNodeKind::Reasoning)
      .unwrap();
    let mut selection = step.profile_selection().unwrap().clone();
    let schema = step.outcomes()[0].schema().clone();
    for candidate in selection.candidates.values_mut() {
      let build = &mut candidate.build;
      build.result_output = key("reviewed_check_summary");
      build.evidence = vec![EvidenceRequirement::new(
        key("frozen_check_files"),
        EvidenceOutputKind::Artifact,
        schema.clone(),
        build.tool.clone(),
        build.plugin.clone(),
      )];
    }
    let mut choices = fixture.choices;
    for candidate in selection.candidates.values() {
      let build = &candidate.build;
      choices = choices
        .with_build_pipeline_contract(FactoryBuildPipelineContract {
          build_configuration: build.build_configuration.clone(),
          tool: build.tool.clone(),
          plugin: build.plugin.clone(),
          model_or_tool: build.model_or_tool.clone(),
          pipeline_node: "job".parse().unwrap(),
          compilation: None,
          command: None,
          protected_tests: Some(FactoryProtectedTestBuildContract::Author {
            result_output: build.result_output.clone(),
            inventory: build.evidence[0].clone(),
          }),
        })
        .unwrap();
    }
    let author = &selection.candidates[&key("default")].build;
    let mut verifier = author.clone();
    verifier.model_or_tool = verifier.tool.clone();
    verifier.result_output = key("check-result");
    verifier.evidence.clear();
    verifier.result_tool = None;
    let verifier_contract = FactoryBuildPipelineContract {
      build_configuration: verifier.build_configuration.clone(),
      tool: verifier.tool.clone(),
      plugin: verifier.plugin.clone(),
      model_or_tool: verifier.model_or_tool.clone(),
      pipeline_node: "job".parse().unwrap(),
      compilation: None,
      protected_tests: None,
      command: Some(FactoryBuildCommandContract {
        task_digest: verifier.task_digest,
        outputs: vec![EvidenceRequirement::new(
          verifier.result_output.clone(),
          EvidenceOutputKind::Report,
          schema,
          verifier.tool.clone(),
          verifier.plugin.clone(),
        )],
        observation_schema: None,
        permissions: FactoryPermissionSet::deny_all(),
      }),
    };
    let author_profile = author.model_or_tool.clone();
    let inventory = author.evidence[0].clone();
    let author_key = step.key().clone();
    let plain_author_key = key("plain-author");
    let mut plain_author =
      serde_json::to_value(step.clone().with_profile_selection(selection.clone()).unwrap()).unwrap();
    plain_author["key"] = serde_json::json!(plain_author_key);
    *step = step
      .clone()
      .with_profile_selection(selection)
      .unwrap()
      .with_protected_verification(reference.clone())
      .unwrap();
    draft.nodes.push(serde_json::from_value(plain_author).unwrap());
    let edge = draft
      .transitions
      .iter_mut()
      .find(|edge| edge.predecessor() == &author_key && edge.outcome() == &key("succeeded"))
      .unwrap();
    *edge = FlowTransition::new(
      author_key.clone(),
      key("succeeded"),
      FlowTransitionTarget::Node(plain_author_key.clone()),
    );
    for outcome in ["succeeded", "retryable", "failed", "cancelled"] {
      let target = if outcome == "succeeded" {
        FlowTransitionTarget::Node(key("validate"))
      } else {
        FlowTransitionTarget::Terminal(key(if outcome == "cancelled" { "cancelled" } else { "failed" }))
      };
      draft
        .transitions
        .push(FlowTransition::new(plain_author_key.clone(), key(outcome), target));
    }
    let artifact_schema = FlowDataSchema::new(
      key("test.artifact"),
      key("v1"),
      serde_json::from_value(serde_json::json!({
        "type": "object", "fields": {
          "artifact_id": {"required": true, "value": {"type": "string", "min_bytes": 1, "max_bytes": 256}},
          "content_digest": {"required": true, "value": {"type": "string", "min_bytes": 64, "max_bytes": 64}},
          "encoded_size": {"required": true, "value": {"type": "integer", "minimum": 1, "maximum": 16777216}}
        }
      }))
      .unwrap(),
    )
    .unwrap();
    let verifier_input = FlowDataSchema::new(
      key("test.verifier-input"),
      key("v1"),
      FlowValueSchema::Object {
        fields: BTreeMap::from([
          (key("suite"), FlowFieldSchema::required(artifact_schema.shape().clone())),
          (
            key("alternate"),
            FlowFieldSchema::required(artifact_schema.shape().clone()),
          ),
        ]),
      },
    )
    .unwrap();
    let verifier_step = draft
      .nodes
      .iter_mut()
      .find(|node| node.key() == &key("validate"))
      .unwrap();
    let mut wire = serde_json::to_value(&*verifier_step).unwrap();
    wire.as_object_mut().unwrap().remove("action");
    wire["kind"] = serde_json::json!("build_command");
    wire["input_schema"] = serde_json::json!(verifier_input.reference());
    let artifact_selection = |path: &str| {
      serde_json::json!({
        "source_kind": "evidence", "source": {"value": {"mapping": "object", "fields": {
          "kind": {"mapping": "constant", "value": "artifact"},
          "value": {"mapping": "pointer", "path": path}
        }}}
      })
    };
    wire["context_binding"] = serde_json::json!({
      "entries": {"selected-suite": artifact_selection("/suite")}
    });
    *verifier_step = serde_json::from_value::<FlowNodeDefinition>(wire)
      .unwrap()
      .with_build(verifier)
      .unwrap();
    let mut request = crate::ValidateFactoryFlowDefinitionQuery {
      id: FlowDefinitionId::generate(),
      project_id: project,
      draft: crate::FactoryFlowDefinitionDraft {
        accepted_predecessor: None,
        issue_snapshot: None,
        root: draft,
        limits: flow.limits.clone(),
        data_schemas: flow.data_schemas,
        pools: flow.pools,
      },
    };
    request.draft.data_schemas.push(verifier_input);
    let now = crate::FactoryExecutionClock::now(&crate::SystemFactoryExecutionClock).unwrap();
    let expired = octacity_server_domain::Timestamp::from_unix_millis(1).unwrap();
    let current = octacity_server_domain::Timestamp::from_unix_millis(now.unix_millis() + 60_000).unwrap();
    for (expires_at, accepts_supplement, producer, alternate_guard, should_validate) in [
      (current, true, &author_key, "/with_additional", true),
      (current, false, &author_key, "/with_additional", false),
      (current, false, &plain_author_key, "/with_additional", true),
      (current, false, &plain_author_key, "/suite", false),
      (current, false, &plain_author_key, "/suite/artifact_id", false),
      (expired, true, &author_key, "/with_additional", false),
    ] {
      let installed = choices
        .clone()
        .with_build_pipeline_contract(FactoryBuildPipelineContract {
          protected_tests: Some(FactoryProtectedTestBuildContract::Verifier {
            author_profile: author_profile.clone(),
            inventory: inventory.clone(),
            accepts_supplement,
          }),
          ..verifier_contract.clone()
        })
        .unwrap();
      let store = Arc::new(PublicationChoices::for_validation(installed));
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let metadata = Input(PublishedProjectInput {
        project_id: project,
        preparation_id: digest(27),
        mapping: key(PROJECT_PROTECTED_VERIFICATION_MAPPING),
        object: FactoryInputObject {
          artifact: reference.clone(),
          role: FactoryInputRole::Document,
          upload_id: octacity_server_domain::ArtifactUploadId::generate(),
          media_type: ArtifactMediaType::new("application/json").unwrap(),
        },
        published_at: expired,
        expires_at,
      });
      let handlers = AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(crate::FactoryFlowDefinitionHandlers::new(store, policy).with_verification_inputs(Arc::new(metadata))),
      );
      let verifier = request
        .draft
        .root
        .nodes
        .iter_mut()
        .find(|node| node.key() == &key("validate"))
        .unwrap();
      let inventory_from = |node: &FactoryKey| FlowInputSource::Record {
        node: node.clone(),
        outcome: key("succeeded"),
        view: FlowRecordView::Metadata,
        path: "/build/outputs/frozen_check_files/artifact".into(),
      };
      let mut guarded = artifact_selection("/alternate");
      guarded["when_present"] = serde_json::json!(alternate_guard);
      let mut wire = serde_json::to_value(&*verifier).unwrap();
      wire["context_binding"]["entries"]["alternate-suite"] = guarded;
      *verifier = serde_json::from_value::<FlowNodeDefinition>(wire)
        .unwrap()
        .with_input_binding(
          FlowInputBinding::new(
            BTreeMap::from([
              (key("inventory"), inventory_from(producer)),
              (key("extra"), inventory_from(&author_key)),
            ]),
            FlowDataMapping::new(FlowValueMapping::Object {
              fields: BTreeMap::from([
                (
                  key("suite"),
                  FlowValueMapping::Pointer {
                    path: "/inventory".into(),
                  },
                ),
                (key("alternate"), FlowValueMapping::Pointer { path: "/extra".into() }),
              ]),
            })
            .unwrap(),
          )
          .unwrap(),
        )
        .unwrap();
      let result = handlers.handle_query(&context(), request.clone()).await;
      assert_eq!(
        result.is_ok(),
        should_validate,
        "producer {producer}, guard {alternate_guard}, supplement support {accepts_supplement}, retention {expires_at:?}, error {:?}",
        result.err()
      );
    }
  });
}

#[test]
fn readiness_bounds_verification_reads_and_distinguishes_store_failure() {
  use std::sync::atomic::{AtomicUsize, Ordering};
  struct VerificationInputs {
    reads: AtomicUsize,
    fail: bool,
    input: PublishedProjectInput,
  }
  #[async_trait::async_trait]
  impl ProjectInputQueryStore for VerificationInputs {
    async fn project_input_artifact(
      &self,
      project: ProjectId,
      artifact: octacity_server_domain::ArtifactId,
    ) -> Result<Option<PublishedProjectInput>, StoreError> {
      self.reads.fetch_add(1, Ordering::SeqCst);
      assert_eq!(project, self.input.project_id);
      assert_eq!(artifact, self.input.object.artifact.artifact_id());
      if self.fail {
        return Err(StoreError::Unavailable);
      }
      Ok(Some(self.input.clone()))
    }
  }
  run_ready(async {
    use crate::{FactoryReadinessState::*, *};
    let project = ProjectId::generate();
    let reference =
      FactoryArtifactReference::new(octacity_server_domain::ArtifactId::generate(), digest(27), 100).unwrap();
    for (count, fail, expected, expected_reads) in
      [(2, true, Unknown, 1), (2, false, Ready, 1), (64, false, Unknown, 0)]
    {
      let mut fixture = selected_fixture(project);
      let flow = fixture.draft.flow.as_mut().unwrap();
      let root = flow.closure.definition(flow.closure.root()).unwrap();
      let template = serde_json::to_value(
        root
          .node(root.entry())
          .unwrap()
          .clone()
          .with_protected_verification(reference.clone())
          .unwrap(),
      )
      .unwrap();
      let name = |index| key(&format!("author-{index}"));
      let mut nodes = Vec::new();
      let mut transitions = Vec::new();
      for index in 0..count {
        let mut wire = template.clone();
        wire["key"] = serde_json::json!(name(index));
        let node: FlowNodeDefinition = serde_json::from_value(wire).unwrap();
        for outcome in node.outcomes() {
          let target = if index + 1 < count && outcome.key() == &key("succeeded") {
            FlowTransitionTarget::Node(name(index + 1))
          } else {
            FlowTransitionTarget::Terminal(key(match outcome.key().as_str() {
              "succeeded" => "succeeded",
              "cancelled" => "cancelled",
              _ => "failed",
            }))
          };
          transitions.push(FlowTransition::new(name(index), outcome.key().clone(), target));
        }
        nodes.push(node);
      }
      let definition = FlowDefinition::new(FlowDefinitionInput {
        id: root.reference().id(),
        version: root.reference().version(),
        entry: name(0),
        input_schema: root.input_schema().cloned(),
        nodes,
        transitions,
        terminals: root.terminals().to_vec(),
        context_projections: vec![],
        data_projections: vec![],
        execution: root.execution().clone(),
      })
      .unwrap();
      flow.closure = PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap();
      let id = FactoryConfigurationId::generate();
      let version = FactoryConfigurationVersion::INITIAL;
      let configuration =
        FactoryConfiguration::publish(id, version, project, digest(28), fixture.draft, &fixture.choices).unwrap();
      let store = Arc::new(InMemoryFactoryConfigurationStore::new());
      store.seed_project(project).unwrap();
      store
        .seed_factory_configuration_version(PublishedFactoryConfiguration {
          configuration,
          published_at: time(0),
        })
        .unwrap();
      let inputs = Arc::new(VerificationInputs {
        fail,
        reads: AtomicUsize::new(0),
        input: PublishedProjectInput {
          project_id: project,
          preparation_id: digest(27),
          mapping: key(PROJECT_PROTECTED_VERIFICATION_MAPPING),
          object: FactoryInputObject {
            artifact: reference.clone(),
            role: FactoryInputRole::Document,
            upload_id: octacity_server_domain::ArtifactUploadId::generate(),
            media_type: ArtifactMediaType::new("application/json").unwrap(),
          },
          published_at: time(0),
          expires_at: time(60_000),
        },
      });
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let handler = AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(
          FactoryReadinessHandlers::new(
            store,
            policy,
            crate::factory_node_test_support::clock(),
            std::time::Duration::from_secs(30),
          )
          .unwrap()
          .with_verification_inputs(inputs.clone()),
        ),
      );
      let report = handler
        .handle_query(
          &context(),
          GetFactoryReadinessQuery {
            id,
            version,
            selection: FactoryReadinessSelection {
              repository: WorkSourceRepositoryRef {
                project_id: project,
                id: octacity_server_domain::RepositoryId::generate(),
                version: octacity_server_domain::RepositoryVersion::INITIAL,
                definition_digest: digest(29),
              },
              input: None,
            },
          },
        )
        .await
        .unwrap();
      let aspect = if expected_reads == 0 {
        "coverage"
      } else {
        "protected_verification"
      };
      assert_eq!(
        report.checks.iter().find(|check| check.aspect == aspect).unwrap().state,
        expected
      );
      assert_eq!(inputs.reads.load(Ordering::SeqCst), expected_reads);
    }
  });
}

#[test]
fn access_constraints_explain_profile_narrowing_and_defer_actual_agent_enforcement() {
  run_ready(async {
    let project = ProjectId::generate();
    let mut fixture = selected_fixture_with_profiles(project, |name, definition| {
      if name == "default" {
        definition.budget = BudgetLimit::new(1, 1000, 1000, 10_000, 10_000).unwrap();
      }
    });
    let old = fixture.draft.flow.take().unwrap();
    let root = old.closure.definition(old.closure.root()).unwrap();
    let selected = root
      .nodes()
      .iter()
      .find(|node| node.profile_selection().is_some())
      .unwrap();
    let profile = selected.profile_selection().unwrap().candidates[&key("default")]
      .profile
      .reference()
      .clone();
    let selected_key = selected.key().clone();
    let mut permissions = serde_json::to_value(FactoryPermissionSet::deny_all()).unwrap();
    permissions["network_hosts"] = serde_json::json!(["api.allowed.example"]);
    permissions["resources"]["elapsed_millis"] = serde_json::json!(10_000);
    let mut root_wire = serde_json::to_value(root).unwrap();
    root_wire.as_object_mut().unwrap().remove("reference");
    root_wire["execution"]["permissions"] = permissions.clone();
    for node in root_wire["nodes"].as_array_mut().unwrap() {
      node["permissions"] = permissions.clone();
    }
    let root_draft: crate::FactoryFlowRootDraft = serde_json::from_value(root_wire).unwrap();
    let definition = FlowDefinition::new(FlowDefinitionInput {
      id: root.reference().id(),
      version: root.reference().version(),
      input_schema: root_draft.input_schema,
      entry: root_draft.entry,
      nodes: root_draft.nodes,
      transitions: root_draft.transitions,
      terminals: root_draft.terminals,
      context_projections: root_draft.context_projections,
      data_projections: root_draft.data_projections,
      execution: root_draft.execution,
    })
    .unwrap();
    let mut limits = serde_json::to_value(old.limits).unwrap();
    limits["permissions"] = permissions;
    fixture.draft.flow = Some(
      FactoryFlowConfiguration::new(
        PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
        serde_json::from_value(limits).unwrap(),
        old.data_schemas,
        old.pools,
      )
      .unwrap(),
    );
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let validation = query(
      &FactoryConfigurationHandlers::new(store),
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id: project,
        definition_digest: digest(20),
        draft: fixture.draft,
      },
    )
    .await
    .unwrap();
    let diagnostics = serde_json::to_value(validation.diagnostics).unwrap();
    let node = diagnostics["effective_permissions"]
      .as_array()
      .unwrap()
      .iter()
      .find(|row| row["node"] == serde_json::json!(selected_key))
      .unwrap();
    assert_eq!(
      node["requested"]["network_hosts"],
      serde_json::json!(["api.allowed.example"])
    );
    assert!(
      node["sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source == "flow_admission")
    );
    let selected = node["profiles"]
      .as_array()
      .expect("profile ceilings remain explicit")
      .iter()
      .find(|row| row["profile"] == serde_json::json!(profile))
      .unwrap();
    assert_eq!(selected["permissions"]["network_hosts"], serde_json::json!([]));
    assert_eq!(selected["permissions"]["resources"]["elapsed_millis"], 0);
    assert!(
      selected["sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source == "profile_budget")
    );
    assert!(
      node["unresolved_sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source == "agent_local")
    );
    assert!(
      node["unresolved_sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source == "backend_placement")
    );
    assert!(!diagnostics.to_string().contains("Private profile instructions"));
    for source in ["project_policy", "installed_runtime_policy"] {
      assert!(
        node["unresolved_sources"]
          .as_array()
          .unwrap()
          .iter()
          .any(|value| value == source),
        "dispatch policy {source} cannot be claimed resolved by definition validation"
      );
    }
  });
}
#[test]
fn decision_profile_answers_are_checked_before_flow_publication() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = selected_fixture(project);
    let flow = fixture.draft.flow.unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    let selector = exact("installed-selector", "v1", 91);
    let mut entries = fixture.choices.entries();
    entries.references.push(FactoryReferenceChoice {
      kind: FactoryChoiceKind::DecisionSignalPolicy,
      alias: key("selector"),
      reference: selector.clone(),
    });
    let mut choices = FactoryConfigurationChoices::try_new(entries).unwrap();
    for (alias, capability) in fixture.choices.role_profile_capabilities() {
      choices = choices
        .with_role_profile_capability(alias.clone(), capability.clone())
        .unwrap();
    }
    choices = choices
      .with_choice_metadata(
        FactoryChoiceKind::DecisionSignalPolicy,
        key("selector"),
        FactoryChoiceMetadata {
          profile_selection: Some(FactoryProfileSelectorMetadata {
            provider: exact("provider", "v1", 92),
            adapter: exact("adapter", "v1", 93),
            model: exact("model", "v1", 94),
            question_set: exact("criteria", "v1", 95),
            calibration: exact("calibration", "v1", 96),
            probability_semantics: exact("semantics", "v1", 97),
            mode: DecisionSignalMode::Shadow,
            baseline: key("default"),
            candidates: DecisionSignalChoices::try_new(vec![key("default"), key("pro"), key("specialized")]).unwrap(),
            minimum_probability: DecisionSignalProbability::new(800_000).unwrap(),
            minimum_margin: DecisionSignalProbability::new(100_000).unwrap(),
          }),
          ..Default::default()
        },
      )
      .unwrap();
    let store = Arc::new(PublicationChoices::for_validation(choices));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(crate::FactoryFlowDefinitionHandlers::new(store, policy)),
    );
    let draft = crate::FactoryFlowDefinitionDraft {
      accepted_predecessor: None,
      issue_snapshot: None,
      root: crate::FactoryFlowRootDraft {
        name: None,
        code_sources: None,
        process: root.process().cloned(),
        input_schema: root.input_schema().cloned(),
        entry: root.entry().clone(),
        nodes: root.nodes().to_vec(),
        transitions: root.transitions().to_vec(),
        terminals: root.terminals().to_vec(),
        context_projections: root.context_projections().to_vec(),
        data_projections: root.data_projections().to_vec(),
        execution: root.execution().clone(),
      },
      limits: flow.limits.clone(),
      data_schemas: flow.data_schemas.clone(),
      pools: flow.pools.clone(),
    };
    let mut input = crate::ValidateFactoryFlowDefinitionQuery {
      id: FlowDefinitionId::generate(),
      project_id: project,
      draft,
    };
    let at = input
      .draft
      .root
      .nodes
      .iter()
      .position(|node| node.profile_selection().is_some())
      .unwrap();
    let node = input.draft.root.nodes[at].clone();
    let mut selection = node.profile_selection().unwrap().clone();
    selection.policy = FlowProfileSelectionPolicy::DecisionApi {
      policy: selector,
      input: FlowDataMapping::new(FlowValueMapping::Object {
        fields: BTreeMap::new(),
      })
      .unwrap(),
    };
    input.draft.root.nodes[at] = node.clone().with_profile_selection(selection.clone()).unwrap();
    assert!(handlers.handle_query(&context(), input.clone()).await.is_ok());
    let candidate = selection.candidates.remove(&key("default")).unwrap();
    selection
      .candidates
      .insert(key("62000000-0000-4000-8000-000000000001"), candidate);
    input.draft.root.nodes[at] = node.clone().with_profile_selection(selection.clone()).unwrap();
    assert!(
      handlers.handle_query(&context(), input.clone()).await.is_err(),
      "a profile UUID is not the installed selector answer default"
    );
    selection.policy = FlowProfileSelectionPolicy::Manual { candidate: None };
    input.draft.root.nodes[at] = node.with_profile_selection(selection).unwrap();
    assert!(
      handlers.handle_query(&context(), input).await.is_ok(),
      "manual selection keeps arbitrary candidate keys and needs no provider"
    );
  });
}

#[test]
fn native_compiler_rejects_missing_task_context_before_publication_and_accepts_explicit_work_task() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = selected_fixture(project);
    let mut choices = fixture.choices;
    let mut draft = fixture.draft;
    let flow = draft.flow.as_ref().unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    let node = root
      .nodes()
      .iter()
      .find(|node| node.profile_selection().is_some())
      .unwrap();
    for binding in node.build_bindings() {
      choices = choices
        .with_build_pipeline_contract(FactoryBuildPipelineContract {
          protected_tests: None,
          command: None,
          build_configuration: binding.build_configuration.clone(),
          tool: binding.tool.clone(),
          plugin: binding.plugin.clone(),
          model_or_tool: binding.model_or_tool.clone(),
          pipeline_node: octacity_server_domain::PipelineNodeId::new("native").unwrap(),
          compilation: Some(FactoryBuildCompilationContract {
            permissions: None,
            capture_write_paths: None,
            scoped_validation: false,
            evidence: vec![],
            task_digest: binding.task_digest,
            role_profiles: true,
            requires_task_context: true,
            result_output: binding.result_output.clone(),
            result_output_kind: binding.result_output_kind,
            mcp_result_tool: None,
          }),
        })
        .unwrap();
    }
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, choices).unwrap();
    let handlers = FactoryConfigurationHandlers::new(store);
    let request = ValidateFactoryConfigurationQuery {
      id: FactoryConfigurationId::generate(),
      project_id: project,
      definition_digest: digest(20),
      draft: draft.clone(),
    };
    assert!(
      query(&handlers, request.clone()).await.is_err(),
      "a native step without Task Context cannot be published"
    );
    let artifact =
      FactoryArtifactReference::new(octacity_server_domain::ArtifactId::generate(), digest(31), 42).unwrap();
    for (binding, accepted) in [
      (
        serde_json::json!({"entries":{"request":{"source_kind":"task","source":{"value":{"mapping":"constant","value":true}}}},"work_task":"task"}),
        false,
      ),
      (
        serde_json::json!({"entries":{"request":{"source_kind":"task","source":{"value":{"mapping":"constant","value":true}}}}}),
        false,
      ),
      (
        serde_json::json!({"entries":{"request":{"source_kind":"task","source":{"value":{"mapping":"constant","value":{"kind":"artifact","value":{"artifact_id":artifact.artifact_id(),"content_digest":artifact.content_digest(),"encoded_size":0}}}}}}}),
        false,
      ),
      (
        serde_json::json!({"entries":{"request":{"source_kind":"task","source":{"value":{"mapping":"object","fields":{"invalid":{"mapping":"constant","value":true}}}}}}}),
        false,
      ),
      (
        serde_json::json!({"entries":{"request":{"source_kind":"task","source":{"value":{"mapping":"constant","value":{"kind":"artifact","value":artifact}}}}}}),
        true,
      ),
      (
        serde_json::json!({"entries":{"request":{"source_kind":"task","source":{"value":{"mapping":"pointer","path":"/incoming_task"}}}}}),
        true,
      ),
      (
        serde_json::json!({"entries":{"request":{"source_kind":"task","source":{"value":{"mapping":"object","fields":{"kind":{"mapping":"constant","value":"artifact"},"value":{"mapping":"constant","value":artifact}}}}}}}),
        true,
      ),
      (
        serde_json::json!({"entries":{"request":{"source_kind":"task","source":{"value":{"mapping":"if_present","path":"/incoming_task","then":{"mapping":"pointer","path":"/incoming_task"},"otherwise":{"mapping":"constant","value":null}}}}}}),
        true,
      ),
      (serde_json::json!({"entries":{},"work_task":"task"}), true),
    ] {
      // Rebuild the immutable definition and its closure through their public constructors.
      let flow = draft.flow.as_ref().unwrap();
      let root = flow.closure.definition(flow.closure.root()).unwrap();
      let mut nodes = root.nodes().to_vec();
      let at = nodes
        .iter()
        .position(|node| node.profile_selection().is_some())
        .unwrap();
      let node = nodes[at].key().clone();
      nodes[at] = nodes[at]
        .clone()
        .with_context_binding(serde_json::from_value(binding.clone()).unwrap())
        .unwrap();
      let definition = FlowDefinition::new(FlowDefinitionInput {
        id: root.reference().id(),
        version: root.reference().version(),
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
      let reference = definition.reference();
      draft.flow.as_mut().unwrap().closure = PinnedFlowDefinitionClosure::new(reference, vec![definition]).unwrap();
      let result = query(
        &handlers,
        ValidateFactoryConfigurationQuery {
          draft: draft.clone(),
          ..request.clone()
        },
      )
      .await;
      if accepted {
        assert!(
          result.is_ok(),
          "explicit Work task, valid literal and dynamic Task bindings remain supported: {binding:?}: {result:?}"
        );
      } else {
        let Err(crate::ApplicationError::FactoryValidation(report)) = result else {
          panic!("invalid literal Task must block publication: {binding:?}");
        };
        assert!(
          report
            .errors
            .iter()
            .any(|issue| issue.field == "/context_binding/work_task"
              && issue.node.as_ref() == Some(&node)
              && issue.definition == Some(reference)),
          "Task diagnostics must identify the affected node and field: {report:?}"
        );
      }
    }
  });
}

#[test]
fn flow_publication_preserves_the_declared_accepted_predecessor() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = selected_fixture(project);
    let flow = fixture.draft.flow.unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    let mut nodes = root.nodes().to_vec();
    let entry = nodes.iter().position(|node| node.key() == root.entry()).unwrap();
    let schema = nodes[entry].input_schema().unwrap().clone();
    nodes[entry] = nodes[entry]
      .clone()
      .with_input_binding(
        FlowInputBinding::new(
          BTreeMap::from([(
            key("triage"),
            FlowInputSource::Incoming {
              schema,
              path: String::new(),
            },
          )]),
          FlowDataMapping::new(FlowValueMapping::Pointer { path: "/triage".into() }).unwrap(),
        )
        .unwrap(),
      )
      .unwrap();
    let predecessor = FlowAcceptedPredecessor {
      definition: root.reference(),
      terminal: key("succeeded"),
      outcomes: [key("succeeded")].into(),
    };
    let producer = PublishedFactoryFlowDefinition {
      project_id: project,
      flow: flow.clone(),
      published_at: time(0),
    };
    let draft = serde_json::from_value(serde_json::json!({
      "root": {
        "input_schema": root.input_schema(), "entry": root.entry(), "nodes": nodes,
        "transitions": root.transitions(), "terminals": root.terminals(),
        "context_projections": root.context_projections(), "data_projections": root.data_projections(),
        "execution": root.execution(),
      },
      "limits": flow.limits, "data_schemas": flow.data_schemas, "pools": flow.pools,
      "accepted_predecessor": predecessor,
    }))
    .expect("the ordinary authoring request accepts a predecessor contract");
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(crate::FactoryFlowDefinitionHandlers::new(
        Arc::new(PublicationChoices::for_publication(fixture.choices).with_predecessor(producer)),
        policy,
      )),
    );
    let published = handlers
      .handle_command(
        &context(),
        crate::CreateFactoryFlowDefinitionCommand {
          id: FlowDefinitionId::generate(),
          project_id: project,
          draft,
          idempotency_key: idempotency("with-predecessor"),
          published_at: time(1),
        },
      )
      .await
      .unwrap()
      .publication;
    assert_eq!(published.flow.accepted_predecessor, Some(predecessor));
  });
}
