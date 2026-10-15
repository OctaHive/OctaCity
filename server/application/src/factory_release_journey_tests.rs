//! Configured owner slice over manual admission, nested definitions and installed ports.
use crate::factory_flow_tests::{Execution, FactoryTestOutputBuilder, Inputs};
use crate::factory_node_test_support::{Builds, fixtures::*};
use crate::factory_requirements_tests::{PublicationCase, Publisher, QualifiedOutputs};
use crate::*;
use octacity_server_factory::*;
use octacity_server_store::testing::InMemoryFactoryConfigurationStore;
use octacity_server_store::*;
use serde_json::{Value, json};
use std::{
  collections::BTreeMap,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
  },
};
fn at(t: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(t).unwrap()
}
fn load(name: &str) -> FlowDefinitionTemplate {
  let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .ancestors()
    .find(|p| p.join("config/factory/flows").is_dir())
    .unwrap()
    .join("config/factory/flows");
  FlowDefinitionTemplate::from_manifest(&std::fs::read(root.join(name)).unwrap(), |p| {
    std::fs::read(root.join(p.as_str())).map_err(|_| FactoryError::InvalidConfiguration {
      field: "configured example",
    })
  })
  .unwrap()
}
fn mapping(value: Value) -> FlowDataMapping {
  serde_json::from_value(json!({"value":value})).unwrap()
}
fn pointer(path: &str) -> Value {
  json!({"mapping":"pointer","path":path})
}
fn source(node: &str, outcome: &str, view: &str, path: &str) -> Value {
  json!({"source":"record","node":node,"outcome":outcome,"view":view,"path":path})
}
fn input_binding(sources: Value, value: Value) -> FlowInputBinding {
  serde_json::from_value(json!({"sources":sources,"mapping":{"value":value}})).unwrap()
}
#[expect(
  clippy::too_many_arguments,
  reason = "fixture supplies the public immutable node contract"
)]
fn node(
  key_: &str,
  kind: FlowNodeKind,
  schema: &FlowDataSchema,
  outcomes: Vec<FlowOutcomeDefinition>,
  budget: BudgetLimit,
  permissions: FactoryPermissionSet,
  subflow: Option<FlowDefinitionRef>,
  binding: FlowInputBinding,
) -> FlowNodeDefinition {
  FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key(key_),
    kind,
    input_schema: Some(schema.reference().clone()),
    outcomes,
    budget,
    permissions,
    required: true,
    subflow,
  })
  .unwrap()
  .with_input_binding(binding)
  .unwrap()
}
fn outcome(name: &str, schema: &FlowDataSchema) -> FlowOutcomeDefinition {
  FlowOutcomeDefinition::new(key(name), FlowOutcomeKind::Success, schema.reference().clone())
}
fn edge(from: &str, result: &str, to: &str) -> FlowTransition {
  FlowTransition::new(key(from), key(result), FlowTransitionTarget::Node(key(to)))
}
fn terminal_edge(from: &str, result: &str, to: &str) -> FlowTransition {
  FlowTransition::new(key(from), key(result), FlowTransitionTarget::Terminal(key(to)))
}

fn configuration(specified: bool, human_disposition: bool) -> (FactoryFlowConfiguration, Arc<Execution>, WorkEnvelope) {
  let (base, execution) = crate::factory_requirements_tests::specified_fixture(
    if specified { "feature_request" } else { "defect" },
    if specified { "medium" } else { "small" },
    "research",
    None,
    PublicationCase::Normal,
  );
  let mut execution = Arc::try_unwrap(execution).ok().unwrap();
  let work = base.work;
  let original = base.flow.closure().definition(base.flow.closure().root()).unwrap();
  let budget = original.nodes()[0].budget();
  let project = work.subject().project_id();
  let mut rollout = load("promotion-flow.json");
  if human_disposition {
    // Actual deterministic health/freshness rejection remains in front of the
    // human decision. The selected criterion itself needs no dummy evaluator.
    let mut human = rollout
      .nodes
      .iter()
      .find(|node| node.key == key("observation_policy"))
      .unwrap()
      .clone();
    human.key = key("operator_disposition");
    human.kind = FlowNodeKind::HumanGate;
    human.gate = None;
    human.input_schema = key("deployment_receipt");
    human.input_binding = input_binding(
      json!({"receipt":source("observation_policy","continue","payload","")}),
      pointer("/receipt"),
    );
    human.outcomes[1].key = key("escalate");
    rollout.nodes.push(human);
    let deploy = rollout
      .nodes
      .iter_mut()
      .find(|node| node.key == key("deploy_selected"))
      .unwrap();
    let action = deploy.action.as_ref().unwrap();
    let mut parameters = action.parameters().clone();
    parameters["plan"]["criteria"] = json!("operator_disposition");
    deploy.action = Some(FlowActionBinding::new(action.plugin().clone(), action.action().clone(), parameters).unwrap());
    rollout
      .transitions
      .retain(|route| !(route.predecessor() == &key("observation_policy") && route.outcome() == &key("continue")));
    rollout.transitions.extend([
      edge("observation_policy", "continue", "operator_disposition"),
      terminal_edge("operator_disposition", "continue", "promoted"),
      terminal_edge("operator_disposition", "escalate", "escalated"),
    ]);
  }
  let mut profiles = BTreeMap::new();
  for declaration in &rollout.nodes {
    if let Some(choice) = &declaration.build_profile {
      let mut profile = binding(project);
      profile.tool = declaration.action.as_ref().unwrap().plugin().clone();
      profile.model_or_tool = reference(choice.as_str());
      profile.result_output = key("deployment_receipt");
      profiles.insert(choice.clone(), profile);
    }
  }
  let rollout = rollout
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  let release = rollout
    .schemas
    .iter()
    .find(|schema| schema.reference().identity().as_str() == "factory.promotion.release_input")
    .unwrap()
    .clone();
  let mut nodes = original.nodes().to_vec();
  let ready = input_binding(
    json!({
      "candidate":source("implementation","observed","metadata","/build/outputs/candidate_facts/data/payload/value"),
      "bundle":source("implementation","observed","metadata","/build/outputs/change-set.bundle/artifact"),
      "manifest":source("implementation","observed","metadata","/build/outputs/change-set-manifest.json/artifact"),
      "original":{"source":"work","path":"/subject"},"contract":{"source":"work","path":"/artifacts/acceptance"}
    }),
    json!({"mapping":"object","fields":{
      "candidate_revision":pointer("/candidate/candidate_revision"),"candidate_digest":pointer("/manifest/content_digest"),
      "subject":{"mapping":"object","fields":{"exact":pointer("/original"),"candidate_revision":pointer("/candidate/candidate_revision"),"changeset_digest":pointer("/manifest/content_digest")}},
      "bundle":pointer("/bundle"),"manifest":pointer("/manifest"),"public_contract":pointer("/contract")
    }}),
  );
  nodes.push(
    node(
      "release_handoff",
      FlowNodeKind::DeterministicGate,
      &release,
      vec![outcome("ready", &release)],
      budget,
      FactoryPermissionSet::deny_all(),
      None,
      ready,
    )
    .with_gate(
      FlowGateProgram::new(vec![], key("ready"))
        .unwrap()
        .with_default_output_mapping(mapping(pointer("")))
        .unwrap()
        .binding()
        .unwrap(),
    )
    .unwrap(),
  );
  let mut transitions = original.transitions().to_vec();
  let route = transitions
    .iter_mut()
    .find(|edge| edge.predecessor() == &key("verification_result_gate") && edge.outcome() == &key("continue"))
    .unwrap();
  *route = edge("verification_result_gate", "continue", "release_handoff");
  transitions.push(terminal_edge("release_handoff", "ready", "verified_candidate"));
  let base_terminals = original
    .terminals()
    .iter()
    .map(|terminal| {
      if terminal.key() == &key("verified_candidate") {
        FlowTerminalDefinition::new(terminal.key().clone(), release.reference().clone())
      } else {
        terminal.clone()
      }
    })
    .collect();
  let base_definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: original.input_schema().cloned(),
    entry: original.entry().clone(),
    nodes,
    transitions,
    terminals: base_terminals,
    context_projections: original.context_projections().to_vec(),
    data_projections: original.data_projections().to_vec(),
    execution: original.execution().clone(),
  })
  .unwrap();
  let mut verification_wire = serde_json::to_value(load("verification-flow.json")).unwrap();
  let check_permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    network_hosts: vec![octacity_server_factory::NetworkHost::new("stage.example.test").unwrap()],
    ..Default::default()
  })
  .unwrap();
  verification_wire["execution"]["permissions"] = json!(check_permissions);
  for check in verification_wire["nodes"]
    .as_array_mut()
    .unwrap()
    .iter_mut()
    .filter(|node| node.get("build_profile").is_some())
  {
    check["permissions"] = json!(check_permissions);
  }
  let verification_template: FlowDefinitionTemplate = serde_json::from_value(verification_wire).unwrap();
  let profiles = verification_template
    .nodes
    .iter()
    .filter_map(|node| {
      node.build_profile.as_ref().map(|name| {
        let mut profile = binding(project);
        profile.model_or_tool = reference(name.as_str());
        (name.clone(), profile)
      })
    })
    .collect();
  let verification = verification_template
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  for definition in [&rollout.definition, &verification.definition] {
    for node in definition.nodes().iter().filter(|node| node.build().is_some()) {
      execution
        .requirements
        .insert(node.key().clone(), node.build().unwrap().evidence.clone());
      execution
        .result_schemas
        .insert(node.key().clone(), node.outcomes()[0].schema().clone());
    }
  }
  let mut schemas = base.flow.data_schemas().to_vec();
  schemas.extend(rollout.schemas);
  schemas.extend(verification.schemas);
  schemas.sort_by(|a, b| a.reference().cmp(b.reference()));
  schemas.dedup();
  let schema = |reference: &ImmutableReference| {
    schemas
      .iter()
      .find(|schema| schema.reference() == reference)
      .unwrap()
      .clone()
  };
  let work_schema = schema(base_definition.input_schema().unwrap());
  let verifier_input = schema(verification.definition.input_schema().unwrap());
  let receipt = schemas
    .iter()
    .find(|schema| schema.reference().identity().as_str() == "factory.base.candidate_receipt")
    .unwrap()
    .clone();
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![
      reference("fixture.publisher"),
      rollout
        .definition
        .node(&key("deploy_selected"))
        .unwrap()
        .action()
        .unwrap()
        .plugin()
        .clone(),
    ],
    network_hosts: vec![octacity_server_factory::NetworkHost::new("stage.example.test").unwrap()],
    ..Default::default()
  })
  .unwrap();
  let root_budget = BudgetLimit::new(160, 3600000, 1000000, 100000000, 33554432).unwrap();
  let call = |name: &str, child: &FlowDefinition, input: &FlowDataSchema, binding: FlowInputBinding| {
    node(
      name,
      FlowNodeKind::SubflowCall,
      input,
      child
        .terminals()
        .iter()
        .map(|terminal| {
          FlowOutcomeDefinition::new(
            terminal.key().clone(),
            FlowOutcomeKind::Success,
            terminal.schema().clone(),
          )
        })
        .collect(),
      child.execution().budget(),
      permissions.clone(),
      Some(child.reference()),
      binding,
    )
  };
  let candidate = call(
    "produce",
    &base_definition,
    &work_schema,
    base_definition
      .node(base_definition.entry())
      .unwrap()
      .input_binding()
      .unwrap()
      .clone(),
  );
  let verify = call(
    "independent_checks",
    &verification.definition,
    &verifier_input,
    input_binding(
      json!({"release":source("produce","verified_candidate","payload","")}),
      json!({"mapping":"object","fields":{"target":{"mapping":"object","fields":{"subject":pointer("/release/subject"),"public_contract":pointer("/release/public_contract"),"environment":{"mapping":"constant","value":reference("qualified.staged-deployment")},"endpoint":{"mapping":"constant","value":"https://stage.example.test"}}}}}),
    ),
  );
  let delivery = node(
    "review_delivery",
    FlowNodeKind::TrustedAction,
    &release,
    vec![outcome("published", &receipt)],
    budget,
    permissions.clone(),
    None,
    input_binding(
      json!({"release":source("produce","verified_candidate","payload","")}),
      pointer("/release"),
    ),
  )
  .with_action(
    FlowActionBinding::new(
      reference("fixture.publisher"),
      key("publish_candidate"),
      json!({"bundle":"candidate_bundle","manifest":"candidate_manifest","target":reference("implementation.review")}),
    )
    .unwrap(),
  )
  .unwrap()
  .with_context_binding(
    rollout
      .definition
      .node(&key("deploy_selected"))
      .unwrap()
      .context_binding()
      .unwrap()
      .clone(),
  )
  .unwrap();
  let approval = node(
    "operator_approval",
    FlowNodeKind::HumanGate,
    &release,
    vec![outcome("approved", &release), outcome("declined", &release)],
    budget,
    FactoryPermissionSet::deny_all(),
    None,
    input_binding(
      json!({"release":source("produce","verified_candidate","payload","")}),
      pointer("/release"),
    ),
  );
  let promotion = call(
    "selected_rollout",
    &rollout.definition,
    &release,
    input_binding(
      json!({"approved":source("operator_approval","approved","payload","")}),
      pointer("/approved"),
    ),
  );
  let mut root_transitions = vec![
    edge("produce", "verified_candidate", "independent_checks"),
    edge("independent_checks", "observed", "review_delivery"),
    edge("review_delivery", "published", "operator_approval"),
    edge("operator_approval", "approved", "selected_rollout"),
    terminal_edge("operator_approval", "declined", "declined"),
  ];
  let mut terminals = vec![FlowTerminalDefinition::new(
    key("declined"),
    release.reference().clone(),
  )];
  for (name, child) in [
    ("produce", &base_definition),
    ("independent_checks", &verification.definition),
    ("selected_rollout", &rollout.definition),
  ] {
    for terminal in child.terminals() {
      if name == "produce" && terminal.key() == &key("verified_candidate")
        || name == "independent_checks" && terminal.key() == &key("observed")
      {
        continue;
      }
      let root_key = key(&format!("{name}.{}", terminal.key()));
      root_transitions.push(FlowTransition::new(
        key(name),
        terminal.key().clone(),
        FlowTransitionTarget::Terminal(root_key.clone()),
      ));
      terminals.push(FlowTerminalDefinition::new(root_key, terminal.schema().clone()));
    }
  }
  let root = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(work_schema.reference().clone()),
    entry: key("produce"),
    nodes: vec![candidate, verify, delivery, approval, promotion],
    transitions: root_transitions,
    terminals,
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(root_budget, permissions.clone(), 4).unwrap(),
  })
  .unwrap();
  let closure = PinnedFlowDefinitionClosure::new(
    root.reference(),
    vec![root, base_definition, verification.definition, rollout.definition],
  )
  .unwrap();
  (
    FactoryFlowConfiguration::new(
      closure,
      FlowAdmissionLimits::product_defaults(4, root_budget, permissions).unwrap(),
      schemas,
      BTreeMap::new(),
    )
    .unwrap(),
    Arc::new(execution),
    work,
  )
}

struct ContextInputs;
#[async_trait::async_trait]
impl FactoryFlowInputPreparer for ContextInputs {
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    let definition = snapshot
      .admitted_flow
      .closure()
      .definition(
        snapshot
          .flow
          .runs
          .iter()
          .find(|run| run.id() == target.flow_run_id)
          .unwrap()
          .definition(),
      )
      .unwrap();
    if !definition
      .execution()
      .verification()
      .is_some_and(|policy| policy.checks.contains_key(&target.node))
    {
      return Inputs.prepare(snapshot, target).await;
    }
    let input = FactoryNodeRunner::<InMemoryFactoryConfigurationStore, FactoryNodeExecutorRegistry>::prepare_input(
      snapshot, target,
    )?;
    let subject: CandidateSubject =
      serde_json::from_value(input.payload().value()["target"]["subject"].clone()).unwrap();
    let artifact: FactoryArtifactReference =
      serde_json::from_value(input.payload().value()["target"]["public_contract"].clone()).unwrap();
    let subject = FactoryTaskSubject::Candidate(subject);
    let entry = ContextManifestEntry::new(
      ContextSourceKind::Specification,
      key("public-contract"),
      subject.clone(),
      FactoryContextReference::Artifact(artifact.clone()),
      artifact.content_digest(),
      artifact.encoded_size(),
      FactorySafeText::new("Exact public contract without source exposure").unwrap(),
      FactoryDigest::from_bytes([4; 32]),
    )
    .unwrap();
    input
      .with_context(
        ContextManifest::new(
          ContextManifestId::generate(),
          subject,
          FactoryDigest::from_bytes([4; 32]),
          vec![entry],
        )
        .unwrap(),
        vec![],
      )
      .map_err(|_| ApplicationError::invalid())
  }
}
struct QualifiedJourneyOutputs {
  failed: Arc<AtomicBool>,
}
impl FactoryTestOutputBuilder for QualifiedJourneyOutputs {
  fn outputs(
    &self,
    request: &CreateFactoryBuild,
    builds: &Builds,
    schema: &ImmutableReference,
    requirements: &[EvidenceRequirement],
  ) -> Option<FactoryNodeBuildOutputs> {
    let value = if let Some(check) = &request.causality.verification_check {
      json!(VerificationObservation {
        plan_digest: check.plan().digest().unwrap(),
        input_digest: request.causality.input_digest,
        check: request.causality.profile.node.clone(),
        outcome: DeterministicGateOutcome::Passed,
        measurements: vec![]
      })
    } else if let Some(action) = &request.causality.action {
      let public = request
        .causality
        .action_input
        .as_ref()
        .expect("CI receives its complete protected plan");
      let policy: PromotionPolicy = serde_json::from_value(public["plan"]["policy"].clone()).unwrap();
      let subject: CandidateSubject = serde_json::from_value(public["plan"]["subject"].clone()).unwrap();
      json!(PromotionObservation {
        plan_digest: serde_json::from_value(public["plan_digest"].clone()).unwrap(),
        input_digest: request.causality.input_digest,
        subject,
        environment: key("pilot"),
        environment_contract: policy.environments[&key("pilot")].clone(),
        deployment: reference("retained.ci-deployment"),
        outcome: if self.failed.load(Ordering::SeqCst) && action.action() == &key("invoke_rollout") {
          DeterministicGateOutcome::Failed
        } else {
          DeterministicGateOutcome::Passed
        },
        started_at: builds.created_at,
        finished_at: at(builds.created_at.unix_millis() + i64::try_from(policy.observation_window_millis).unwrap()),
        measurements: BTreeMap::from([(
          key("errors"),
          u64::from(self.failed.load(Ordering::SeqCst) && action.action() == &key("invoke_rollout"))
        )])
      })
    } else {
      return QualifiedOutputs {
        feedback: None,
        validation: None,
      }
      .outputs(request, builds, schema, requirements);
    };
    let published_at = if request.causality.action.is_some() {
      serde_json::from_value::<PromotionObservation>(value.clone())
        .unwrap()
        .finished_at
    } else {
      builds.created_at
    };
    let bytes = serde_json::to_vec(&value).unwrap();
    let size = bytes.len() as u64;
    let mut document = crate::factory_node_test_support::document_at(
      builds,
      &request.causality.profile.result_output,
      schema,
      bytes,
      request.causality.profile.tool.clone(),
      published_at,
    );
    document.plugin = request.causality.profile.plugin.clone();
    Some(FactoryNodeBuildOutputs {
      execution: FactoryNodeBuildExecutionFacts {
        profile: request.causality.profile.clone(),
        input_digest: request.causality.input_digest,
        permissions: request.effective_permissions.clone(),
        budget: request.budget,
        deadline: request.deadline,
      },
      documents: vec![document],
      usage: BudgetUsage {
        attempts: 1,
        output_bytes: size,
        ..Default::default()
      },
      verified_at: published_at,
      fresh_until: at(10000),
    })
  }
}

async fn admit_configuration(
  flow: FactoryFlowConfiguration,
  work: &WorkEnvelope,
) -> (Arc<InMemoryFactoryConfigurationStore>, FactoryRunId) {
  let project = work.subject().project_id();
  let repository = work.subject().repository_id();
  let configuration_id = work.configuration().id();
  let config = crate::factory_admission_tests::configuration_with_flow(project, configuration_id, Some(flow));
  let store = Arc::new(InMemoryFactoryConfigurationStore::new());
  store.seed_project(project).unwrap();
  store
    .seed_repository_version(crate::factory_admission_tests::repository(
      project,
      repository,
      Some(octacity_server_domain::SourceReference::new("refs/heads/main").unwrap()),
    ))
    .unwrap();
  store
    .seed_factory_configuration_version(PublishedFactoryConfiguration {
      configuration: config,
      published_at: at(1),
    })
    .unwrap();
  let handlers = FactoryAdmissionHandlers::new(
    store.clone(),
    Arc::new(crate::factory_admission_tests::RecordingResolver::default()),
  );
  let mut command = crate::factory_admission_tests::command(project, configuration_id, repository);
  command.artifacts = work.artifacts().clone();
  command.classification = WorkClassification::new(
    WorkPriority::new(20).unwrap(),
    RiskClass::Low,
    FactoryMetadata::default(),
  );
  let admitted = crate::factory_admission_tests::admit(&handlers, command).await.unwrap();
  (store, admitted.factory_run_id)
}

async fn journey(
  failed: bool,
  human_disposition: bool,
) -> (
  Arc<InMemoryFactoryConfigurationStore>,
  FactoryRunSnapshot,
  Arc<Execution>,
  FactoryRunClaimRecord,
  Arc<FactoryNodeExecutorRegistry>,
  Arc<AtomicBool>,
) {
  let (flow, execution, work) = configuration(!failed, human_disposition);
  let (store, run_id) = admit_configuration(flow, &work).await;
  let before = store.factory_run_snapshot(run_id).await.unwrap();
  assert_eq!(before.admitted_flow.closure().definitions().len(), 4);
  let mut execution = Arc::try_unwrap(execution).ok().unwrap();
  let failure = Arc::new(AtomicBool::new(failed));
  execution.output_builder = Some(Arc::new(QualifiedJourneyOutputs {
    failed: failure.clone(),
  }));
  let execution = Arc::new(execution);
  let publisher = Arc::new(Publisher {
    behavior: PublicationCase::Normal,
    lost: AtomicBool::new(false),
    writes: Mutex::new(BTreeMap::new()),
    requests: Mutex::new(vec![]),
  });
  let original = before
    .admitted_flow
    .closure()
    .definitions()
    .iter()
    .find_map(|definition| definition.node(&key("specification_publication")))
    .unwrap();
  let publication = Arc::new(FactoryFlowActionAdapter::new(Arc::new(
    FactoryCandidatePublicationAction::new(
      reference("fixture.publisher"),
      publisher.clone(),
      original.outcome(&key("published")).unwrap().clone(),
    ),
  )));
  let plain = Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone()));
  let promotion_identity = before
    .admitted_flow
    .closure()
    .definitions()
    .iter()
    .find_map(|definition| definition.node(&key("deploy_selected")))
    .unwrap()
    .action()
    .unwrap()
    .plugin()
    .clone();
  let promotion = Arc::new(FactoryPromotionBuildAdapter::new(
    promotion_identity.clone(),
    FactoryNodeBuildAdapter::new(execution.clone(), execution.clone()),
  ));
  let registry = Arc::new(
    FactoryNodeExecutorRegistry::new(vec![
      (
        FlowNodeKind::Reasoning,
        NodeExecutionIdentity::External(reference("audit.command")),
        plain.clone(),
      ),
      (
        FlowNodeKind::BuildCommand,
        NodeExecutionIdentity::External(reference("audit.command")),
        plain,
      ),
      (
        FlowNodeKind::TrustedAction,
        NodeExecutionIdentity::External(reference("fixture.publisher")),
        publication,
      ),
      (
        FlowNodeKind::TrustedAction,
        NodeExecutionIdentity::External(promotion_identity),
        promotion,
      ),
    ])
    .unwrap(),
  );
  let mut claim = crate::factory_requirements_tests::claim_run(store.as_ref(), run_id, 10).await;
  let mut resolved = None;
  for tick in 31..800 {
    execution.clock.store(tick, Ordering::SeqCst);
    let owner = FactoryFlowRunner::new(
      store.clone(),
      registry.clone(),
      Arc::new(ContextInputs),
      crate::factory_node_test_support::clock(),
    );
    match crate::factory_node_test_support::reconcile_admission_then_observe(
      &owner,
      store.as_ref(),
      run_id,
      claim.id,
      at(tick),
    )
    .await
    .unwrap_or_else(|error| panic!("owner failed at tick {tick}: {error:?}"))
    {
      FactoryFlowStep::Advanced | FactoryFlowStep::Waiting => {}
      FactoryFlowStep::Resolved(terminal) => {
        resolved = Some(terminal);
        break;
      }
      other => panic!("unexpected full-journey disposition {other:?}"),
    }
    let snapshot = store.factory_run_snapshot(run_id).await.unwrap();
    if let Some(pending) = snapshot.flow.attempts.iter().find(|attempt| {
      attempt.node_kind() == FlowNodeKind::HumanGate
        && !snapshot
          .flow
          .completions
          .iter()
          .any(|done| done.node_attempt_id() == attempt.id())
    }) {
      assert_eq!(
        crate::factory_node_test_support::reconcile_admission_then_observe(
          &owner,
          store.as_ref(),
          run_id,
          claim.id,
          at(tick)
        )
        .await
        .unwrap(),
        FactoryFlowStep::Waiting,
        "a human disposition cannot advance until its authorized exact response"
      );
      let input = snapshot
        .flow
        .data
        .inputs
        .iter()
        .find(|input| input.digest().unwrap() == pending.input_digest())
        .unwrap();
      FactoryControlHandlers::new(store.clone())
        .execute_management_command(
          &crate::factory_admission_tests::context(),
          &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
          ResolveFactoryNodeCommand {
            run_id,
            expected_version: snapshot.run.version(),
            node_attempt_id: pending.id(),
            outcome: key(if pending.node_key() == &key("operator_disposition") {
              "continue"
            } else {
              "approved"
            }),
            payload: input.payload().clone(),
            idempotency_key: IdempotencyKey::new(format!("approve-exact-{}", pending.id())).unwrap(),
            requested_at: at(tick),
          },
        )
        .await
        .unwrap();
      claim = crate::factory_requirements_tests::claim_run(store.as_ref(), run_id, tick).await;
    }
  }
  assert_eq!(
    resolved,
    Some(key(if failed {
      "selected_rollout.rolled_back"
    } else {
      "selected_rollout.promoted"
    }))
  );
  let snapshot = store.factory_run_snapshot(run_id).await.unwrap();
  assert_eq!(
    snapshot
      .flow
      .attempts
      .iter()
      .filter(|attempt| attempt.node_kind() == FlowNodeKind::HumanGate)
      .count(),
    if human_disposition { 2 } else { 1 }
  );
  assert_eq!(publisher.requests.lock().unwrap().len(), if failed { 1 } else { 2 });
  let report = snapshot
    .flow
    .data
    .records
    .iter()
    .find(|record| record.input().node() == &key("deploy_selected"))
    .unwrap();
  let deployed: PromotionObservation = serde_json::from_value(report.payload().value().clone()).unwrap();
  assert_eq!(
    deployed.subject,
    publisher.requests.lock().unwrap().last().unwrap().candidate
  );
  assert_eq!(
    snapshot
      .flow
      .data
      .build_intents
      .iter()
      .filter(|intent| intent.verification_check().is_some())
      .count(),
    7
  );
  assert_eq!(
    snapshot
      .flow
      .data
      .build_intents
      .iter()
      .filter(|intent| intent.action().is_some())
      .count(),
    if failed { 2 } else { 1 }
  );
  (store, snapshot, execution, claim, registry, failure)
}
#[test]
fn manual_work_runs_nested_quality_delivery_and_selected_rollout_through_one_owner() {
  crate::factory_admission_tests::run_ready(async {
    journey(false, false).await;
  });
}
#[test]
fn a_configured_human_post_observation_disposition_waits_for_the_exact_authorized_response() {
  crate::factory_admission_tests::run_ready(async {
    let (_, snapshot, _, _, _, _) = journey(false, true).await;
    let input = snapshot
      .flow
      .data
      .inputs
      .iter()
      .find(|input| input.node() == &key("deploy_selected"))
      .unwrap();
    let plan = PromotionPlan::freeze(&snapshot.admitted_flow, input).unwrap();
    assert_eq!(plan.criteria().kind(), FlowNodeKind::HumanGate);
    assert_eq!(plan.criteria().key(), &key("operator_disposition"));
    assert!(
      snapshot
        .flow
        .data
        .records
        .iter()
        .any(|record| record.input().node() == &key("operator_disposition")
          && record.metadata().unwrap()["outcome"] == "continue")
    );
  });
}

#[test]
fn a_failed_bounded_observation_rolls_back_and_external_monitoring_cannot_rewrite_it() {
  crate::factory_admission_tests::run_ready(async {
    let (store, snapshot, execution, claim, registry, failed) = journey(true, false).await;
    failed.store(false, Ordering::SeqCst);
    execution.clock.store(900, Ordering::SeqCst);
    let owner = FactoryFlowRunner::new(
      store.clone(),
      registry.clone(),
      Arc::new(ContextInputs),
      crate::factory_node_test_support::clock(),
    );
    assert_eq!(
      crate::factory_node_test_support::reconcile_admission_then_observe(
        &owner,
        store.as_ref(),
        snapshot.run.id(),
        claim.id,
        at(900)
      )
      .await
      .unwrap(),
      FactoryFlowStep::Resolved(key("selected_rollout.rolled_back"))
    );
    assert_eq!(store.factory_run_snapshot(snapshot.run.id()).await.unwrap(), snapshot);
    // External monitoring enters through ordinary Work admission; it has no owner
    // transition port and cannot reuse the completed rollout's claim or history.
    let report = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|record| record.input().node() == &key("deploy_selected"))
      .unwrap()
      .build_provenance()
      .unwrap()
      .outputs()[0]
      .artifact
      .clone();
    let mut command = crate::factory_admission_tests::command(
      snapshot.work.subject().project_id(),
      snapshot.work.configuration().id(),
      snapshot.work.subject().repository_id(),
    );
    command.external_identity = ExternalWorkIdentity::new("monitoring/new-observation").unwrap();
    command.idempotency_key = IdempotencyKey::new("monitoring-new-work").unwrap();
    command.artifacts =
      WorkArtifacts::new(report.clone(), snapshot.work.artifacts().acceptance().clone(), vec![]).unwrap();
    command.admitted_at = at(901);
    let admission = FactoryAdmissionHandlers::new(
      store.clone(),
      Arc::new(crate::factory_admission_tests::RecordingResolver::default()),
    );
    let new = crate::factory_admission_tests::admit(&admission, command)
      .await
      .unwrap();
    assert_ne!(new.factory_run_id, snapshot.run.id());
    let new_snapshot = store.factory_run_snapshot(new.factory_run_id).await.unwrap();
    assert_eq!(new_snapshot.work.artifacts().task(), &report);
    assert!(new_snapshot.flow.data.records.is_empty());
    let new_claim = crate::factory_requirements_tests::claim_run(store.as_ref(), new.factory_run_id, 902).await;
    for tick in 903..922 {
      execution.clock.store(tick, Ordering::SeqCst);
      assert!(matches!(
        crate::factory_node_test_support::reconcile_admission_then_observe(
          &owner,
          store.as_ref(),
          new.factory_run_id,
          new_claim.id,
          at(tick)
        )
        .await
        .unwrap(),
        FactoryFlowStep::Advanced | FactoryFlowStep::Waiting
      ));
    }
    let new_snapshot = store.factory_run_snapshot(new.factory_run_id).await.unwrap();
    for entry in ["eligibility", "classification"] {
      assert!(
        new_snapshot
          .flow
          .data
          .records
          .iter()
          .any(|record| record.input().node() == &key(entry)),
        "a monitoring report must pass both configured triage phases"
      );
    }
    assert_eq!(store.factory_run_snapshot(snapshot.run.id()).await.unwrap(), snapshot);
  });
}

#[test]
fn optional_declared_nodes_can_be_added_removed_and_reordered_in_the_admitted_configuration() {
  crate::factory_admission_tests::run_ready(async {
    for order in [
      vec![],
      vec!["operator_preflight", "operator_audit"],
      vec!["operator_audit", "operator_preflight"],
    ] {
      let (flow, _, work) = configuration(false, false);
      let original = flow.closure.definition(flow.closure.root()).unwrap();
      let schema = flow
        .data_schemas
        .iter()
        .find(|schema| Some(schema.reference()) == original.input_schema())
        .unwrap();
      let entry = original.node(original.entry()).unwrap();
      let mut nodes = original.nodes().to_vec();
      let mut transitions = original.transitions().to_vec();
      for (index, name) in order.iter().enumerate() {
        nodes.push(
          node(
            name,
            FlowNodeKind::DeterministicGate,
            schema,
            vec![outcome("ready", schema)],
            entry.budget(),
            FactoryPermissionSet::deny_all(),
            None,
            entry.input_binding().unwrap().clone(),
          )
          .with_gate(
            FlowGateProgram::new(vec![], key("ready"))
              .unwrap()
              .with_default_output_mapping(mapping(pointer("")))
              .unwrap()
              .binding()
              .unwrap(),
          )
          .unwrap(),
        );
        transitions.push(edge(
          name,
          "ready",
          order.get(index + 1).copied().unwrap_or(original.entry().as_str()),
        ));
      }
      let root = FlowDefinition::new(FlowDefinitionInput {
        id: FlowDefinitionId::generate(),
        version: FlowDefinitionVersion::INITIAL,
        input_schema: original.input_schema().cloned(),
        entry: order.first().map_or_else(|| original.entry().clone(), |name| key(name)),
        nodes,
        transitions,
        terminals: original.terminals().to_vec(),
        context_projections: original.context_projections().to_vec(),
        data_projections: original.data_projections().to_vec(),
        execution: original.execution().clone(),
      })
      .unwrap();
      let mut definitions = flow
        .closure
        .definitions()
        .iter()
        .filter(|definition| definition.reference() != original.reference())
        .cloned()
        .collect::<Vec<_>>();
      definitions.push(root.clone());
      let changed = FactoryFlowConfiguration::new(
        PinnedFlowDefinitionClosure::new(root.reference(), definitions).unwrap(),
        flow.limits.clone(),
        flow.data_schemas.to_vec(),
        BTreeMap::new(),
      )
      .unwrap();
      let (store, run) = admit_configuration(changed, &work).await;
      let claim = crate::factory_requirements_tests::claim_run(store.as_ref(), run, 10).await;
      let registry = Arc::new(FactoryNodeExecutorRegistry::new(vec![]).unwrap());
      let owner = FactoryFlowRunner::new(
        store.clone(),
        registry,
        Arc::new(ContextInputs),
        crate::factory_node_test_support::clock(),
      );
      for tick in 31..(32 + 2 * order.len() as i64) {
        assert_eq!(
          crate::factory_node_test_support::reconcile_admission_then_observe(
            &owner,
            store.as_ref(),
            run,
            claim.id,
            at(tick)
          )
          .await
          .unwrap(),
          FactoryFlowStep::Advanced
        );
      }
      let snapshot = store.factory_run_snapshot(run).await.unwrap();
      let mut records = snapshot.flow.data.records.iter().collect::<Vec<_>>();
      records.sort_by_key(|record| record.observed_at());
      let recorded = records
        .iter()
        .map(|record| record.input().node().as_str())
        .collect::<Vec<_>>();
      assert_eq!(recorded, order);
      assert!(
        snapshot
          .flow
          .attempts
          .iter()
          .any(|attempt| attempt.node_key() == &key("produce")),
        "the configured entry resumes ordinary nested work"
      );
    }
  });
}
