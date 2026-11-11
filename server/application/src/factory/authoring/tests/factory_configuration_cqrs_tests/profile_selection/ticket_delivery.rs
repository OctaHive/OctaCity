//! The delivery roles compose through ordinary Flow authoring and the Build boundary.
use super::*;
use crate::{
  CreateFactoryFlowDefinitionCommand, FactoryArtifactFlowInputs, FactoryArtifactReader, FactoryFlowRunner,
  FactoryFlowStep, FactoryNodeBuildAdapter, ProjectDocumentSubmission, ProjectTextIntake,
  factory_flow_tests::Execution,
  factory_node_test_support::{clock, reconcile_admission_then_observe},
};
use octacity_server_domain::{RepositoryId, RepositorySourceSelection};
use octacity_server_store::testing::{
  ArtifactLeaseFixture, InMemoryArtifactRecordStore, InMemoryConfigurationStore, authoritative_store_contract_fixture,
  factory_profile_selection_capabilities, factory_profile_selection_contract_fixture, management_mutation_with_request,
};
use serde_json::json;
use std::collections::BTreeSet;

#[test]
fn ticket_delivery_roles_run_separate_invocations_with_only_their_declared_inputs() {
  run_ready(async {
    let (mut admission, original_input, ownership) = factory_profile_selection_contract_fixture(Some("baseline"));
    let project = admission.work.subject().project_id();
    let project_inputs = Arc::new(InMemoryFactoryConfigurationStore::new());
    project_inputs.seed_project(project).unwrap();
    let bytes = Arc::new(crate::factory::execution::factory_test_support::InMemoryArtifactBytes::default());
    let documents = ProjectTextIntake::new(
      project_inputs.clone(),
      bytes.clone(),
      Arc::new(crate::factory::admission::tests::factory_admission_tests::RecordingResolver::default()),
      Arc::new(TrustedNetworkManagementPolicy),
      clock(),
    );
    let mut published = BTreeMap::new();
    for (alias, content) in [
      ("task", "Reproduce the reported failure."),
      ("components", "Check the checkout component."),
      ("environment", "Use the staging environment configuration."),
    ] {
      let input = documents
        .publish_document(
          &context(),
          ProjectDocumentSubmission {
            purpose: Default::default(),
            project_id: project,
            name: alias.parse().unwrap(),
            content: content.into(),
            idempotency_key: idempotency(alias),
          },
        )
        .await
        .unwrap();
      published.insert(alias, input.artifact);
    }
    let original_work = &admission.work;
    admission.work = WorkEnvelope::new(
      original_work.id(),
      original_work.configuration().clone(),
      original_work.external_identity().clone(),
      original_work.subject().clone(),
      WorkArtifacts::new(
        published["task"].clone(),
        original_work.artifacts().acceptance().clone(),
        vec![],
      )
      .unwrap(),
      WorkClassification::new(
        original_work.priority(),
        original_work.risk(),
        original_work.metadata().clone(),
      ),
    )
    .unwrap();
    admission.run = FactoryRun::admitted(admission.run.id(), &admission.work);
    let original = &admission.flow.closure().definitions()[0];
    let baseline = &original.nodes()[0].profile_selection().unwrap().candidates[&key("baseline")];
    // Role names are existing Project vocabulary. Review uses a distinct profile of the coding role.
    let repositories = Arc::new(InMemoryConfigurationStore::new());
    repositories.seed_project(project).unwrap();
    let revision: octacity_server_domain::ImmutableRevision = "0123456789abcdef".parse().unwrap();
    let mut code_sources = BTreeMap::new();
    for alias in ["product", "available-but-unused"] {
      let id = RepositoryId::generate();
      let repository = repositories
        .create_repository(management_mutation_with_request(
          CreateRepository {
            id,
            project_id: project,
            name: alias.parse().unwrap(),
            definition: crate::factory::admission::tests::factory_admission_tests::repository(project, id, None)
              .definition,
            idempotency_key: idempotency(alias),
            published_at: time(1),
          },
          alias,
        ))
        .await
        .unwrap()
        .repository;
      code_sources.insert(
        key(alias),
        FlowCodeSource {
          repository: crate::factory::execution::factory_code_sources::repository_reference(&repository).unwrap(),
          selection: RepositorySourceSelection::ExactRevision(revision.clone()),
          access: MountMode::ReadOnly,
          scope: FlowCodeScope::Repository,
        },
      );
    }
    let selected_source = ExactSubject::new(project, code_sources[&key("product")].repository.id, revision.clone());
    let selected_scope = FlowCodeScope::Selected {
      entries: BTreeSet::from([FlowCodeSelection::Range {
        path: FactoryRepositoryPath::new("src/checkout.rs").unwrap(),
        lines: FactoryRepositoryLineRange::new(3, 7).unwrap(),
      }]),
    };
    let source_build = BuildConfigurationRef::new(
      BuildConfigurationId::generate(),
      BuildConfigurationVersion::INITIAL,
      project,
      digest(61),
    );
    let source_runtime = key("source-testing");
    let stages = [
      (
        "reproduction",
        "Quality engineer",
        "Source testing",
        "Try to reproduce the reported behavior.",
      ),
      (
        "requirements",
        "Business analyst",
        "Analysis",
        "Write the implementation acceptance criteria.",
      ),
      (
        "author_tests",
        "Quality engineer",
        "Testing",
        "Author the protected regression test.",
      ),
      (
        "implementation",
        "Software engineer",
        "Coding",
        "Implement the accepted change.",
      ),
      (
        "review",
        "Software engineer",
        "Review",
        "Review the exact implementation independently.",
      ),
      (
        "validation",
        "Quality engineer",
        "Testing",
        "Validate against the original protected tests.",
      ),
      (
        "delivery",
        "Release engineer",
        "Release",
        "Prepare the accepted candidate for review.",
      ),
    ];
    let budget = BudgetLimit::new(32, 300_000, 300_000, 300_000, 300_000).unwrap();
    let permissions = original.execution().permissions().clone();
    let mut source_grants = serde_json::to_value(&permissions).unwrap();
    source_grants["mounts"] = json!([{
      "root": octacity_protocol::FACTORY_SOURCE_ROOT,
      "mode": "read_only"
    }]);
    let source_permissions: FactoryPermissionSet = serde_json::from_value(source_grants).unwrap();
    let limits = FlowAdmissionLimits::product_defaults(1, budget, source_permissions.clone()).unwrap();
    let input = original_input.payload();
    let mut schemas = admission.flow.data_schemas().to_vec();
    let mut roles = BTreeMap::new();
    let mut profiles = BTreeMap::new();
    let mut nodes = Vec::new();
    let mut transitions = Vec::new();
    let mut reports = BTreeMap::new();
    let mut expected = Vec::new();
    for (index, (stage, role_name, profile_name, assignment)) in stages.iter().copied().enumerate() {
      let role = roles.entry(role_name).or_insert_with(|| {
        FactoryRole::publish(
          FactoryRoleId::generate(),
          project,
          FactoryRoleDefinition {
            avatar: None,
            skills: Default::default(),
            name: FactoryText::new(role_name).unwrap(),
            instructions: "Follow the Project conventions.".into(),
            template: None,
            icon: None,
            description: String::new(),
            concurrency_limit: None,
          },
        )
        .unwrap()
      });
      let profile = profiles.entry((role_name, profile_name)).or_insert_with(|| {
        let mut definition = baseline.profile.definition().clone();
        definition.name = FactoryText::new(profile_name).unwrap();
        if stage == "reproduction" {
          definition.build_configuration = source_build.clone();
          definition.permissions = source_permissions.clone();
        }
        FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, role, definition).unwrap()
      });
      let stage = key(stage);
      let outcome = key("reported");
      let mut value = json!({stage.as_str(): true});
      let mut fields = BTreeMap::from([(stage.clone(), FlowFieldSchema::required(FlowValueSchema::Boolean))]);
      if stage.as_str() == "review" {
        value["check_count"] = json!(17);
        fields.insert(
          key("check_count"),
          FlowFieldSchema::required(FlowValueSchema::Integer {
            minimum: 0,
            maximum: 100,
          }),
        );
      }
      let result = FlowDataSchema::new(stage.clone(), key("v1"), FlowValueSchema::Object { fields }).unwrap();
      let mut build = baseline.build.clone();
      build.result_outcome = outcome.clone();
      build.build_configuration = profile.definition().build_configuration.clone();
      let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
        key: stage.clone(),
        kind: FlowNodeKind::Reasoning,
        input_schema: Some(input.schema().clone()),
        outcomes: vec![FlowOutcomeDefinition::new(
          outcome.clone(),
          FlowOutcomeKind::Success,
          result.reference().clone(),
        )],
        budget: profile.definition().budget,
        permissions: profile.definition().permissions.clone(),
        required: true,
        subflow: None,
      })
      .unwrap()
      .with_task(assignment.into())
      .unwrap()
      .with_profile_selection(FlowProfileSelectionBinding {
        role: role.reference().clone(),
        candidates: BTreeMap::from([(
          key("assigned"),
          FlowProfileCandidate {
            profile: profile.clone(),
            build,
          },
        )]),
        policy: FlowProfileSelectionPolicy::Manual {
          candidate: Some(key("assigned")),
        },
        failure: FlowProfileSelectionFailure::ManualSelection,
        selection_budget: BudgetUsage::default(),
      })
      .unwrap();
      // Only validation explicitly projects one field from the predecessor's result.
      let binding = if stage.as_str() == "validation" {
        FlowInputBinding::new(
          BTreeMap::from([(
            key("checks"),
            FlowInputSource::Predecessor {
              view: FlowRecordView::Payload,
              path: "/check_count".into(),
            },
          )]),
          FlowDataMapping::new(FlowValueMapping::Object {
            fields: BTreeMap::from([(key("count"), FlowValueMapping::Pointer { path: "/checks".into() })]),
          })
          .unwrap(),
        )
        .unwrap()
      } else {
        FlowInputBinding::new(
          BTreeMap::new(),
          FlowDataMapping::new(FlowValueMapping::Constant {
            value: input.value().clone(),
          })
          .unwrap(),
        )
        .unwrap()
      };
      let node = node.with_input_binding(binding).unwrap();
      let node = match stage.as_str() {
        "reproduction" => node
          .with_context_binding(
            serde_json::from_value(json!({
              "work_task": "task",
              "entries": {}
            }))
            .unwrap(),
          )
          .unwrap(),
        "requirements" => node
          .with_context_binding(
            FlowContextBinding::new(
              None,
              BTreeMap::from([
                (
                  key("components"),
                  artifact_selection(ContextSourceKind::Specification, &published["components"]),
                ),
                (
                  key("environment"),
                  artifact_selection(ContextSourceKind::Evidence, &published["environment"]),
                ),
              ]),
            )
            .unwrap(),
          )
          .unwrap(),
        _ => node,
      };
      let code = if stage.as_str() == "reproduction" {
        FlowCodeBinding::Source {
          source: key("product"),
          access: MountMode::ReadOnly,
          scope: selected_scope.clone(),
          revision: FlowCodeRevision::ExactRevision {
            revision: revision.clone(),
          },
        }
      } else {
        FlowCodeBinding::None
      };
      let mut node = serde_json::to_value(node).unwrap();
      node["code"] = json!(code);
      let node = serde_json::from_value(node).unwrap();
      let target = stages.get(index + 1).map_or_else(
        || FlowTransitionTarget::Terminal(key("finished")),
        |next| FlowTransitionTarget::Node(key(next.0)),
      );
      transitions.push(FlowTransition::new(stage.clone(), outcome, target));
      reports.insert(stage.clone(), (result.reference().clone(), value));
      expected.push((stage, profile.reference().clone(), assignment));
      nodes.push(node);
      schemas.push(result);
    }
    let last_result = schemas.last().unwrap().reference().clone();
    let draft = crate::FactoryFlowDefinitionDraft {
      accepted_predecessor: None,
      issue_snapshot: None,
      root: crate::FactoryFlowRootDraft {
        name: Some(FactoryText::new("Delivery qualification").unwrap()),
        code_sources: Some(code_sources),
        process: None,
        input_schema: Some(input.schema().clone()),
        entry: key(stages[0].0),
        nodes,
        transitions,
        terminals: vec![FlowTerminalDefinition::new(key("finished"), last_result)],
        context_projections: vec![],
        data_projections: vec![],
        execution: FlowExecutionPolicy::new(budget, source_permissions.clone(), 1).unwrap(),
      },
      limits,
      data_schemas: schemas,
      pools: BTreeMap::new(),
    };
    let baseline_choices = factory_profile_selection_capabilities(&admission.flow);
    let mut entries = baseline_choices.entries();
    entries.references.push(FactoryReferenceChoice {
      kind: FactoryChoiceKind::NodeCapability,
      alias: source_runtime.clone(),
      reference: baseline.build.tool.clone(),
    });
    entries
      .build_configurations
      .push((source_runtime.clone(), source_build.clone()));
    let mut choices = FactoryConfigurationChoices::try_new(entries).unwrap();
    for (alias, capability) in baseline_choices.role_profile_capabilities() {
      choices = choices
        .with_role_profile_capability(alias.clone(), capability.clone())
        .unwrap();
    }
    let mut source_capability = baseline_choices
      .role_profile_capabilities()
      .values()
      .find(|capability| capability.ceiling.toolchain == baseline.profile.definition().toolchain)
      .unwrap()
      .clone();
    source_capability.ceiling.build_configuration = source_build;
    source_capability.ceiling.permissions = source_permissions.clone();
    source_capability.local_permissions = LocalPermissionCeiling::fully_enforced(source_permissions);
    let choices = choices
      .with_role_profile_capability(source_runtime, source_capability)
      .unwrap();
    let catalog = Arc::new(PublicationChoices::for_publication(choices));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(crate::FactoryFlowDefinitionHandlers::new(catalog.clone(), policy).with_repositories(repositories)),
    );
    let flow = handlers
      .handle_command(
        &context(),
        CreateFactoryFlowDefinitionCommand {
          id: FlowDefinitionId::generate(),
          project_id: project,
          draft,
          idempotency_key: idempotency("delivery-stage-contract"),
          published_at: time(1),
        },
      )
      .await
      .unwrap()
      .publication
      .flow;
    let root = FlowRun::root(&admission.run, flow.closure.root()).unwrap();
    let cycle = WorkflowCycle::initial(&root).unwrap();
    admission.flow = AdmittedFlow::new(flow.closure, flow.limits, root, cycle, admission.flow.wip_limits())
      .unwrap()
      .with_data_schemas(flow.data_schemas)
      .unwrap();
    let (store, claim) =
      crate::factory::execution::factory_test_support::start_profile_run(admission.clone(), ownership).await;
    let execution_fixture = Arc::new(Execution::for_reports(&reports));
    let fixture = authoritative_store_contract_fixture();
    let artifacts = Arc::new(InMemoryArtifactRecordStore::new(ArtifactLeaseFixture {
      access: LeaseAccess {
        lease_id: octacity_server_domain::LeaseId::generate(),
        fence: LeaseFence::from_bytes([7; 32]),
        agent_id: fixture.agent_id,
        registration_epoch: fixture.registration_epoch,
      },
      build_id: fixture.request.build.id,
      attempt_id: fixture.request.attempt_id,
      attempt: octacity_server_domain::AttemptNumber::FIRST,
      job_id: fixture.request.jobs[0].id,
      expires_at: time(10_000),
    }));
    let reader = Arc::new(
      FactoryArtifactReader::new(artifacts, execution_fixture.clone(), bytes.clone())
        .with_project_inputs(project_inputs),
    );
    let runner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(FactoryNodeBuildAdapter::new(
        execution_fixture.clone(),
        execution_fixture.clone(),
      )),
      Arc::new(FactoryArtifactFlowInputs::new(reader, clock())),
      clock(),
    )
    .with_profile_catalog(catalog);
    let run = admission.run.id();
    let reads_before_execution = bytes.2.load(std::sync::atomic::Ordering::SeqCst);
    let mut resolved = false;
    for tick in 31..100 {
      execution_fixture.clock.store(tick, std::sync::atomic::Ordering::SeqCst);
      match reconcile_admission_then_observe(&runner, store.as_ref(), run, claim.id, time(tick))
        .await
        .unwrap()
      {
        FactoryFlowStep::Advanced => {}
        FactoryFlowStep::Resolved(terminal) => {
          assert_eq!(terminal, key("finished"));
          resolved = true;
          break;
        }
        other => panic!("unexpected progress after publication: {other:?}"),
      }
    }
    assert!(resolved, "all seven steps must reach the configured terminal");
    let snapshot = store.factory_run_snapshot(run).await.unwrap();
    let mut operations = BTreeSet::new();
    let mut contexts = BTreeSet::new();
    let mut builds = BTreeSet::new();
    let intents = &snapshot.flow.data.build_intents;
    assert_eq!(intents.len(), stages.len());
    for (stage, profile, assignment) in expected {
      let intent = intents.iter().find(|intent| intent.input().node() == &stage).unwrap();
      assert_eq!(intent.selected_profile(), Some(&profile));
      assert_eq!(
        intent.step_task().map(FactoryInstructions::entries),
        Some(&[assignment.to_owned()][..]),
        "{stage} must receive its own assignment"
      );
      let operation = intent.operation_id().unwrap();
      assert!(operations.insert(operation));
      let execution = snapshot
        .flow
        .data
        .build_executions
        .iter()
        .find(|execution| execution.operation_id() == operation)
        .unwrap();
      assert!(builds.insert(execution.build_id()));
      let context = intent.input().context().unwrap();
      assert!(contexts.insert(context.id()));
      let expected_context = match stage.as_str() {
        "reproduction" => &["task"][..],
        "requirements" => &["components", "environment"][..],
        _ => &[][..],
      };
      assert_eq!(
        context.entries().len(),
        expected_context.len(),
        "unexpected Context for {stage}"
      );
      for alias in expected_context {
        let entry = context
          .entries()
          .iter()
          .find(|entry| entry.logical_identity().as_str() == *alias)
          .unwrap();
        assert_eq!(
          entry.source(),
          &FactoryContextReference::Artifact(published[alias].clone())
        );
      }
      let accepted = execution_fixture.by_build(execution.build_id());
      let requests = accepted.requests.lock().unwrap();
      let request = &requests[0];
      let dispatch = serde_json::from_slice::<serde_json::Value>(&request.causality.input).unwrap();
      if stage.as_str() == "reproduction" {
        assert_eq!(request.subject, FactoryTaskSubject::Exact(selected_source.clone()));
        assert_eq!(dispatch["code"]["scope"], json!(selected_scope));
        assert_eq!(dispatch["code"]["access"], "read_only");
        let scope = factory_job_source_scope(intent.input(), &request.effective_permissions)
          .unwrap()
          .unwrap();
        assert_eq!(
          scope.selections,
          vec![octacity_protocol::FactorySourceSelectionV1 {
            path: "src/checkout.rs".into(),
            lines: Some(octacity_protocol::FactorySourceLinesV1 { first: 3, last: 7 }),
          }]
        );
      } else {
        assert!(matches!(request.subject, FactoryTaskSubject::WorkInputV1(_)));
        assert_eq!(dispatch["code"], json!({"kind":"none"}));
        assert!(request.effective_permissions.mounts().next().is_none());
      }
      if stage.as_str() == "validation" {
        assert_eq!(dispatch["payload"]["value"], json!({"count":17}));
        assert_eq!(intent.input().sources().len(), 1);
      } else {
        assert_eq!(&dispatch["payload"]["value"], input.value());
        assert!(intent.input().sources().is_empty(), "control edges confer no data");
      }
      let result = snapshot
        .flow
        .data
        .records
        .iter()
        .find(|record| record.input().node() == &stage)
        .unwrap();
      assert_eq!(result.payload().schema(), &reports[&stage].0);
      assert_eq!(result.payload().value(), &reports[&stage].1);
    }
    assert_eq!(snapshot.flow.data.build_executions.len(), stages.len());
    assert_eq!(
      bytes.2.load(std::sync::atomic::Ordering::SeqCst) - reads_before_execution,
      3,
      "execution reads only the three explicitly selected artifacts"
    );
  });
}

fn artifact_selection(kind: ContextSourceKind, artifact: &FactoryArtifactReference) -> FlowContextSelection {
  FlowContextSelection {
    when_present: None,
    source_kind: kind,
    source: FlowDataMapping::new(FlowValueMapping::Constant {
      value: json!(FactoryContextReference::Artifact(artifact.clone())),
    })
    .unwrap(),
  }
}
