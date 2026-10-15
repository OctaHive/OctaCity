//! Actual deterministic Evidence, immutable review plan and readonly evaluator Job.
use super::*;
use octacity_server_application::*;
use octacity_server_domain::{ArtifactId, AttemptId, BuildId, ImmutableRevision, JobId, Timestamp};
use octacity_server_factory::*;
fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn reference(value: &str) -> ImmutableReference {
  ImmutableReference::new(key(value), key("v1"), FactoryDigest::content_sha256(value.as_bytes()))
}
fn at(n: i64) -> Timestamp {
  Timestamp::from_unix_millis(n).unwrap()
}
/// Fixture schema samples describe shapes only; none are persisted as completions.
fn shape(value: &Value) -> FlowValueSchema {
  match value {
    Value::Object(fields) => FlowValueSchema::Object {
      fields: fields
        .iter()
        .map(|(k, v)| (key(k), FlowFieldSchema::required(shape(v))))
        .collect(),
    },
    Value::Array(items) => FlowValueSchema::Array {
      item: Box::new(items.first().map(shape).unwrap_or(FlowValueSchema::Boolean)),
      min_items: if items.is_empty() { 0 } else { 1 },
      max_items: if items.is_empty() { 0 } else { 64 },
    },
    Value::String(_) => FlowValueSchema::String {
      min_bytes: 1,
      max_bytes: 4096,
    },
    Value::Number(_) => FlowValueSchema::Integer {
      minimum: 0,
      maximum: i64::MAX,
    },
    Value::Bool(_) => FlowValueSchema::Boolean,
    Value::Null => FlowValueSchema::Enum {
      values: vec![Value::Null],
    },
  }
}
fn schema(name: &str, value: Value) -> FlowDataSchema {
  FlowDataSchema::new(key(name), key("v1"), shape(&value)).unwrap()
}
pub(super) const CRITERIA:&[u8]=b"Compare the actual readonly candidate public contract with the original executable validation evidence. Confirm public/facts.txt contains facts. Private test bodies are not evaluator input.";
pub(super) const CRITERION_OUTPUT: &str = "review-criteria";
pub(super) struct ReviewStages<'a> {
  pub(super) coder: &'a str,
  pub(super) validator: &'a str,
  pub(super) previous: &'a str,
  pub(super) previous_outcome: &'a str,
  pub(super) evidence: &'a str,
  pub(super) planner: &'a str,
  pub(super) reviewer: &'a str,
  pub(super) decision: &'a str,
}
pub(super) struct Reviewer {
  pub(super) verifier: workflow_verification::Verifier,
  evidence: FactoryProtectedValidationEvidenceProfile,
  evidence_identity: ImmutableReference,
  planning: FactoryReviewPlanningProfile,
  planning_identity: ImmutableReference,
  decision: FactoryEvaluationDecisionProfile,
  decision_identity: ImmutableReference,
  schemas: Vec<FlowDataSchema>,
  input: FlowDataSchema,
  assessment: FlowDataSchema,
  decision_input: FlowDataSchema,
  pack: CriterionPack,
}
impl Reviewer {
  pub(super) fn new(
    s: &workflow_author::AuthorScenario<'_>,
    author: &FactoryProtectedTestAuthoringProfile,
    validator: &workflow_validation::Validator,
  ) -> Self {
    let directory = if s.prelude {
      let directory = s.directory.join("independent-reviewer");
      fs::create_dir(&directory).unwrap();
      directory
    } else {
      s.directory.to_owned()
    };
    let mut verifier = workflow_verification::Verifier::new(
      &directory,
      s.release,
      s.resources,
      s.isolated.unwrap(),
      s.backend,
      s.coder.unwrap(),
    );
    let profile = &mut verifier.model.profile;
    let evaluator = reference("independent-code-reviewer");
    profile.reviewer = Some(FactoryCodexReviewerPolicy {
      evaluator: evaluator.clone(),
    });
    profile.instructions=FactoryText::new(json!({"fixture_version":1,"checks":[
      {"path":models::CANDIDATE_FILE,"readable":true,"expected_text":"facts"},
      {"path":"/octacity/protected/test-inventory.json","readable":false},
      {"path":"/octacity/protected/tests/contract.rs","readable":false},
      {"path":"/workspace/output/private-contract.rs","readable":false}],
      "result":{"outcome":"satisfied","summary":"The selected public contract and deterministic evidence satisfy the selected criterion pack.","findings":[]}}).to_string()).unwrap();
    profile.toolchain.model = ImmutableReference::new(
      profile.toolchain.model.identity().clone(),
      profile.toolchain.model.version().clone(),
      FactoryDigest::content_sha256(profile.instructions.as_str().as_bytes()),
    );
    profile.usage_policy = FactoryCodexUsagePolicy::TokenRates {
      model: profile.toolchain.model.clone(),
      input_micro_units_per_million: 1_000_000,
      cached_input_micro_units_per_million: 1_000_000,
      output_micro_units_per_million: 1_000_000,
    };
    let mut grants = serde_json::to_value(&profile.project_permissions).unwrap();
    grants["resources"]["elapsed_millis"] = json!(90_000);
    profile.project_permissions = serde_json::from_value(grants).unwrap();
    profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
    let criterion = s.artifacts["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|a| a["name"] == CRITERION_OUTPUT)
      .unwrap();
    let artifact = FactoryArtifactReference::new(
      criterion["id"].as_str().unwrap().parse().unwrap(),
      FactoryDigest::from_lower_hex(criterion["sha256"].as_str().unwrap()).unwrap(),
      criterion["size_bytes"].as_u64().unwrap(),
    )
    .unwrap();
    assert_eq!(artifact.content_digest(), FactoryDigest::content_sha256(CRITERIA));
    assert_eq!(artifact.encoded_size(), CRITERIA.len() as u64);
    let pack = CriterionPack::new(
      s.resources.project.parse().unwrap(),
      ImmutableReference::new(key("public-contract-criteria"), key("v1"), artifact.content_digest()),
      reference("operator-criterion-text-v1"),
      artifact.clone(),
    )
    .unwrap();
    let declared = validator.entry(author);
    let report: EvidenceRequirement = serde_json::from_value(declared["report"].clone()).unwrap();
    let facts: EvidenceRequirement = serde_json::from_value(declared["facts"].clone()).unwrap();
    let subject = CandidateSubject::new(
      ExactSubject::new(
        s.resources.project.parse().unwrap(),
        s.resources.repository.parse().unwrap(),
        ImmutableRevision::new(s.revision).unwrap(),
      ),
      ImmutableRevision::new(s.revision).unwrap(),
      FactoryDigest::content_sha256(b"schema candidate only"),
    );
    let producer = EvidenceProducer::new(
      BuildId::generate(),
      AttemptId::generate(),
      JobId::generate(),
      report.tool().clone(),
      report.plugin().clone(),
    );
    let item = EvidenceItem::new(EvidenceItemInput {
      subject: subject.clone(),
      kind: report.kind().clone(),
      output_kind: report.output_kind(),
      artifact: artifact.clone(),
      schema: report.schema().clone(),
      producer,
      outcome: DeterministicGateOutcome::Passed,
      published_at: at(1),
      fresh_until: at(600_000),
    });
    let sample_evidence:EvidenceManifest=serde_json::from_value(json!({"id":EvidenceManifestId::generate(),"changeset_id":ChangeSetId::generate(),"subject":subject,"constructed_at":2,"requirements":[report],"items":[item],"inputs":[artifact]})).unwrap();
    let evidence_schema = schema("retained-original-test-evidence", json!(sample_evidence));
    let escalation = schema(
      "review-capability-escalation",
      json!({"reason":"unavailable","branch":"review","required":1,"available":0}),
    );
    // Escalation metadata has optional fields determined by the selected failure.
    let mut escalation_value = serde_json::to_value(&escalation).unwrap();
    for (name, field) in escalation_value["shape"]["fields"].as_object_mut().unwrap() {
      if name != "reason" {
        field["required"] = json!(false);
      }
    }
    let escalation = FlowDataSchema::new(
      key("review-capability-escalation"),
      key("v1"),
      serde_json::from_value(escalation_value["shape"].clone()).unwrap(),
    )
    .unwrap();
    let plan_budget = BudgetLimit::new(1, 120_000, 100, 100, 8 * 1024 * 1024).unwrap();
    let handling = reference("isolated-readonly-review");
    let policy =
      EvaluationPolicy::try_new(vec![pack.reference().clone()], vec![evaluator.clone()], 1, plan_budget).unwrap();
    let capability = ReviewEvaluatorCapability::try_new(
      evaluator.clone(),
      vec![ReviewPurpose::CodeReview],
      vec![handling.clone()],
      plan_budget,
    )
    .unwrap();
    let ReviewPlanPreparation::Ready(sample_plan) = prepare_evaluation_plan(
      EvaluationPlanDefinition {
        id: EvaluationPlanId::generate(),
        purpose: ReviewPurpose::CodeReview,
        subject: subject.clone(),
        criterion_packs: vec![pack.clone()],
        branches: vec![ReviewBranch::new(key("independent"), evaluator.clone(), true)],
        budget: plan_budget,
        created_at: at(3),
        deadline: at(120_003),
        data_handling: handling.clone(),
      },
      &sample_evidence,
      &policy,
      std::slice::from_ref(&capability),
    )
    .unwrap() else {
      panic!("installed reviewer schema contract")
    };
    let plan_schema = schema("frozen-independent-review-plan", json!(sample_plan));
    let assessment_sample = Assessment::new(
      AssessmentId::generate(),
      &sample_plan,
      AssessmentInput {
        subject: subject.clone(),
        evaluator: evaluator.clone(),
        outcome: AssessmentOutcome::Satisfied,
        summary: BoundedSummary::new(
          FactoryTaskSubject::Candidate(subject.clone()),
          FactorySafeText::new("Selected criteria satisfied.").unwrap(),
          FactoryDigest::content_sha256(b"schema summary only"),
        ),
        findings: vec![],
        model: profile.toolchain.model.clone(),
        prompt_digest: FactoryDigest::content_sha256(b"schema prompt only"),
        result: artifact.clone(),
        provenance: FactoryArtifactReference::new(
          ArtifactId::generate(),
          artifact.content_digest(),
          artifact.encoded_size(),
        )
        .unwrap(),
      },
    )
    .unwrap();
    let assessment = schema("qualified-code-assessment", json!(assessment_sample));
    let input = schema(
      "explicit-qualified-review-input",
      json!({"plan":sample_plan,"evidence":sample_evidence,"evaluator":evaluator,"candidate_revision":s.revision,"candidate_bundle":artifact,"candidate_manifest":artifact,"criterion":artifact}),
    );
    let decision_input = schema(
      "explicit-completed-review-join",
      json!({"evidence":sample_evidence,"plan":sample_plan,"assessments":[assessment_sample],"signal_receipts":[]}),
    );
    let decision_policy = DecisionPolicy::try_new(
      DecisionPolicyVersion::INITIAL,
      DecisionPolicyDefinition {
        required_evidence: vec![report.kind().clone()],
        required_evaluators: vec![evaluator.identity().clone()],
        quorum: 1,
        severity_threshold: FindingSeverity::High,
        indeterminate_policy: IndeterminatePolicy::RequiredOnly,
        failure_outcome: DecisionOutcome::Rework,
      },
    )
    .unwrap();
    let decision_sample:Decision=serde_json::from_value(json!({"id":DecisionId::generate(),"plan_id":sample_plan.id(),"subject":subject,"outcome":"accept","assessment_ids":[assessment_sample.id()],"signal_receipt_ids":[],"reasons":["PolicySatisfied"],"input_digest":FactoryDigest::content_sha256(b"schema join only"),"policy_version":decision_policy.version(),"policy_digest":decision_policy.digest()})).unwrap();
    let decision_schema = schema("deterministic-review-decision", json!(decision_sample));
    let evidence = FactoryProtectedValidationEvidenceProfile {
      action: key("construct"),
      ready: FlowOutcomeDefinition::new(
        key("evidenced"),
        FlowOutcomeKind::Success,
        evidence_schema.reference().clone(),
      ),
      report,
      facts,
    };
    let evidence_identity = ImmutableReference::new(
      key("installed-validation-evidence"),
      key("v1"),
      evidence.digest().unwrap(),
    );
    let planning = FactoryReviewPlanningProfile {
      action: key("freeze"),
      ready: FlowOutcomeDefinition::new(
        key("planned"),
        FlowOutcomeKind::Success,
        plan_schema.reference().clone(),
      ),
      escalation: FlowOutcomeDefinition::new(
        key("escalated"),
        FlowOutcomeKind::Failure,
        escalation.reference().clone(),
      ),
      purpose: ReviewPurpose::CodeReview,
      criterion_packs: vec![pack.clone()],
      branches: vec![ReviewBranch::new(key("independent"), evaluator, true)],
      policy,
      data_handling: handling,
      capabilities: vec![capability],
    };
    let planning_identity = ImmutableReference::new(
      key("installed-independent-planner"),
      key("v1"),
      planning.digest().unwrap(),
    );
    let decision = FactoryEvaluationDecisionProfile {
      action: key("decide"),
      policy: decision_policy,
      outcomes: DecisionOutcome::VALUES
        .iter()
        .map(|disposition| {
          (
            *disposition,
            FlowOutcomeDefinition::new(
              key(&format!("{disposition:?}").to_ascii_lowercase()),
              if *disposition == DecisionOutcome::Accept {
                FlowOutcomeKind::Success
              } else {
                FlowOutcomeKind::Failure
              },
              decision_schema.reference().clone(),
            ),
          )
        })
        .collect(),
    };
    let decision_identity = ImmutableReference::new(
      key("installed-deterministic-decision"),
      key("v1"),
      decision.digest().unwrap(),
    );
    Self {
      verifier,
      evidence,
      evidence_identity,
      planning,
      planning_identity,
      decision,
      decision_identity,
      schemas: vec![
        evidence_schema,
        plan_schema,
        escalation,
        assessment.clone(),
        input.clone(),
        decision_input.clone(),
        decision_schema,
      ],
      input,
      assessment,
      decision_input,
      pack,
    }
  }
  pub(super) fn install(
    &self,
    catalog: &mut Value,
    ceiling: &mut FactoryPermissionSet,
    author: &FactoryProtectedTestAuthoringProfile,
  ) {
    let mut entry = self.verifier.entry();
    entry["reviewer"] = json!(self.verifier.model.profile.reviewer);
    catalog["codex"].as_array_mut().unwrap().push(entry);
    catalog["protected_validation_evidence"] = json!([{"identity":self.evidence_identity,"profile":self.evidence,"author_profile":author.command().identity,"policy":author.policy(),"inventory":author.inventory(),"capture_tool":self.verifier.model.profile.candidate_capture_tool}]);
    catalog["review_planning"] = json!([{"identity":self.planning_identity,"profile":self.planning}]);
    let rules = json!({"version":self.decision.policy.version(),"required_evidence":[self.evidence.report.kind()],"required_evaluators":[self.verifier.model.profile.reviewer.as_ref().unwrap().evaluator.identity()],"quorum":1,"severity_threshold":"high","indeterminate_policy":"required_only","failure_outcome":"rework"});
    catalog["evaluation_decisions"] = json!([{"identity":self.decision_identity,"profile":{"action":self.decision.action,"policy":rules,"outcomes":self.decision.outcomes.iter().map(|(disposition,outcome)|json!({"disposition":disposition,"outcome":outcome})).collect::<Vec<_>>()}}]);
    let p = &self.verifier.model.profile;
    catalog["projects"][0]["build_configurations"]
      .as_array_mut()
      .unwrap()
      .push(json!({"alias":"reviewer-build","reference":p.build_configuration}));
    let refs = catalog["projects"][0]["references"].as_array_mut().unwrap();
    for (alias, reference) in [
      ("reviewer-executable", &p.toolchain.executable),
      ("reviewer-model", &p.toolchain.model),
      ("review-evidence", &self.evidence_identity),
      ("review-planner", &self.planning_identity),
      ("review-decision", &self.decision_identity),
    ] {
      refs.push(json!({"kind":"node_capability","alias":alias,"reference":reference}));
    }
    let credential = json!(p.credentials.profile(FactoryCredentialPurpose::Evaluator));
    if !refs
      .iter()
      .any(|entry| entry["kind"] == "secret_profile" && entry["alias"] == credential)
    {
      refs.push(json!({"kind":"secret_profile","alias":credential,"reference":reference("installed.independent-review-credential")}));
    }
    let mut grants = serde_json::to_value(&*ceiling).unwrap();
    let extra = serde_json::to_value(&p.project_permissions).unwrap();
    for category in ["executables", "commands", "secret_profiles"] {
      let values = grants[category].as_array_mut().unwrap();
      for value in extra[category].as_array().unwrap() {
        if !values.contains(value) {
          values.push(value.clone());
        }
      }
    }
    grants["plugins"].as_array_mut().unwrap().extend([
      json!(self.evidence_identity),
      json!(self.planning_identity),
      json!(self.decision_identity),
    ]);
    *ceiling = serde_json::from_value(grants).unwrap();
  }
  pub(super) fn extend(&self, draft: &mut Value, stages: ReviewStages<'_>) {
    let ReviewStages {
      coder,
      validator,
      previous,
      previous_outcome,
      evidence,
      planner,
      reviewer,
      decision,
    } = stages;
    let p = &self.verifier.model.profile;
    let metadata_budget = BudgetLimit::new(1, 120_000, 0, 0, 1024 * 1024).unwrap();
    let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
      plugins: vec![
        self.evidence_identity.clone(),
        self.planning_identity.clone(),
        self.decision_identity.clone(),
      ],
      ..Default::default()
    })
    .unwrap();
    let evidence_schema = self
      .schemas
      .iter()
      .find(|s| s.reference() == self.evidence.ready.schema())
      .unwrap();
    let mut collect = workflow::node(
      evidence,
      "trusted_action",
      evidence_schema,
      metadata_budget,
      &permissions,
      "evidenced",
    );
    collect["action"] =
      json!(FlowActionBinding::new(self.evidence_identity.clone(), self.evidence.action.clone(), json!({})).unwrap());
    let refs = schema(
      "selected-original-validation-publications",
      json!({"report":self.pack.artifact(),"facts":self.pack.artifact()}),
    );
    collect["input_schema"] = json!(refs.reference());
    collect["input_binding"] = json!({"sources":{"validation":{"source":"record","node":validator,"outcome":"validated","view":"metadata","path":""}},"mapping":{"value":{"mapping":"object","fields":{"report":{"mapping":"pointer","path":format!("/validation/build/outputs/{}/artifact",self.evidence.report.kind())},"facts":{"mapping":"pointer","path":format!("/validation/build/outputs/{}/artifact",self.evidence.facts.kind())}}}}});
    let plan_schema = self
      .schemas
      .iter()
      .find(|s| s.reference() == self.planning.ready.schema())
      .unwrap();
    let mut freeze = workflow::node(
      planner,
      "trusted_action",
      plan_schema,
      self.planning.policy.budget(),
      &permissions,
      "planned",
    );
    freeze["input_schema"] = json!(evidence_schema.reference());
    freeze["action"] =
      json!(FlowActionBinding::new(self.planning_identity.clone(), self.planning.action.clone(), json!({})).unwrap());
    freeze["outcomes"]
      .as_array_mut()
      .unwrap()
      .push(json!(self.planning.escalation));
    freeze["input_binding"] = json!({"sources":{"evidence":{"source":"predecessor","view":"payload","path":""}},"mapping":{"value":{"mapping":"pointer","path":"/evidence"}}});
    let mut evaluate = workflow::node(
      reviewer,
      "reasoning",
      &self.assessment,
      BudgetLimit::new(1, 90_000, 100, 100, 8 * 1024 * 1024).unwrap(),
      &p.project_permissions,
      "assessed",
    );
    evaluate["input_schema"] = json!(self.input.reference());
    evaluate["input_binding"] = json!({"sources":{
      "plan":{"source":"predecessor","view":"payload","path":""},
      "evidence":{"source":"record","node":evidence,"outcome":"evidenced","view":"payload","path":""},
      "candidate":{"source":"record","node":coder,"outcome":"implemented","view":"metadata","path":format!("/build/outputs/{}/data/payload/value",models::CANDIDATE_FACTS)},
      "bundle":{"source":"record","node":coder,"outcome":"implemented","view":"metadata","path":format!("/build/outputs/{}/artifact",octacity_protocol::CHANGE_SET_BUNDLE_OUTPUT)},
      "manifest":{"source":"record","node":coder,"outcome":"implemented","view":"metadata","path":format!("/build/outputs/{}/artifact",octacity_protocol::CHANGE_SET_MANIFEST_OUTPUT)}},
      "mapping":{"value":{"mapping":"object","fields":{"plan":{"mapping":"pointer","path":"/plan"},"evidence":{"mapping":"pointer","path":"/evidence"},"evaluator":{"mapping":"constant","value":p.reviewer.as_ref().unwrap().evaluator},"candidate_revision":{"mapping":"pointer","path":"/candidate/candidate_revision"},"candidate_bundle":{"mapping":"pointer","path":"/bundle"},"candidate_manifest":{"mapping":"pointer","path":"/manifest"},"criterion":{"mapping":"constant","value":self.pack.artifact()}}}}});
    evaluate["context_binding"] = json!({"candidate_revision":{"value":{"mapping":"pointer","path":"/candidate_revision"}},"entries":{
      "candidate_bundle":{"source_kind":"evidence","source":{"value":{"mapping":"object","fields":{"kind":{"mapping":"constant","value":"artifact"},"value":{"mapping":"pointer","path":"/candidate_bundle"}}}}},
      "candidate_manifest":{"source_kind":"evidence","source":{"value":{"mapping":"object","fields":{"kind":{"mapping":"constant","value":"artifact"},"value":{"mapping":"pointer","path":"/candidate_manifest"}}}}},
      "criterion":{"source_kind":"policy","source":{"value":{"mapping":"object","fields":{"kind":{"mapping":"constant","value":"artifact"},"value":{"mapping":"pointer","path":"/criterion"}}}}}
    }});
    evaluate["build"] = json!(FlowBuildBinding {
      tool: p.toolchain.executable.clone(),
      plugin: p.toolchain.plugin.clone(),
      result_tool: None,
      capture_write_paths: None,
      capture_deliverables: None,
      model_or_tool: p.toolchain.model.clone(),
      task_digest: p.task_digest(),
      build_configuration: p.build_configuration.clone(),
      result_output: key("codex-run-result"),
      result_output_kind: EvidenceOutputKind::Report,
      result_outcome: key("assessed"),
      evidence: vec![]
    });
    let decision_schema = self.schemas.last().unwrap();
    let mut decide = workflow::node(
      decision,
      "trusted_action",
      decision_schema,
      metadata_budget,
      &permissions,
      "accept",
    );
    decide["outcomes"] = json!(self.decision.outcomes.iter().map(|(_, o)| o).collect::<Vec<_>>());
    decide["action"] =
      json!(FlowActionBinding::new(self.decision_identity.clone(), self.decision.action.clone(), json!({})).unwrap());
    decide["input_schema"] = json!(self.decision_input.reference());
    decide["input_binding"] = json!({"sources":{"assessment":{"source":"predecessor","view":"payload","path":""},"evidence":{"source":"record","node":evidence,"outcome":"evidenced","view":"payload","path":""},"plan":{"source":"record","node":planner,"outcome":"planned","view":"payload","path":""}},"mapping":{"value":{"mapping":"object","fields":{"evidence":{"mapping":"pointer","path":"/evidence"},"plan":{"mapping":"pointer","path":"/plan"},"assessments":{"mapping":"array","items":[{"mapping":"pointer","path":"/assessment"}]},"signal_receipts":{"mapping":"constant","value":[]}}}}});
    draft["root"]["nodes"]
      .as_array_mut()
      .unwrap()
      .extend([collect, freeze, evaluate, decide]);
    let transitions = draft["root"]["transitions"].as_array_mut().unwrap();
    *transitions
      .iter_mut()
      .find(|t| t["predecessor"] == previous && t["outcome"] == previous_outcome)
      .unwrap() = workflow::edge(previous, previous_outcome, "node", evidence);
    transitions.extend([
      workflow::edge(evidence, "evidenced", "node", planner),
      workflow::edge(planner, "planned", "node", reviewer),
      workflow::edge(reviewer, "assessed", "node", decision),
    ]);
    for (_, outcome) in &self.decision.outcomes {
      transitions.push(workflow::edge(decision, outcome.key().as_str(), "terminal", "finished"));
    }
    transitions.push(workflow::edge(planner, "escalated", "terminal", "review-unavailable"));
    draft["root"]["terminals"][0]["schema"] = json!(decision_schema.reference());
    draft["root"]["terminals"]
      .as_array_mut()
      .unwrap()
      .push(json!({"key":"review-unavailable","schema":self.planning.escalation.schema()}));
    draft["data_schemas"]
      .as_array_mut()
      .unwrap()
      .extend(self.schemas.iter().map(|s| json!(s)).chain([json!(refs)]));
  }
  #[expect(
    clippy::too_many_arguments,
    reason = "fixture verifies independently owned run, records and placement"
  )]
  pub(super) async fn verify(
    &self,
    client: &Client,
    origin: &str,
    rows: &[(Vec<u8>, Vec<u8>)],
    nodes: [&str; 4],
    coder: &str,
    isolated: &workflow_verification::VerifierResources,
    original_pool: &str,
  ) {
    let records = rows
      .iter()
      .map(|(_, bytes)| serde_json::from_slice::<Value>(bytes).unwrap())
      .collect::<Vec<_>>();
    let selected = |node: &str| records.iter().find(|r| r["input"]["node"] == node).unwrap();
    let evidence: EvidenceManifest = serde_json::from_value(selected(nodes[0])["payload"]["value"].clone()).unwrap();
    let plan: EvaluationPlan = serde_json::from_value(selected(nodes[1])["payload"]["value"].clone()).unwrap();
    let assessment: Assessment = serde_json::from_value(selected(nodes[2])["payload"]["value"].clone()).unwrap();
    let decision: Decision = serde_json::from_value(selected(nodes[3])["payload"]["value"].clone()).unwrap();
    assessment.validate_bindings(&plan, &evidence).unwrap();
    assert_eq!(assessment.outcome(), AssessmentOutcome::Satisfied);
    assert_eq!(decision.outcome(), DecisionOutcome::Accept);
    assert_eq!(decision.assessment_ids(), [assessment.id()]);
    assert_eq!(decision.plan_id(), plan.id());
    assert_eq!(decision.policy_digest(), self.decision.policy.digest());
    assert_eq!(
      selected(nodes[0])["discovery"],
      json!(FlowDiscoveryFacts::evidence(&evidence).unwrap())
    );
    assert_eq!(
      selected(nodes[1])["discovery"],
      json!(FlowDiscoveryFacts::review_plan(&plan).unwrap())
    );
    assert_eq!(
      selected(nodes[2])["discovery"],
      json!(FlowDiscoveryFacts::assessment(&assessment).unwrap())
    );
    assert_eq!(
      selected(nodes[3])["discovery"],
      json!(FlowDiscoveryFacts::decision(&decision))
    );
    let mut agents = vec![];
    for (node, pool) in [(coder, original_pool), (nodes[2], isolated.pool.as_str())] {
      let producer = &selected(node)["build"]["outputs"][0]["producer"];
      let attempt = get_json(
        client,
        format!("{origin}/api/v1/attempts/{}", string(producer, "attempt_id")),
      )
      .await;
      assert_eq!(attempt["jobs"][0]["state"], "succeeded");
      assert_eq!(attempt["jobs"][0]["assignment"]["selected_pool_id"], pool);
      agents.push(attempt["jobs"][0]["assignment"]["assigned_agent_id"].clone());
    }
    assert_ne!(agents[0], agents[1]);
    let bytes = workflow_author::download_artifact(
      client,
      origin,
      &assessment.provenance().result().artifact_id().to_string(),
    )
    .await;
    let model: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(model["structured_result"]["outcome"], "satisfied");
    assert!(model["structured_result"].get("provenance").is_none());
    for record in [selected(nodes[1]), selected(nodes[2]), selected(nodes[3])] {
      let serialized = record.to_string();
      for canary in [
        "isolated-verifier-credential-canary",
        "finite-coder-private-canary",
        "\"expected\":\"facts\"",
        "PRIVATE_EXPECTATION",
      ] {
        assert!(!serialized.contains(canary));
      }
    }
  }
}
