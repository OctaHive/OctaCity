use crate::factory_node_test_support::fixtures;
use crate::factory_node_test_support::{Builds, document};
use crate::{
  ApplicationError, CreateFactoryBuild, FactoryBuildAcceptance, FactoryFlowCoordinator, FactoryFlowInputPreparer,
  FactoryFlowRunner, FactoryFlowStep, FactoryNodeBuildAdapter, FactoryNodeBuildExecutionFacts,
  FactoryNodeBuildOutputSource, FactoryNodeBuildOutputs, FactoryNodeExecutionRequest, FactoryNodeExecutionStep,
  FactoryNodeExecutor, FactoryNodeRunner, FactoryNodeTarget, OrdinaryBuildApplication, OrdinaryBuildApplicationError,
};
use async_trait::async_trait;
use fixtures::{key, reference};
use octacity_server_factory::*;
use octacity_server_store::testing as store_testing;
use octacity_server_store::*;
use std::{
  collections::BTreeMap,
  sync::{Arc, Mutex},
};
fn at(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}

pub(crate) struct Inputs;
#[async_trait]
impl FactoryFlowInputPreparer for Inputs {
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    let input = FactoryNodeRunner::<store_testing::InMemoryFactoryConfigurationStore, FakePending>::prepare_input(
      snapshot, target,
    )?;
    let binding = snapshot
      .admitted_flow
      .closure()
      .definition(input.definition())
      .unwrap()
      .node(input.node())
      .unwrap()
      .context_binding();
    if let Some(binding) = binding {
      let selected = binding.selected_sources(input.payload()).unwrap();
      let subject = match binding.candidate_revision(input.payload()).unwrap() {
        None => FactoryTaskSubject::Exact(snapshot.work.subject().clone()),
        Some(revision) => {
          let (_, (_, FactoryContextReference::Artifact(manifest))) = selected
            .iter()
            .find(|(name, _)| name.as_str() == "manifest" || name.as_str() == "candidate_manifest")
            .unwrap()
          else {
            panic!("fixture requires a verified manifest artifact")
          };
          FactoryTaskSubject::Candidate(CandidateSubject::new(
            snapshot.work.subject().clone(),
            revision,
            input
              .payload()
              .value()
              .get("candidate_digest")
              .and_then(|digest| digest.as_str())
              .map(|digest| digest.parse().unwrap())
              .unwrap_or_else(|| manifest.content_digest()),
          ))
        }
      };
      let entries = selected
        .into_iter()
        .map(|(name, (kind, source))| {
          let FactoryContextReference::Artifact(artifact) = &source else {
            panic!("fixture has only independently verified artifacts")
          };
          ContextManifestEntry::new(
            kind,
            name,
            subject.clone(),
            source.clone(),
            artifact.content_digest(),
            artifact.encoded_size(),
            FactorySafeText::new("Qualified exact fixture artifact").unwrap(),
            FactoryDigest::from_bytes([4; 32]),
          )
          .unwrap()
        })
        .collect();
      return input
        .with_context(
          ContextManifest::new(
            ContextManifestId::generate(),
            subject,
            FactoryDigest::from_bytes([4; 32]),
            entries,
          )
          .unwrap(),
          vec![],
        )
        .map_err(|_| ApplicationError::invalid());
    }
    let subject = FactoryTaskSubject::Exact(snapshot.work.subject().clone());
    let task = snapshot.work.artifacts().task().clone();
    let manifest = ContextManifest::new(
      ContextManifestId::generate(),
      subject.clone(),
      FactoryDigest::from_bytes([4; 32]),
      vec![
        ContextManifestEntry::new(
          ContextSourceKind::Task,
          key("task"),
          subject,
          FactoryContextReference::Artifact(task.clone()),
          task.content_digest(),
          task.encoded_size(),
          FactorySafeText::new("Configured task projection").unwrap(),
          FactoryDigest::from_bytes([4; 32]),
        )
        .unwrap(),
      ],
    )
    .unwrap();
    input
      .with_context(manifest, vec![])
      .map_err(|_| ApplicationError::invalid())
  }
}
struct FakePending;
#[async_trait]
impl FactoryNodeExecutor for FakePending {
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: octacity_server_domain::Timestamp,
  ) -> Result<crate::FactoryNodeCancellationStep, ApplicationError> {
    panic!("this fixture has no cancellable external operation")
  }

  async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    Ok(FactoryNodeExecutionStep::Waiting { execution: None })
  }
}

pub(crate) struct Execution {
  pub(crate) clock: std::sync::atomic::AtomicI64,
  pub(crate) seeder: Option<Arc<dyn FactoryTestBuildSeeder>>,
  builds: Mutex<BTreeMap<FactoryDigest, Arc<Builds>>>,
  requirements: BTreeMap<FactoryKey, Vec<EvidenceRequirement>>,
  result_schemas: BTreeMap<FactoryKey, ImmutableReference>,
  pub(crate) reports: BTreeMap<FactoryKey, serde_json::Value>,
  pub(crate) facts: BTreeMap<FactoryKey, serde_json::Value>,
  pub(crate) output_builder: Option<Arc<dyn FactoryTestOutputBuilder>>,
}
pub(crate) trait FactoryTestOutputBuilder: Send + Sync {
  fn outputs(
    &self,
    request: &CreateFactoryBuild,
    builds: &Builds,
    schema: &ImmutableReference,
    requirements: &[EvidenceRequirement],
  ) -> Option<FactoryNodeBuildOutputs>;
}
impl Execution {
  pub(crate) fn by_build(&self, id: octacity_server_domain::BuildId) -> Arc<Builds> {
    self
      .builds
      .lock()
      .unwrap()
      .values()
      .find(|build| build.acceptance.build_id == id)
      .unwrap()
      .clone()
  }
}
#[async_trait]
impl OrdinaryBuildApplication for Execution {
  async fn factory_build_for_operation(
    &self,
    operation_id: FactoryDigest,
  ) -> Result<Option<FactoryBuildAcceptance>, OrdinaryBuildApplicationError> {
    Ok(
      self
        .builds
        .lock()
        .unwrap()
        .get(&operation_id)
        .map(|build| build.acceptance.clone()),
    )
  }
  async fn create_factory_build(
    &self,
    request: CreateFactoryBuild,
  ) -> Result<FactoryBuildAcceptance, OrdinaryBuildApplicationError> {
    let builds = {
      let mut map = self.builds.lock().unwrap();
      map
        .entry(request.operation_id)
        .or_insert_with(|| {
          let builds = Arc::new(Builds::new_at(at(self.clock.load(std::sync::atomic::Ordering::SeqCst))));
          *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
          builds
        })
        .clone()
    };
    let accepted = builds.create_factory_build(request.clone()).await?;
    if let Some(seeder) = &self.seeder {
      seeder.seed(&request, &accepted).await;
    }
    Ok(accepted)
  }
}
#[async_trait]
impl BuildQueryStore for Execution {
  async fn build(&self, id: octacity_server_domain::BuildId) -> Result<BuildRecord, StoreError> {
    self.by_build(id).build(id).await
  }
  async fn latest_attempt(&self, id: octacity_server_domain::BuildId) -> Result<AttemptRecord, StoreError> {
    self.by_build(id).latest_attempt(id).await
  }
  async fn attempt(&self, id: octacity_server_domain::AttemptId) -> Result<AttemptRecord, StoreError> {
    let build = self
      .builds
      .lock()
      .unwrap()
      .values()
      .find(|build| build.acceptance.attempt_id == id)
      .cloned()
      .ok_or(StoreError::Unavailable)?;
    build.attempt(id).await
  }
  async fn job(&self, id: octacity_server_domain::JobId) -> Result<JobRecord, StoreError> {
    let build = self
      .builds
      .lock()
      .unwrap()
      .values()
      .find(|build| build.acceptance.job_ids.contains(&id))
      .cloned()
      .ok_or(StoreError::Unavailable)?;
    build.job(id).await
  }
}
#[async_trait]
impl crate::OrdinaryBuildCancellationApplication for Execution {
  async fn cancel_factory_build(
    &self,
    request: crate::CancelFactoryBuild,
  ) -> Result<(), OrdinaryBuildApplicationError> {
    self.by_build(request.build_id).cancel_factory_build(request).await
  }
}
#[async_trait]
impl FactoryNodeBuildOutputSource for Execution {
  async fn terminal_usage(
    &self,
    _: octacity_server_domain::BuildId,
    _: octacity_server_domain::AttemptId,
  ) -> Result<BudgetUsage, OrdinaryBuildApplicationError> {
    Ok(BudgetUsage::default())
  }

  async fn published_outputs(
    &self,
    build: octacity_server_domain::BuildId,
    _: octacity_server_domain::AttemptId,
    max_bytes: u64,
  ) -> Result<FactoryNodeBuildOutputs, OrdinaryBuildApplicationError> {
    let builds = self.by_build(build);
    let request = builds.requests.lock().unwrap()[0].clone();
    let causality = request.causality.as_ref();
    let profile = &causality.profile;
    if let Some(outputs) = self.output_builder.as_ref().and_then(|builder| {
      builder.outputs(
        &request,
        &builds,
        &self.result_schema(profile.node.clone()),
        &self.requirements[&profile.node],
      )
    }) {
      return Ok(outputs);
    }
    let wire: serde_json::Value = serde_json::from_slice(&causality.input).unwrap();
    let schema = self.result_schema(profile.node.clone());
    let report = self.reports.get(&profile.node).cloned().unwrap_or_else(|| {
      let input = &wire["payload"]["value"];
      match schema.identity().as_str() {
        "factory.base.protected_test_handoff" => {
          serde_json::json!({"accepted_contract":causality.input_digest,"classes":["project.custom"]})
        }
        "factory.base.protected_validation_report" => serde_json::json!({"changeset_id":ChangeSetId::generate(),
            "candidate":causality.context.subject().candidate().unwrap(),"tests":input["protected_inventory"],
            "input_digest":causality.input_digest,"classes":[{"class":"project.custom","outcome":"passed"}]}),
        "factory.base.code_review" => serde_json::json!({"candidate_revision":input["candidate_revision"],
          "candidate_digest":input["candidate_digest"],"input_digest":causality.input_digest,
          "quality":{"correctness":"satisfied","maintainability":"satisfied","public_tests":"satisfied","security":"satisfied"},"findings":[]}),
        "factory.base.validation_classification" => serde_json::json!({"candidate_revision":input["request"]["candidate_revision"],
          "candidate_digest":input["request"]["candidate_digest"],"validation_digest":input["request"]["validation_digest"],
          "input_digest":causality.input_digest,"classification":"implementation_defect","remediation":"Preserve the public contract"}),
        _ => input.get("accepted").unwrap_or(input).clone(),
      }
    });
    let mut observed = document(
      &builds,
      &profile.result_output,
      &schema,
      serde_json::to_vec(&report).unwrap(),
      profile.result_tool().clone(),
    );
    observed.plugin = profile.plugin.clone();
    let mut documents = vec![observed];
    for requirement in &self.requirements[&profile.node] {
      let mut value = if let Some(value) = fixture_capture_or_validation_value(
        &request,
        requirement,
        FactoryDigest::content_sha256(&documents[0].bytes),
      ) {
        value
      } else if requirement.output_kind() == EvidenceOutputKind::Artifact {
        serde_json::json!({"problem":"bounded feature proposal","sources":["admitted.task"],"assumptions":[],"alternatives":["existing contract"],"unresolved_questions":[]})
      } else if requirement.kind() == &key("terminal_resolution") {
        serde_json::json!({"accepted":false,"outcome":"unresolved"})
      } else if requirement.kind() == &key("execution_facts") {
        serde_json::json!({"outcome":"observed","candidate_revision":request.immutable_revision,"report_digest":FactoryDigest::content_sha256(&documents[0].bytes),"fresh_until":10000,"requirements_evidence":[]})
      } else {
        self.facts[&profile.node].clone()
      };
      value["input_digest"] = serde_json::json!(causality.input_digest);
      let mut proof = crate::factory_node_test_support::document_kind(
        &builds,
        requirement.kind(),
        requirement.schema(),
        serde_json::to_vec(&value).unwrap(),
        requirement.tool().clone(),
        requirement.output_kind(),
      );
      proof.plugin = requirement.plugin().clone();
      documents.push(proof);
    }
    let output_bytes = documents.iter().map(|doc| doc.record.identity().size_bytes).sum();
    assert!(output_bytes <= max_bytes);
    Ok(FactoryNodeBuildOutputs {
      execution: FactoryNodeBuildExecutionFacts {
        profile: profile.clone(),
        input_digest: causality.input_digest,
        permissions: request.effective_permissions,
        budget: request.budget,
        deadline: request.deadline,
      },
      documents,
      usage: BudgetUsage {
        attempts: 1,
        output_bytes,
        ..Default::default()
      },
      verified_at: builds.created_at,
      fresh_until: at(10000),
    })
  }
}
/// Metadata-only fixtures for the configured owner contract. Byte/capture trust
/// is exercised separately through the installed capture and validation ports.
pub(crate) fn fixture_capture_or_validation_value(
  request: &CreateFactoryBuild,
  requirement: &EvidenceRequirement,
  report_digest: FactoryDigest,
) -> Option<serde_json::Value> {
  let c = request.causality.as_ref();
  let wire: serde_json::Value = serde_json::from_slice(&c.input).unwrap();
  let input = &wire["payload"]["value"];
  match requirement.kind().as_str() {
    "candidate_facts" => Some(
      serde_json::json!({"candidate_revision":format!("implementation-{}",c.node_attempt_id),
      "candidate_digest":FactoryDigest::from_bytes([11;32]),"input_digest":c.input_digest,"fresh_until":10000}),
    ),
    "code_review_facts" | "classification_facts" => Some(serde_json::json!({"accepted":true,
      "input_digest":c.input_digest,"report_digest":report_digest,"fresh_until":10000})),
    "validation_facts" => Some(
      serde_json::json!({"accepted":true,"input_digest":c.input_digest,"report_digest":report_digest,
      "candidate_revision":input["candidate_revision"],"candidate_digest":input["candidate_digest"],
      "protected_inventory":input["protected_inventory"],"covered_classes":input["classes"],"fresh_until":10000}),
    ),
    "change-set.bundle" | "change-set-manifest.json" => {
      Some(serde_json::json!({"fixture":"opaque immutable candidate Artifact"}))
    }
    _ => None,
  }
}
impl Execution {
  fn result_schema(&self, node: FactoryKey) -> ImmutableReference {
    self.result_schemas[&node].clone()
  }
}

pub(crate) fn configured_fixture() -> (PublishedFactoryAdmission, Arc<Execution>) {
  let mut template: FlowDefinitionTemplate = crate::factory_node_test_support::fixtures::supplied_base_flow_template();
  let mut audit = template
    .nodes
    .iter()
    .find(|node| node.key == key("protected_tests"))
    .unwrap()
    .clone();
  audit.key = key("accessibility_audit");
  audit.outcomes[0].schema = key("summary");
  audit.pool = None;
  audit.build_profile = Some(key("accessibility.profile"));
  template.nodes.push(audit);
  template.transitions = template
    .transitions
    .iter()
    .map(|edge| {
      if edge.predecessor() == &key("requirements_needed") && edge.outcome() == &key("bypass") {
        FlowTransition::new(
          key("requirements_needed"),
          key("bypass"),
          FlowTransitionTarget::Node(key("accessibility_audit")),
        )
      } else {
        edge.clone()
      }
    })
    .collect();
  template.transitions.push(FlowTransition::new(
    key("accessibility_audit"),
    key("observed"),
    FlowTransitionTarget::Node(key("protected_tests")),
  ));
  configured_fixture_from_template(template, "/accepted")
}

pub(crate) fn configured_fixture_from_template(
  template: FlowDefinitionTemplate,
  pool_payload_root: &str,
) -> (PublishedFactoryAdmission, Arc<Execution>) {
  let work = store_testing::factory_node_journal_contract_fixture().admission.work;
  fixture_with_work_and_pool_root(template, work, pool_payload_root)
}
pub(crate) fn configured_fixture_with_work(
  template: FlowDefinitionTemplate,
  work: WorkEnvelope,
) -> (PublishedFactoryAdmission, Arc<Execution>) {
  fixture_with_work_and_pool_root(template, work, "/accepted")
}
fn fixture_with_work_and_pool_root(
  template: FlowDefinitionTemplate,
  work: WorkEnvelope,
  pool_payload_root: &str,
) -> (PublishedFactoryAdmission, Arc<Execution>) {
  let schemas = template.data_schemas().unwrap();
  let fact_schemas = BTreeMap::from([
    (key("eligibility"), key("eligibility_facts")),
    (key("classification"), key("classification_facts")),
    (key("defect_research"), key("defect_facts")),
    (key("feature_research"), key("feature_facts")),
    (key("implementation"), key("execution_facts")),
    (key("verification"), key("execution_facts")),
  ]);
  let mut profiles = BTreeMap::new();
  let mut requirements = BTreeMap::new();
  let mut result_schemas = BTreeMap::new();
  for node in &template.nodes {
    if let Some(choice) = &node.build_profile {
      let mut profile = fixtures::binding(work.subject().project_id());
      profile.task_digest = FactoryDigest::sha256("fixture.profile-task", &[choice.as_str().as_bytes()]);
      profile.model_or_tool = reference(choice.as_str());
      profile.result_output = key("observations");
      if let Some(facts) = fact_schemas.get(&node.key) {
        profile.evidence = vec![EvidenceRequirement::new(
          if matches!(node.key.as_str(), "implementation" | "verification") {
            key("execution_facts")
          } else {
            key("facts")
          },
          EvidenceOutputKind::Report,
          schemas[facts].reference().clone(),
          reference("independent.verifier"),
          reference("verifier.plugin"),
        )];
      }
      if node.key == key("implementation") {
        for name in ["change-set.bundle", "change-set-manifest.json"] {
          profile.evidence.push(EvidenceRequirement::new(
            key(name),
            EvidenceOutputKind::Artifact,
            reference("capture.artifacts"),
            reference("git.capture"),
            profile.plugin.clone(),
          ));
        }
        profile.evidence.push(EvidenceRequirement::new(
          key("candidate_facts"),
          EvidenceOutputKind::Report,
          schemas[&key("candidate_capture_facts")].reference().clone(),
          reference("git.capture"),
          profile.plugin.clone(),
        ));
      }
      if node.key == key("feature_research") {
        profile.evidence.push(EvidenceRequirement::new(
          key("proposal"),
          EvidenceOutputKind::Artifact,
          reference("proposal.verified"),
          reference("proposal.verifier"),
          reference("verifier.plugin"),
        ));
      }
      if matches!(node.key.as_str(), "defect_research" | "feature_research") {
        profile.evidence.push(EvidenceRequirement::new(
          key("terminal_resolution"),
          EvidenceOutputKind::Report,
          schemas[&key("terminal_facts")].reference().clone(),
          reference("terminal.verifier"),
          reference("verifier.plugin"),
        ));
      }
      match node.outcomes[0].schema.as_str() {
        "code_review" | "validation_classification" => {
          let (report, facts) = if node.outcomes[0].schema.as_str() == "code_review" {
            ("code_review", "code_review_facts")
          } else {
            ("classification_report", "classification_facts")
          };
          profile.result_output = key(report);
          profile.evidence = vec![EvidenceRequirement::new(
            key(facts),
            EvidenceOutputKind::Report,
            schemas[&key("spec_review_facts")].reference().clone(),
            reference("independent.verifier"),
            reference("verifier.plugin"),
          )];
        }
        "protected_test_handoff" => {
          profile.result_output = key("public.coverage");
          profile.result_tool = Some(reference("tests.capture"));
          profile.evidence = vec![EvidenceRequirement::new(
            key("protected_inventory"),
            EvidenceOutputKind::Artifact,
            reference("tests.inventory"),
            reference("tests.capture"),
            profile.plugin.clone(),
          )];
        }
        "protected_validation_report" => {
          profile.result_output = key("validation_report");
          profile.evidence = vec![EvidenceRequirement::new(
            key("validation_facts"),
            EvidenceOutputKind::Report,
            schemas[&key("protected_validation_facts")].reference().clone(),
            profile.tool.clone(),
            profile.plugin.clone(),
          )];
        }
        "spec_manifest" => {
          profile.result_output = key("change-set-manifest.json");
          profile.result_output_kind = EvidenceOutputKind::Artifact;
          profile.result_tool = Some(reference("capture"));
          profile.evidence = vec![
            EvidenceRequirement::new(
              key("change-set.bundle"),
              EvidenceOutputKind::Artifact,
              reference("capture.artifacts"),
              reference("capture"),
              profile.plugin.clone(),
            ),
            EvidenceRequirement::new(
              key("specification_handoff"),
              EvidenceOutputKind::Report,
              schemas[&key("spec_handoff")].reference().clone(),
              profile.tool.clone(),
              profile.plugin.clone(),
            ),
            EvidenceRequirement::new(
              key("specification_facts"),
              EvidenceOutputKind::Report,
              schemas[&key("spec_facts")].reference().clone(),
              reference("independent.verifier"),
              reference("verifier.plugin"),
            ),
          ];
        }
        "spec_review" => {
          profile.result_output = key("specification_review");
          profile.evidence = vec![EvidenceRequirement::new(
            key("specification_review_facts"),
            EvidenceOutputKind::Report,
            schemas[&key("spec_review_facts")].reference().clone(),
            reference("independent.verifier"),
            reference("verifier.plugin"),
          )];
        }
        _ => {}
      }
      requirements.insert(node.key.clone(), profile.evidence.clone());
      result_schemas.insert(node.key.clone(), schemas[&node.outcomes[0].schema].reference().clone());
      profiles.insert(choice.clone(), profile);
    }
  }
  let published = template
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&run, published.definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let limits =
    FlowAdmissionLimits::product_defaults(4, template.execution.budget(), template.execution.permissions().clone())
      .unwrap();
  let policy = PhasePoolPolicy {
    phase: key("development.ready"),
    order: vec![
      PhasePoolOrder::Severity,
      PhasePoolOrder::ProjectPriority,
      PhasePoolOrder::Age,
    ],
    max_wip: 4,
    max_project_wip: 1,
    budget: limits.budget(),
  };
  let pools = BTreeMap::from([(
    policy.phase.clone(),
    FlowPoolSettings {
      policy,
      project_priority: 4,
      capabilities: Default::default(),
      selection: FlowDataMapping::new(FlowValueMapping::Object {
        fields: BTreeMap::from([
          (
            key("severity"),
            FlowValueMapping::Pointer {
              path: format!("{pool_payload_root}/severity"),
            },
          ),
          (
            key("dependencies"),
            FlowValueMapping::Pointer {
              path: format!("{pool_payload_root}/dependencies"),
            },
          ),
        ]),
      })
      .unwrap(),
    },
  )]);
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(published.definition.reference(), vec![published.definition]).unwrap(),
    limits,
    flow,
    cycle,
    octacity_server_factory::FactoryWipLimits::new(8, 16).unwrap(),
  )
  .unwrap()
  .with_pool_settings(pools)
  .unwrap()
  .with_data_schemas(published.schemas)
  .unwrap();
  let execution = Execution {
    clock: std::sync::atomic::AtomicI64::new(30),
    seeder: None,
    output_builder: None,
    builds: Mutex::new(BTreeMap::new()),
    requirements,
    result_schemas,
    reports: BTreeMap::from([
      (key("eligibility"), serde_json::json!({"fit":"fits","duplicate":false})),
      (
        key("classification"),
        serde_json::json!({"work_kind":"defect","size":"small","risk":"low","component":"api","severity":"high","dependencies":[],"recommended_route":"development","reproducibility":"not_reproduced"}),
      ),
      (
        key("defect_research"),
        serde_json::json!({"outcome":"reproduced","summary":"Independent bounded reproduction"}),
      ),
    ]),
    facts: BTreeMap::from([
      (key("eligibility"), serde_json::json!({"fit":"fits","duplicate":false})),
      (
        key("classification"),
        serde_json::json!({"work_kind":"defect","size":"small","risk":"low","component":"api","severity":"high","dependencies":[],"reproduced":false,"already_fixed":false,"verification_only":false}),
      ),
      (key("defect_research"), serde_json::json!({"outcome":"reproduced"})),
    ]),
  };
  (
    PublishedFactoryAdmission {
      work,
      run,
      flow: admitted,
      admitted_at: at(1),
    },
    Arc::new(execution),
  )
}
pub(crate) async fn verify_configured_owner_journey<S: FactoryRunStore + FactoryPhasePoolStore>(
  store: Arc<S>,
  restarted: Arc<S>,
  admission: &PublishedFactoryAdmission,
  execution: Arc<Execution>,
) -> Box<PhasePoolEntry> {
  verify_owner_journey(
    store,
    restarted,
    admission,
    execution,
    "defect_research",
    "defect_research_gate",
  )
  .await
}
async fn verify_owner_journey<S: FactoryRunStore + FactoryPhasePoolStore>(
  store: Arc<S>,
  restarted: Arc<S>,
  admission: &PublishedFactoryAdmission,
  execution: Arc<Execution>,
  research: &str,
  gate: &str,
) -> Box<PhasePoolEntry> {
  let claim = FactoryRunClaimRecord::new(
    admission.run.id(),
    key("flow.worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
      at(10),
      at(900),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: admission.run.id(),
      expected_version: admission.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        admission.run.id(),
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        FactoryDigest::from_bytes([1; 32]),
        key("accepted"),
        at(10),
      ),
    })
    .await
    .unwrap();
  let mut ready = None;
  for time in 31..75 {
    execution.clock.store(time, std::sync::atomic::Ordering::SeqCst);
    let adapter = Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone()));
    let owner = FactoryFlowRunner::new(
      if time % 2 == 0 {
        store.clone()
      } else {
        restarted.clone()
      },
      adapter,
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    match owner
      .reconcile_flow(admission.run.id(), claim.id, at(time))
      .await
      .unwrap()
    {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Ready(entry) => {
        ready = Some(entry);
        break;
      }
      other => panic!("expected configured progress, got {other:?}"),
    }
  }
  let ready = ready.expect("configured queue must be reached");
  assert_eq!(ready.input.node.as_str(), "protected_tests");
  assert_eq!(ready.input.severity, FindingSeverity::High);
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(snapshot.run.state(), FactoryRunState::Running);
  let mut records = snapshot
    .flow
    .data
    .records
    .iter()
    .map(|row| {
      (
        snapshot
          .flow
          .attempts
          .iter()
          .find(|attempt| attempt.id() == row.node_attempt_id())
          .unwrap()
          .number(),
        row.input().node().as_str(),
      )
    })
    .collect::<Vec<_>>();
  records.sort();
  assert_eq!(
    records.into_iter().map(|(_, name)| name).collect::<Vec<_>>(),
    vec![
      "eligibility",
      "eligibility_gate",
      "classification",
      "classification_gate",
      research,
      gate,
      "requirements_needed",
      "accessibility_audit"
    ]
  );
  assert_eq!(snapshot.flow.data.build_intents.len(), 4);
  assert_eq!(snapshot.flow.data.build_executions.len(), 4);
  let accepted = snapshot
    .flow
    .data
    .records
    .iter()
    .find(|record| record.input().node() == &key("requirements_needed"))
    .unwrap();
  let contract = AcceptedWorkContract::freeze(
    &snapshot.work,
    &snapshot.admitted_flow,
    &snapshot.flow.data,
    FlowRuntimeHistory {
      stops: &snapshot.flow.data.stops,
      flow_runs: &snapshot.flow.runs,
      cycles: &snapshot.flow.cycles,
      attempts: &snapshot.flow.attempts,
      completions: &snapshot.flow.completions,
    },
    accepted.node_attempt_id(),
  )
  .unwrap();
  assert_eq!(
    contract.work().artifacts().acceptance(),
    admission.work.artifacts().acceptance()
  );
  assert_eq!(contract.records().len(), 7);
  let author = snapshot
    .flow
    .data
    .build_intents
    .iter()
    .find(|intent| intent.input().node() == &key("accessibility_audit"))
    .unwrap();
  let policy = crate::FactoryProtectedTestPolicy::new(
    ProtectedTestCoverage::new(vec![key("integration"), key("project.custom")], 8, 9000, 7).unwrap(),
    1000,
  )
  .unwrap();
  let plan = crate::freeze_factory_protected_test_plan(author, &contract, &policy).unwrap();
  assert_eq!(plan.accepted_contract(), contract.digest().unwrap());
  assert_eq!(plan.classes(), &[key("integration"), key("project.custom")]);
  assert_eq!(
    crate::freeze_factory_protected_test_plan(author, &contract, &policy).unwrap(),
    plan
  );
  use crate::FactoryProtectedTestPlanSource as _;
  let plans = crate::FactoryStoredProtectedTestPlanSource::new(
    store.clone(),
    author.profile().model_or_tool.clone(),
    policy.clone(),
  );
  assert_eq!(plans.frozen_plan(author).await.unwrap(), plan);
  let recovered = crate::FactoryStoredProtectedTestPlanSource::new(
    restarted.clone(),
    author.profile().model_or_tool.clone(),
    policy.clone(),
  );
  assert_eq!(
    recovered.frozen_plan(author).await.unwrap(),
    plan,
    "plan authority must reconstruct from persisted accepted ancestry after restart"
  );
  let wrong =
    crate::FactoryStoredProtectedTestPlanSource::new(store.clone(), reference("unselected.policy"), policy.clone());
  assert!(wrong.frozen_plan(author).await.is_err());
  let earlier = snapshot
    .flow
    .data
    .build_intents
    .iter()
    .find(|intent| intent.input().node() == &key("classification"))
    .unwrap();
  assert!(
    crate::freeze_factory_protected_test_plan(earlier, &contract, &policy).is_err(),
    "only explicit accepted-contract ancestry can authorize authoring"
  );
  assert!(
    !snapshot
      .flow
      .attempts
      .iter()
      .any(|attempt| attempt.node_key() == &key("protected_tests"))
  );
  let policy = snapshot.admitted_flow.pool_settings()[&key("development.ready")]
    .policy
    .clone();
  let selected = store
    .select_phase_ready(SelectPhasePool {
      policy_digest: policy.digest(),
      request_id: FactoryDigest::from_bytes([99; 32]),
      owner: key("pool.worker"),
      observed_at: at(901),
      expires_at: at(1800),
      capabilities: Default::default(),
      limit: 1,
    })
    .await
    .unwrap();
  assert_eq!(selected.len(), 1);
  assert_eq!(&selected[0].entry, ready.as_ref());
  let selected_claim = selected[0].run_claim();
  let mut terminal = None;
  for time in 902..940 {
    execution.clock.store(time, std::sync::atomic::Ordering::SeqCst);
    let owner = FactoryFlowRunner::new(
      if time % 2 == 0 {
        store.clone()
      } else {
        restarted.clone()
      },
      Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone())),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    match owner
      .reconcile_flow(admission.run.id(), selected_claim.id, at(time))
      .await
      .unwrap()
    {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Resolved(outcome) => {
        terminal = Some(outcome);
        break;
      }
      other => panic!("unexpected selected progress {other:?}"),
    }
  }
  assert_eq!(terminal, Some(key("verified_candidate")));
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(
    snapshot.run.state(),
    FactoryRunState::Completed,
    "the configured root terminal must durably finish its Run"
  );
  let author = snapshot
    .flow
    .data
    .records
    .iter()
    .find(|row| row.input().node() == &key("protected_tests"))
    .unwrap();
  let private = author
    .build_provenance()
    .unwrap()
    .outputs()
    .iter()
    .find(|output| output.output_kind == EvidenceOutputKind::Artifact)
    .unwrap();
  let implementation = snapshot
    .flow
    .data
    .build_intents
    .iter()
    .find(|intent| intent.input().node() == &key("implementation"))
    .unwrap();
  assert_eq!(
    implementation.input().payload().value()["accepted"],
    author.input().payload().value()["accepted"],
    "the control edge forwards the accepted contract, not author outputs"
  );
  let exposed = serde_json::to_string(implementation.input()).unwrap();
  assert!(!exposed.contains(&private.artifact.artifact_id().to_string()));
  assert!(implementation.input().context().unwrap().entries().iter().all(|entry|
    matches!(entry.source(),FactoryContextReference::Artifact(reference) if reference==snapshot.work.artifacts().task())),
    "the implementation's frozen Context cannot grant the private inventory or author files");
  let replay = FactoryFlowRunner::new(
    store.clone(),
    Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone())),
    Arc::new(Inputs),
    crate::factory_node_test_support::clock(),
  );
  assert_eq!(
    replay
      .reconcile_flow(admission.run.id(), selected_claim.id, at(1900))
      .await
      .unwrap(),
    FactoryFlowStep::Resolved(key("verified_candidate"))
  );
  assert_eq!(store.factory_run_snapshot(admission.run.id()).await.unwrap(), snapshot);
  let mut records = snapshot
    .flow
    .data
    .records
    .iter()
    .map(|row| {
      (
        snapshot
          .flow
          .attempts
          .iter()
          .find(|attempt| attempt.id() == row.node_attempt_id())
          .unwrap()
          .number(),
        row.input().node().as_str(),
      )
    })
    .collect::<Vec<_>>();
  records.sort();
  assert_eq!(
    records.into_iter().map(|(_, name)| name).collect::<Vec<_>>(),
    vec![
      "eligibility",
      "eligibility_gate",
      "classification",
      "classification_gate",
      research,
      gate,
      "requirements_needed",
      "accessibility_audit",
      "protected_tests",
      "implementation_entry",
      "implementation",
      "implementation_result_gate",
      "protected_validation",
      "protected_validation_gate",
      "review",
      "code_review_gate",
      "verification",
      "verification_result_gate"
    ]
  );
  assert_eq!(snapshot.flow.data.build_intents.len(), 9);
  ready
}
#[test]
fn configured_intake_research_unknown_step_and_pool_share_one_owner_interface_across_restart() {
  let (admission, execution) = configured_fixture();
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "configured-flow")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  run_ready(verify_configured_owner_journey(
    store.clone(),
    store,
    &admission,
    execution,
  ));
}

#[test]
fn model_small_work_claim_cannot_bypass_independently_verified_large_work() {
  let (admission, execution) = configured_fixture();
  let mut execution = Arc::try_unwrap(execution).ok().unwrap();
  execution.reports.get_mut(&key("classification")).unwrap()["reproducibility"] = serde_json::json!("reproduced");
  execution.facts.get_mut(&key("classification")).unwrap()["reproduced"] = serde_json::json!(true);
  execution.facts.get_mut(&key("classification")).unwrap()["size"] = serde_json::json!("large");
  let execution = Arc::new(execution);
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "untrusted-size")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  run_ready(async {
    let claim = FactoryRunClaimRecord::new(
      admission.run.id(),
      key("flow.worker"),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
        at(10),
        at(900),
      )
      .unwrap(),
    );
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: admission.run.id(),
        expected_version: admission.run.version(),
        record: claim.clone(),
        audit: FactoryAuditFact::new(
          admission.run.id(),
          AuditActorKind::Worker,
          None,
          key("factory.claimed"),
          FactoryDigest::from_bytes([1; 32]),
          key("accepted"),
          at(10),
        ),
      })
      .await
      .unwrap();
    let owner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone())),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    let mut decision = None;
    for time in 31..65 {
      execution.clock.store(time, std::sync::atomic::Ordering::SeqCst);
      match owner
        .reconcile_flow(admission.run.id(), claim.id, at(time))
        .await
        .unwrap()
      {
        FactoryFlowStep::Advanced => {}
        FactoryFlowStep::Resolved(outcome) => {
          panic!("authoring must remain unfinished: {outcome}");
        }
        other => panic!("unexpected readiness: {other:?}"),
      }
      let current = store.factory_run_snapshot(admission.run.id()).await.unwrap();
      if let Some(record) = current
        .flow
        .data
        .records
        .iter()
        .find(|record| record.input().node() == &key("requirements_needed"))
      {
        decision = Some(key(record.metadata().unwrap()["outcome"].as_str().unwrap()));
        if current
          .flow
          .data
          .records
          .iter()
          .any(|row| row.input().node() == &key("author_subject"))
        {
          break;
        }
      }
    }
    assert_eq!(decision, Some(key("specification")));
    let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    let author_input = Inputs
      .prepare(
        &snapshot,
        &FactoryNodeTarget {
          flow_run_id: snapshot.admitted_flow.root_run().id(),
          cycle_id: snapshot.admitted_flow.initial_cycle().id(),
          node: key("specification_author"),
        },
      )
      .await
      .unwrap();
    assert_eq!(
      author_input.payload().value()["work_id"],
      snapshot.work.id().to_string()
    );
    assert_eq!(
      author_input.payload().value()["original_base"],
      snapshot.work.subject().base_revision().as_str()
    );
    assert_eq!(author_input.payload().value()["accepted_input"]["size"], "large");
    assert_eq!(author_input.sources().len(), 1);
    assert!(!snapshot.flow.attempts.iter().any(
      |attempt| attempt.node_key() == &key("accessibility_audit") || attempt.node_key() == &key("protected_tests")
    ));
  });
}

#[async_trait]
pub(crate) trait FactoryTestBuildSeeder: Send + Sync {
  async fn seed(&self, request: &CreateFactoryBuild, accepted: &FactoryBuildAcceptance);
}
pub(crate) fn run_ready<T>(future: impl std::future::Future<Output = T>) -> T {
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  let mut future = std::pin::pin!(future);
  match future.as_mut().poll(&mut context) {
    std::task::Poll::Ready(value) => value,
    std::task::Poll::Pending => panic!("deterministic memory contract yielded"),
  }
}

#[test]
fn a_configured_bounded_cycle_executes_fresh_attempts_and_exhausts_its_declared_route() {
  verify_bounded_cycle(false, false);
}

#[test]
fn configured_nested_calls_freeze_caller_input_and_return_the_exact_child_result_after_restart() {
  run_ready(async {
    let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
    let admission = nested_fixture();
    let audit = store_testing::management_mutation_with_request((), "nested-owner")
      .audit()
      .clone();
    store
      .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    verify_nested_owner(store.clone(), store, &admission).await;
  });
}

pub(crate) fn nested_fixture() -> PublishedFactoryAdmission {
  let work = store_testing::factory_node_journal_contract_fixture().admission.work;
  let schema = FlowDataSchema::new(key("nested.value"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let budget = BudgetLimit::new(16, 10000, 1000, 10000, 10000).unwrap();
  let child_node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("inspect_input"),
    kind: FlowNodeKind::DeterministicGate,
    input_schema: Some(schema.reference().clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      key("observed"),
      FlowOutcomeKind::Success,
      schema.reference().clone(),
    )],
    budget,
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_input_binding(
    FlowInputBinding::new(
      BTreeMap::from([(key("caller"), FlowInputSource::CallerInput { path: String::new() })]),
      FlowDataMapping::new(FlowValueMapping::Pointer { path: "/caller".into() }).unwrap(),
    )
    .unwrap(),
  )
  .unwrap()
  .with_gate(
    FlowGateProgram::new(vec![], key("observed"))
      .unwrap()
      .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap())
      .unwrap()
      .binding()
      .unwrap(),
  )
  .unwrap()
  .with_accepted_work_outcomes(vec![key("observed")])
  .unwrap();
  let child = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema.reference().clone()),
    entry: key("inspect_input"),
    nodes: vec![child_node],
    transitions: vec![FlowTransition::new(
      key("inspect_input"),
      key("observed"),
      FlowTransitionTarget::Terminal(key("returned")),
    )],
    terminals: vec![FlowTerminalDefinition::new(key("returned"), schema.reference().clone())],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, FactoryPermissionSet::deny_all(), 1).unwrap(),
  })
  .unwrap();
  let call = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("configured_call"),
    kind: FlowNodeKind::SubflowCall,
    input_schema: Some(schema.reference().clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      key("returned"),
      FlowOutcomeKind::Success,
      schema.reference().clone(),
    )],
    budget,
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: Some(child.reference()),
  })
  .unwrap()
  .with_input_binding(
    FlowInputBinding::new(
      Default::default(),
      FlowDataMapping::new(FlowValueMapping::Constant {
        value: serde_json::json!(true),
      })
      .unwrap(),
    )
    .unwrap(),
  )
  .unwrap();
  let root = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema.reference().clone()),
    entry: key("configured_call"),
    nodes: vec![call],
    transitions: vec![FlowTransition::new(
      key("configured_call"),
      key("returned"),
      FlowTransitionTarget::Terminal(key("done")),
    )],
    terminals: vec![FlowTerminalDefinition::new(key("done"), schema.reference().clone())],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, FactoryPermissionSet::deny_all(), 1).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&run, root.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(root.reference(), vec![root, child]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, budget, FactoryPermissionSet::deny_all()).unwrap(),
    flow,
    cycle,
    octacity_server_factory::FactoryWipLimits::new(8, 16).unwrap(),
  )
  .unwrap()
  .with_data_schemas(vec![schema])
  .unwrap();
  PublishedFactoryAdmission {
    work,
    run,
    flow: admitted,
    admitted_at: at(1),
  }
}

pub(crate) async fn verify_nested_owner<S: FactoryRunStore + FactoryPhasePoolStore>(
  store: Arc<S>,
  restarted: Arc<S>,
  admission: &PublishedFactoryAdmission,
) {
  let run = &admission.run;
  let mut claim = FactoryRunClaimRecord::new(
    run.id(),
    key("nested.worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([3; 32])),
      at(10),
      at(900),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run.id(),
      expected_version: run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run.id(),
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        FactoryDigest::from_bytes([1; 32]),
        key("accepted"),
        at(10),
      ),
    })
    .await
    .unwrap();
  let mut resolved = None;
  let mut time = 11;
  let mut took_over = false;
  for _ in 0..14 {
    let owner = FactoryFlowRunner::new(
      if time % 2 == 0 {
        store.clone()
      } else {
        restarted.clone()
      },
      Arc::new(FakePending),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    match owner.reconcile_flow(run.id(), claim.id, at(time)).await.unwrap() {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Resolved(outcome) => {
        resolved = Some(outcome);
        break;
      }
      other => panic!("nested flow stalled: {other:?}"),
    }
    let snapshot = restarted
      .factory_run_snapshot(run.id())
      .await
      .unwrap_or_else(|error| panic!("nested snapshot at {time}, takeover {took_over}: {error:?}"));
    if !took_over && snapshot.flow.data.records.len() == 1 {
      time = claim.claim.expires_at().unix_millis() + 1;
      claim = FactoryRunClaimRecord::new(
        run.id(),
        key("nested.successor"),
        FactoryClaim::new(
          FactoryClaimFence::new(FactoryDigest::from_bytes([4; 32])),
          at(time),
          at(time + 900),
        )
        .unwrap(),
      );
      restarted
        .claim_factory_run(ClaimFactoryRun {
          run_id: run.id(),
          expected_version: snapshot.run.version(),
          record: claim.clone(),
          audit: FactoryAuditFact::new(
            run.id(),
            AuditActorKind::Worker,
            None,
            key("factory.claimed"),
            FactoryDigest::from_bytes([1; 32]),
            key("accepted"),
            at(time),
          ),
        })
        .await
        .unwrap();
      took_over = true;
    }
    time += 1;
  }
  assert!(took_over);
  assert_eq!(resolved, Some(key("done")));
  let snapshot = restarted.factory_run_snapshot(run.id()).await.unwrap();
  assert_eq!(snapshot.flow.runs.len(), 2);
  assert_eq!(snapshot.flow.attempts.len(), 2);
  assert_eq!(snapshot.flow.data.records.len(), 2);
  let returned = snapshot
    .flow
    .data
    .records
    .iter()
    .find(|row| row.input().node() == &key("configured_call"))
    .unwrap();
  assert_eq!(returned.payload().value(), &serde_json::json!(true));
  let mut missing_child_proof = snapshot.flow.data.clone();
  missing_child_proof
    .records
    .retain(|row| row.input().node() == &key("configured_call"));
  assert!(
    missing_child_proof
      .validate(
        &snapshot.work,
        &snapshot.admitted_flow,
        FlowRuntimeHistory {
          stops: &snapshot.flow.data.stops,
          flow_runs: &snapshot.flow.runs,
          cycles: &snapshot.flow.cycles,
          attempts: &snapshot.flow.attempts,
          completions: &snapshot.flow.completions,
        }
      )
      .is_err()
  );

  assert_eq!(returned.metadata().unwrap()["outcome"], serde_json::json!("returned"));
  let child_result = snapshot
    .flow
    .data
    .records
    .iter()
    .find(|row| row.input().node() == &key("inspect_input"))
    .unwrap();
  let accepted = AcceptedWorkContract::freeze(
    &snapshot.work,
    &snapshot.admitted_flow,
    &snapshot.flow.data,
    FlowRuntimeHistory {
      stops: &snapshot.flow.data.stops,
      flow_runs: &snapshot.flow.runs,
      cycles: &snapshot.flow.cycles,
      attempts: &snapshot.flow.attempts,
      completions: &snapshot.flow.completions,
    },
    child_result.node_attempt_id(),
  )
  .unwrap();
  assert_eq!(
    serde_json::to_value(accepted).unwrap()["caller_inputs"]
      .as_array()
      .unwrap()
      .len(),
    1
  );
}

#[test]
fn frozen_predecessor_inputs_survive_a_later_different_predecessor_in_a_bounded_cycle() {
  verify_bounded_cycle(true, false);
}

#[test]
fn a_pooled_bounded_repeat_requires_a_fresh_selection_and_reserves_each_execution() {
  verify_bounded_cycle(false, true);
}
pub(crate) fn bounded_fixture(project_predecessor: bool, pooled: bool) -> PublishedFactoryAdmission {
  let work = store_testing::factory_node_journal_contract_fixture().admission.work;
  let schema = FlowDataSchema::new(key("cycle.value"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let budget = BudgetLimit::new(16, 10000, 1000, 10000, 10000).unwrap();
  let program = FlowGateProgram::new(vec![], key("observed"))
    .unwrap()
    .with_output_mapping(
      key("observed"),
      FlowDataMapping::new(FlowValueMapping::Constant {
        value: serde_json::json!(true),
      })
      .unwrap(),
    )
    .unwrap();
  let binding = FlowInputBinding::new(
    Default::default(),
    FlowDataMapping::new(FlowValueMapping::Constant {
      value: serde_json::json!(true),
    })
    .unwrap(),
  )
  .unwrap();
  let projected = FlowInputBinding::new(
    BTreeMap::from([(
      key("previous"),
      FlowInputSource::Predecessor {
        view: FlowRecordView::Payload,
        path: String::new(),
      },
    )]),
    FlowDataMapping::new(FlowValueMapping::Pointer {
      path: "/previous".into(),
    })
    .unwrap(),
  )
  .unwrap();
  let node = |name| {
    let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: key(name),
      kind: FlowNodeKind::DeterministicGate,
      input_schema: Some(schema.reference().clone()),
      outcomes: vec![FlowOutcomeDefinition::new(
        key("observed"),
        FlowOutcomeKind::Success,
        schema.reference().clone(),
      )],
      budget: BudgetLimit::new(1, 1000, 100, 1000, 1000).unwrap(),
      permissions: FactoryPermissionSet::deny_all(),
      required: true,
      subflow: None,
    })
    .unwrap()
    .with_input_binding(if project_predecessor && name != "a" {
      projected.clone()
    } else {
      binding.clone()
    })
    .unwrap()
    .with_gate(program.binding().unwrap())
    .unwrap();
    if pooled && name == "b" {
      node.with_phase_pool(key("repeat.queue"))
    } else {
      node
    }
  };
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema.reference().clone()),
    entry: key("a"),
    nodes: if project_predecessor {
      vec![node("a"), node("b"), node("c")]
    } else {
      vec![node("a"), node("b")]
    },
    transitions: if project_predecessor {
      vec![
        FlowTransition::new(key("a"), key("observed"), FlowTransitionTarget::Node(key("b"))),
        FlowTransition::new(key("b"), key("observed"), FlowTransitionTarget::Node(key("c"))),
        FlowTransition::repeated(
          key("c"),
          key("observed"),
          FlowTransitionTarget::Node(key("b")),
          1,
          key("exhausted"),
        )
        .unwrap(),
      ]
    } else {
      vec![
        FlowTransition::new(key("a"), key("observed"), FlowTransitionTarget::Node(key("b"))),
        FlowTransition::repeated(
          key("b"),
          key("observed"),
          FlowTransitionTarget::Node(key("a")),
          1,
          key("exhausted"),
        )
        .unwrap(),
      ]
    },
    terminals: vec![FlowTerminalDefinition::new(
      key("exhausted"),
      schema.reference().clone(),
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, FactoryPermissionSet::deny_all(), 1).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let mut admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, budget, FactoryPermissionSet::deny_all()).unwrap(),
    flow,
    cycle,
    octacity_server_factory::FactoryWipLimits::new(8, 16).unwrap(),
  )
  .unwrap()
  .with_data_schemas(vec![schema])
  .unwrap();
  let pool_policy = PhasePoolPolicy {
    phase: key("repeat.queue"),
    order: vec![
      PhasePoolOrder::Severity,
      PhasePoolOrder::ProjectPriority,
      PhasePoolOrder::Age,
    ],
    max_wip: 1,
    max_project_wip: 1,
    budget,
  };
  if pooled {
    admitted = admitted
      .with_pool_settings(BTreeMap::from([(
        pool_policy.phase.clone(),
        FlowPoolSettings {
          policy: pool_policy.clone(),
          project_priority: 0,
          capabilities: Default::default(),
          selection: FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::json!({"severity":"low","dependencies":[]}),
          })
          .unwrap(),
        },
      )]))
      .unwrap();
  }
  PublishedFactoryAdmission {
    work,
    run,
    flow: admitted,
    admitted_at: at(1),
  }
}
fn verify_bounded_cycle(project_predecessor: bool, pooled: bool) {
  let admission = bounded_fixture(project_predecessor, pooled);
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "bounded-cycle")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  run_ready(verify_bounded_owner(
    store.clone(),
    store,
    &admission,
    project_predecessor,
    pooled,
  ));
}
pub(crate) async fn verify_bounded_owner<S: FactoryRunStore + FactoryPhasePoolStore>(
  store: Arc<S>,
  restarted: Arc<S>,
  admission: &PublishedFactoryAdmission,
  project_predecessor: bool,
  pooled: bool,
) {
  let run = &admission.run;
  let pool_policy = admission
    .flow
    .pool_settings()
    .values()
    .next()
    .map(|settings| settings.policy.clone());
  let mut claim = FactoryRunClaimRecord::new(
    run.id(),
    key("worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
      at(10),
      at(900),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run.id(),
      expected_version: run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run.id(),
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        FactoryDigest::from_bytes([1; 32]),
        key("accepted"),
        at(10),
      ),
    })
    .await
    .unwrap();
  let mut terminal = None;
  let mut time = 11;
  let mut selections = Vec::new();
  for _ in 0..40 {
    let owner = FactoryFlowRunner::new(
      if time % 2 == 0 {
        store.clone()
      } else {
        restarted.clone()
      },
      Arc::new(FakePending),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    match owner.reconcile_flow(run.id(), claim.id, at(time)).await.unwrap() {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Resolved(outcome) => {
        terminal = Some(outcome);
        break;
      }
      FactoryFlowStep::Ready(entry) if pooled => {
        time = claim.claim.expires_at().unix_millis() + 1;
        let selected = store
          .select_phase_ready(SelectPhasePool {
            policy_digest: pool_policy.as_ref().unwrap().digest(),
            request_id: FactoryDigest::content_sha256(&time.to_be_bytes()),
            owner: key("repeat.worker"),
            observed_at: at(time),
            expires_at: at(time + 30),
            capabilities: Default::default(),
            limit: 1,
          })
          .await
          .unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].entry, *entry);
        assert_eq!(selected[0].entry.input.generation as usize, selections.len());
        let selected_snapshot = restarted.factory_run_snapshot(run.id()).await.unwrap();
        assert!(phase_pool_selection_active(&selected[0], &selected_snapshot, at(time)));
        for previous in &selections {
          assert!(!phase_pool_selection_active(previous, &selected_snapshot, at(time)));
        }

        claim = selected[0].run_claim();
        selections.push(selected[0].clone());
      }
      other => panic!("unexpected cycle state {other:?}"),
    }
    time += 1;
  }
  if pooled {
    assert_eq!(
      selections.len(),
      2,
      "every bounded repeat must acquire its own capacity"
    );
    assert_ne!(selections[0].entry.id, selections[1].entry.id);
  }
  assert_eq!(terminal, Some(key("exhausted")));
  let snapshot = store.factory_run_snapshot(run.id()).await.unwrap();
  assert_eq!(snapshot.flow.attempts.len(), if project_predecessor { 5 } else { 4 });
  assert_eq!(
    snapshot.flow.data.records.len(),
    if project_predecessor { 5 } else { 4 }
  );
  assert_eq!(snapshot.flow.data.inputs.len(), if project_predecessor { 5 } else { 4 });
}

#[test]
fn configured_feature_research_retains_an_actual_proposal_artifact_before_bypass() {
  let (admission, execution) = configured_fixture();
  let mut execution = Arc::try_unwrap(execution).ok().unwrap();
  let report = execution.reports.get_mut(&key("classification")).unwrap();
  report["work_kind"] = serde_json::json!("feature_request");
  report["recommended_route"] = serde_json::json!("research");
  report["reproducibility"] = serde_json::json!("not_applicable");
  execution.facts.get_mut(&key("classification")).unwrap()["work_kind"] = serde_json::json!("feature_request");
  execution.reports.insert(key("feature_research"),serde_json::json!({"outcome":"proposed","problem":"Add an operator-configured check","acceptance_boundary":"Declared contract passes","out_of_scope":"Deployment"}));
  execution.facts.insert(
    key("feature_research"),
    serde_json::json!({"proposal_verified":true,"already_fixed":false}),
  );
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "configured-feature")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  run_ready(async {
    verify_owner_journey(
      store.clone(),
      store.clone(),
      &admission,
      Arc::new(execution),
      "feature_research",
      "feature_research_gate",
    )
    .await;
    let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    let result = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|record| record.input().node() == &key("feature_research"))
      .unwrap();
    let outputs = result.build_provenance().unwrap().outputs();
    assert_eq!(outputs.len(), 4);
    let proposal = outputs.iter().find(|output| output.name == key("proposal")).unwrap();
    assert_eq!(proposal.output_kind, EvidenceOutputKind::Artifact);
    assert_eq!(proposal.producer.tool().identity().as_str(), "proposal.verifier");
    assert!(
      snapshot
        .flow
        .data
        .build_executions
        .iter()
        .any(|execution| execution.build_id() == proposal.producer.build_id()
          && execution.attempt_id() == proposal.producer.attempt_id()
          && execution.jobs().contains(&proposal.producer.job_id()))
    );
  });
}

pub(crate) fn correction_fixture() -> (PublishedFactoryAdmission, Arc<Execution>) {
  configured_fixture_from_template(correction_template(), "")
}
fn correction_template() -> FlowDefinitionTemplate {
  let mut template: FlowDefinitionTemplate = crate::factory_node_test_support::fixtures::supplied_base_flow_template();
  let prototype = template
    .nodes
    .iter()
    .find(|node| node.key == key("implementation"))
    .unwrap()
    .clone();
  let summary = serde_json::json!({"work_kind":"feature_request","size":"small","risk":"low","admission_risk":"low","research_required":false,"research_accepted":false,"component":"api","severity":"high","dependencies":[],"recommended_route":"requirements"});
  template.entry = key("seed_input");
  template.input_schema = key("summary");
  template.nodes = ["seed_input", "write_policy", "inspect_policy"].into_iter().map(|name| {
    let mut node = prototype.clone();
    node.key = key(name);
    node.input_schema = key("summary");
    node.context_binding = None;
    node.capture_deliverables = None;
    node.pool = None;
    node.required = true;
    node.build_profile = Some(key(name));
    node.input_binding = serde_json::from_value(if name == "seed_input" {
      serde_json::json!({"sources":{},"mapping":{"value":{"mapping":"constant","value":summary}}})
    } else {
      serde_json::json!({"sources":{"previous":{"source":"predecessor","view":"payload","path":""}},"mapping":{"value":{"mapping":"pointer","path":"/previous"}}})
    }).unwrap();
    node
  }).collect();
  template.transitions = serde_json::from_value(serde_json::json!([
    {"predecessor":"seed_input","outcome":"observed","target":{"kind":"node","key":"write_policy"},"max_repeats":0},
    {"predecessor":"write_policy","outcome":"observed","target":{"kind":"node","key":"inspect_policy"},"max_repeats":0},
    {"predecessor":"inspect_policy","outcome":"observed","target":{"kind":"node","key":"write_policy"},"max_repeats":1,"exhausted_terminal":"needs_operator","starts_cycle":true}
  ])).unwrap();
  template.terminals = BTreeMap::from([(key("needs_operator"), key("summary"))]);
  template
}

pub(crate) async fn verify_correction_owner<S: FactoryRunStore + FactoryPhasePoolStore>(
  store: Arc<S>,
  restarted: Arc<S>,
  admission: &PublishedFactoryAdmission,
  execution: Arc<Execution>,
) {
  let claim = FactoryRunClaimRecord::new(
    admission.run.id(),
    key("flow.worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
      at(10),
      at(900),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: admission.run.id(),
      expected_version: admission.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        admission.run.id(),
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        FactoryDigest::from_bytes([1; 32]),
        key("accepted"),
        at(10),
      ),
    })
    .await
    .unwrap();
  let mut resolved = false;
  let mut checked_old_authority = false;
  for time in 31..65 {
    let before = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    if before.flow.cycles.len() == 2 && !checked_old_authority {
      let stale = FactoryNodeRunner::new(
        store.clone(),
        Arc::new(FakePending),
        crate::factory_node_test_support::clock(),
      );
      let target = FactoryNodeTarget {
        flow_run_id: admission.flow.root_run().id(),
        cycle_id: admission.flow.initial_cycle().id(),
        node: key("write_policy"),
      };
      assert!(
        stale
          .run_once(admission.run.id(), claim.id, target, None, at(time))
          .await
          .is_err(),
        "old-cycle acceptance cannot authorize execution after correction"
      );
      let stale_input = before
        .flow
        .data
        .inputs
        .iter()
        .find(|input| input.cycle_id() == admission.flow.initial_cycle().id() && input.node() == &key("write_policy"))
        .unwrap()
        .retry()
        .unwrap();
      let mut append = FactoryRunHistoryAppend::default();
      append.flow.data.inputs.push(stale_input);
      assert!(
        store
          .commit_factory_run_transition(append_request(&before, append, at(time)))
          .await
          .is_err(),
        "the store must reject direct append of an input carrying superseded cycle authority"
      );
      checked_old_authority = true;
    }
    execution.clock.store(time, std::sync::atomic::Ordering::SeqCst);
    let owner = FactoryFlowRunner::new(
      if time % 2 == 0 {
        store.clone()
      } else {
        restarted.clone()
      },
      Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone())),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    match owner
      .reconcile_flow(admission.run.id(), claim.id, at(time))
      .await
      .unwrap_or_else(|error| panic!("correction at {time}: {error:?}"))
    {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Resolved(terminal) => {
        assert_eq!(terminal, key("needs_operator"));
        resolved = true;
        break;
      }
      other => panic!("unexpected correction progress {other:?}"),
    }
  }
  assert!(checked_old_authority);
  assert!(resolved, "declared correction must terminate at its bounded exhaustion");
  let snapshot = restarted.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(snapshot.flow.cycles.len(), 2);
  assert_eq!(snapshot.flow.attempts.len(), 5);
  assert_eq!(snapshot.flow.data.records.len(), 5);
  let first = snapshot.flow.cycles.iter().min_by_key(|row| row.number()).unwrap();
  let second = snapshot.flow.cycles.iter().max_by_key(|row| row.number()).unwrap();
  let mut attempts = snapshot.flow.attempts.iter().collect::<Vec<_>>();
  attempts.sort_by_key(|row| row.number());
  assert_eq!(second.predecessor(), Some(first.id()));
  assert_eq!(second.source_attempt_id(), Some(attempts[2].id()));
  assert_eq!(attempts[3].node_key(), &key("write_policy"));
  assert_eq!(attempts[3].workflow_cycle_id(), second.id());
  let input = snapshot
    .flow
    .data
    .inputs
    .iter()
    .find(|input| input.digest().unwrap() == attempts[3].input_digest())
    .unwrap();
  assert_eq!(input.sources().len(), 1);
  assert_eq!(input.sources()[0].node_attempt_id(), attempts[2].id());
  let trigger = snapshot
    .flow
    .data
    .records
    .iter()
    .find(|row| row.node_attempt_id() == attempts[2].id())
    .unwrap();
  assert_eq!(input.payload().value(), trigger.payload().value());
}

#[test]
fn configured_correction_recovers_a_fresh_cycle_and_exact_trigger_without_reusing_old_authority() {
  let (admission, execution) = correction_fixture();
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "correction-flow")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  run_ready(verify_correction_owner(store.clone(), store, &admission, execution));
}

fn append_request(
  snapshot: &FactoryRunSnapshot,
  append: FactoryRunHistoryAppend,
  time: octacity_server_domain::Timestamp,
) -> CommitFactoryRunTransition {
  let claim = snapshot.current_claim.as_ref().unwrap();
  let version = FactoryRunVersion::new(snapshot.run.version().get() + 1).unwrap();
  let usage = snapshot
    .budgets
    .iter()
    .find(|row| row.id == snapshot.current.budget_id)
    .unwrap()
    .usage;
  let budget = FactoryBudgetRecord::new(snapshot.run.id(), version, usage, time);
  let lifecycle_checkpoint = FactoryLifecycleCheckpoint::new(
    snapshot.run.id(),
    version,
    FactoryLifecycleProgress::FlowRunning {
      root: snapshot.admitted_flow.root_run().id(),
    },
    false,
    time,
  );
  let mut current = snapshot.current.clone();
  current.budget_id = budget.id;
  current.lifecycle_checkpoint_id = lifecycle_checkpoint.id;
  CommitFactoryRunTransition {
    run_id: snapshot.run.id(),
    expected_version: snapshot.run.version(),
    claim_id: claim.id,
    owner: claim.owner.clone(),
    fence: claim.claim.fence(),
    committed_at: time,
    next_run: FactoryRun::restore(
      snapshot.run.id(),
      snapshot.run.configuration().clone(),
      &snapshot.work,
      snapshot.run.subject().clone(),
      FactoryRunState::Running,
      version,
    )
    .unwrap(),
    budget,
    lifecycle_checkpoint,
    current,
    append,
    outbox: vec![],
    audit: FactoryAuditFact::new(
      snapshot.run.id(),
      AuditActorKind::Worker,
      None,
      key("factory.node.advance"),
      FactoryDigest::sha256(
        "octacity.factory.node-transition.v1",
        &[&snapshot.run.version().get().to_be_bytes()],
      ),
      key("accepted"),
      time,
    ),
  }
}

#[test]
fn every_ready_fanout_sibling_can_be_published_to_its_configured_pool() {
  struct Fork;
  #[async_trait]
  impl FactoryNodeExecutor for Fork {
    async fn cancel(
      &self,
      _: FactoryNodeExecutionRequest<'_>,
      _: octacity_server_domain::Timestamp,
    ) -> Result<crate::FactoryNodeCancellationStep, ApplicationError> {
      panic!("the fork has no external operation")
    }
    async fn observe(
      &self,
      request: FactoryNodeExecutionRequest<'_>,
    ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
      let definition = request
        .admitted
        .closure()
        .definition(request.input.definition())
        .unwrap();
      let record = FlowNodeRecord::new(
        request.input,
        request.attempt,
        definition,
        FlowRecordObservation {
          outcome: key("observed"),
          payload: request.input.payload().clone(),
          producer: request.attempt.execution().clone(),
          observed_at: request.observed_at,
        },
      )
      .unwrap();
      Ok(FactoryNodeExecutionStep::Completed {
        execution: None,
        record: Box::new(record),
        usage: BudgetUsage::default(),
      })
    }
  }
  let mut template = correction_template();
  template.nodes[0].kind = FlowNodeKind::FanOut;
  template.nodes[0].build_profile = None;
  for node in &mut template.nodes[1..] {
    node.pool = Some(key("development.ready"));
  }
  template.transitions = serde_json::from_value(serde_json::json!([
    {"predecessor":"seed_input","outcome":"observed","target":{"kind":"node","key":"write_policy"},"max_repeats":0},
    {"predecessor":"seed_input","outcome":"observed","target":{"kind":"node","key":"inspect_policy"},"max_repeats":0},
    {"predecessor":"write_policy","outcome":"observed","target":{"kind":"terminal","key":"done"},"max_repeats":0},
    {"predecessor":"inspect_policy","outcome":"observed","target":{"kind":"terminal","key":"done"},"max_repeats":0}
  ]))
  .unwrap();
  template.terminals = BTreeMap::from([(key("done"), key("summary"))]);
  let (admission, _) = configured_fixture_from_template(template, "");
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "fanout-pool")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  run_ready(async {
    let claim = FactoryRunClaimRecord::new(
      admission.run.id(),
      key("pool.owner"),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
        at(10),
        at(100),
      )
      .unwrap(),
    );
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: admission.run.id(),
        expected_version: admission.run.version(),
        record: claim.clone(),
        audit: FactoryAuditFact::new(
          admission.run.id(),
          AuditActorKind::Worker,
          None,
          key("factory.claimed"),
          FactoryDigest::from_bytes([1; 32]),
          key("accepted"),
          at(10),
        ),
      })
      .await
      .unwrap();
    let owner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(Fork),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    for time in 31..34 {
      assert_eq!(
        owner
          .reconcile_flow(admission.run.id(), claim.id, at(time))
          .await
          .unwrap(),
        FactoryFlowStep::Advanced
      );
    }
    let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    assert_eq!(snapshot.flow.completions.len(), 1);
    let mut append = FactoryRunHistoryAppend::default();
    for name in ["write_policy", "inspect_policy"] {
      append.flow.data.inputs.push(
        Inputs
          .prepare(
            &snapshot,
            &FactoryNodeTarget {
              flow_run_id: admission.flow.root_run().id(),
              cycle_id: admission.flow.initial_cycle().id(),
              node: key(name),
            },
          )
          .await
          .unwrap(),
      );
    }
    store
      .commit_factory_run_transition(append_request(&snapshot, append, at(34)))
      .await
      .unwrap();
    let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    let policy = snapshot.admitted_flow.pool_settings()[&key("development.ready")]
      .policy
      .clone();
    for name in ["write_policy", "inspect_policy"] {
      let input = snapshot
        .flow
        .data
        .inputs
        .iter()
        .find(|input| input.node() == &key(name))
        .unwrap();
      let ready = store
        .publish_phase_ready(policy.clone(), configured_phase_pool_input(&snapshot, input).unwrap())
        .await;
      assert!(ready.is_ok(), "declared sibling {name} must be publishable: {ready:?}");
    }
  });
}

#[test]
fn a_correction_cycle_can_publish_its_non_root_entry_to_a_configured_pool() {
  let mut template = correction_template();
  template.transitions[0] = FlowTransition::new(
    key("seed_input"),
    key("observed"),
    FlowTransitionTarget::Node(key("inspect_policy")),
  );
  template
    .nodes
    .iter_mut()
    .find(|node| node.key == key("write_policy"))
    .unwrap()
    .pool = Some(key("development.ready"));
  let (admission, execution) = configured_fixture_from_template(template, "");
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "correction-pool")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  run_ready(async {
    let claim = FactoryRunClaimRecord::new(
      admission.run.id(),
      key("pool.owner"),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
        at(10),
        at(100),
      )
      .unwrap(),
    );
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: admission.run.id(),
        expected_version: admission.run.version(),
        record: claim.clone(),
        audit: FactoryAuditFact::new(
          admission.run.id(),
          AuditActorKind::Worker,
          None,
          key("factory.claimed"),
          FactoryDigest::from_bytes([1; 32]),
          key("accepted"),
          at(10),
        ),
      })
      .await
      .unwrap();
    let owner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone())),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    let mut reached = false;
    for time in 31..48 {
      execution.clock.store(time, std::sync::atomic::Ordering::SeqCst);
      match owner
        .reconcile_flow(admission.run.id(), claim.id, at(time))
        .await
        .unwrap()
      {
        FactoryFlowStep::Advanced => {}
        FactoryFlowStep::Ready(entry) => {
          let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
          assert_eq!(snapshot.flow.cycles.len(), 2);
          let current = snapshot.flow.cycles.iter().max_by_key(|cycle| cycle.number()).unwrap();
          assert_eq!(entry.input.node, key("write_policy"));
          assert_eq!(entry.input.cycle_id, current.id());
          reached = true;
          break;
        }
        other => panic!("unexpected pooled correction {other:?}"),
      }
    }
    assert!(reached);
  });
}
