//! Owner/store contracts with qualified Build-output and protected-publication ports.
use crate::factory_flow_tests::{Execution, FactoryTestOutputBuilder, Inputs, configured_fixture_with_work};
use crate::factory_node_test_support::{
  Builds, document_kind,
  fixtures::{key, reference},
};
use crate::{
  ApplicationError, CreateFactoryBuild, FactoryCandidatePublication, FactoryCandidatePublicationAction,
  FactoryCandidatePublicationRequest, FactoryCandidatePublisher, FactoryControlHandlers, FactoryFlowActionAdapter,
  FactoryFlowRunner, FactoryFlowStep, FactoryNodeBuildAdapter, FactoryNodeBuildExecutionFacts, FactoryNodeBuildOutputs,
  FactoryNodeExecutorRegistry, ManagementAuthorizationGrant, ManagementCommandUseCase, ManagementRequestContext,
  ManagementRequestId, ManagementVisibility, ResolveFactoryNodeCommand,
};
use async_trait::async_trait;
use octacity_server_factory::*;
use octacity_server_store::testing;
use octacity_server_store::*;
use serde_json::{Value, json};
use std::{
  collections::BTreeMap,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
  },
};
fn at(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}

pub(crate) struct QualifiedOutputs {
  pub(crate) feedback: Option<FactoryKey>,
  pub(crate) validation: Option<Arc<ValidationCase>>,
}
impl FactoryTestOutputBuilder for QualifiedOutputs {
  fn outputs(
    &self,
    request: &CreateFactoryBuild,
    builds: &Builds,
    schema: &ImmutableReference,
    requirements: &[EvidenceRequirement],
  ) -> Option<FactoryNodeBuildOutputs> {
    let c = request.causality.as_ref();
    let p = &c.profile;
    let wire: Value = serde_json::from_slice(&c.input).unwrap();
    let input = &wire["payload"]["value"];
    let mut extra = BTreeMap::new();
    let report = if let Some(case) = self
      .validation
      .as_ref()
      .filter(|_| schema.identity().as_str() == "factory.base.protected_validation_report")
    {
      let mut candidates = case.candidates.lock().unwrap();
      if !candidates.contains(&input["candidate_revision"]) {
        candidates.push(input["candidate_revision"].clone());
      }
      let outcome = if case.exhaust || candidates.len() == 1 {
        "failed"
      } else {
        "passed"
      };
      json!({"changeset_id":ChangeSetId::generate(),"candidate":c.context.subject().candidate().unwrap(),"tests":input["protected_inventory"],
        "input_digest":c.input_digest,"classes":[{"class":"project.custom","outcome":outcome}]})
    } else if let Some(case) = self
      .validation
      .as_ref()
      .filter(|_| schema.identity().as_str() == "factory.base.validation_classification")
    {
      json!({"candidate_revision":input["request"]["candidate_revision"],"candidate_digest":input["request"]["candidate_digest"],
        "validation_digest":input["request"]["validation_digest"],"input_digest":c.input_digest,"classification":case.classification,"remediation":"Preserve the public contract"})
    } else if p.result_output == key("change-set-manifest.json") {
      let bundle = json!({"fixture":"qualified immutable candidate bytes"});
      let requirement = requirements
        .iter()
        .find(|r| r.kind() == &key("change-set.bundle"))
        .unwrap();
      let mut doc = document_kind(
        builds,
        requirement.kind(),
        requirement.schema(),
        serde_json::to_vec(&bundle).unwrap(),
        requirement.tool().clone(),
        requirement.output_kind(),
      );
      doc.plugin = requirement.plugin().clone();
      let size = doc.bytes.len();
      let digest = FactoryDigest::content_sha256(&doc.bytes);
      extra.insert(key("change-set.bundle"), doc);
      let predecessor = c
        .context
        .subject()
        .candidate()
        .map_or(&request.immutable_revision, |s| s.candidate_revision());
      let candidate = format!("candidate-{}", c.node_attempt_id);
      let status = if c.context.subject().candidate().is_some() {
        "M"
      } else {
        "A"
      };
      let manifest = json!({"format_version":1,"base_revision":predecessor,"candidate_revision":candidate,"node_attempt_id":c.node_attempt_id,
        "capture_tool":{"identity":p.result_tool().identity(),"version":p.result_tool().version(),"sha256":p.result_tool().digest()},
        "changed_paths":[{"status":status,"path":"openspec/specs/api/spec.md","old_mode":"100644","new_mode":"100644"},{"status":status,"path":"openspec/changes/api/proposal.md","old_mode":"100644","new_mode":"100644"}],
        "bundle":{"name":"change-set.bundle","size_bytes":size,"sha256":digest}});
      let handoff = json!({"work_id":wire["work_id"],"original_base":request.immutable_revision,"predecessor_revision":predecessor,"candidate_revision":candidate,"input_digest":c.input_digest,
        "work_kind":input["accepted_input"]["work_kind"],"contract_path":"openspec/specs/api/spec.md","change_path":"openspec/changes/api/proposal.md",
        "acceptance_criteria":["Specified behavior preserves all original acceptance checks"],"regression_criteria":if input["accepted_input"]["work_kind"]=="defect" {json!(["The defect has a reproducible regression check"])}else{json!([])},"declared_requirements":input["declared_requirements"]});
      let requirement = requirements
        .iter()
        .find(|r| r.kind() == &key("specification_handoff"))
        .unwrap();
      let bytes = serde_json::to_vec(&handoff).unwrap();
      let handoff_digest = FactoryDigest::content_sha256(&bytes);
      let mut doc = document_kind(
        builds,
        requirement.kind(),
        requirement.schema(),
        bytes,
        requirement.tool().clone(),
        requirement.output_kind(),
      );
      doc.plugin = requirement.plugin().clone();
      extra.insert(requirement.kind().clone(), doc);
      let facts = json!({"accepted":true,"contract_exists":true,"requirements_preserved":true,"input_digest":c.input_digest,"handoff_digest":handoff_digest,"fresh_until":10000});
      let requirement = requirements
        .iter()
        .find(|r| r.kind() == &key("specification_facts"))
        .unwrap();
      let mut doc = document_kind(
        builds,
        requirement.kind(),
        requirement.schema(),
        serde_json::to_vec(&facts).unwrap(),
        requirement.tool().clone(),
        requirement.output_kind(),
      );
      doc.plugin = requirement.plugin().clone();
      extra.insert(requirement.kind().clone(), doc);
      manifest
    } else if p.result_output == key("specification_review") {
      let mut review = input["request"].clone();
      review["input_digest"] = json!(c.input_digest);
      review["quality"] = json!({"complete":"satisfied","consistent":"satisfied","feasible":"satisfied","testable":"satisfied","in_scope":"satisfied"});
      review["findings"] = json!([]);
      review
    } else if requirements.iter().any(|r| r.kind() == &key("execution_facts")) {
      let input = input.get("accepted").unwrap_or(input);
      let mut result = input.clone();
      if self.feedback.as_ref() == Some(&p.node) && c.cycle_id.as_uuid() == c.flow_run_id.as_uuid() {
        result["requirements_defect"] = json!({"reason":"Acceptance omitted a required contract case","candidate_revision":input["specification"]["manifest"]["candidate_revision"],"evidence":[input["specification"]["artifacts"]["manifest"].clone()]});
      }
      result
    } else {
      return None;
    };
    let mut primary = document_kind(
      builds,
      &p.result_output,
      schema,
      serde_json::to_vec(&report).unwrap(),
      p.result_tool().clone(),
      p.result_output_kind,
    );
    primary.plugin = p.plugin.clone();
    let report_digest = FactoryDigest::content_sha256(&primary.bytes);
    let mut documents = vec![primary];
    for requirement in requirements {
      let doc = if let Some(doc) = extra.remove(requirement.kind()) {
        doc
      } else {
        let facts = if let Some(value) =
          crate::factory_flow_tests::fixture_capture_or_validation_value(request, requirement, report_digest)
        {
          value
        } else if requirement.kind() == &key("specification_review_facts") {
          json!({"accepted":true,"input_digest":c.input_digest,"report_digest":report_digest,"fresh_until":500})
        } else {
          json!({"outcome":if report.get("requirements_defect").is_some(){"requirements_defect"}else{"observed"},"candidate_revision":if p.node==key("implementation") || report.get("specification").is_none() {json!(c.context.subject().candidate().map_or(&request.immutable_revision,|candidate|candidate.candidate_revision()))} else {report["specification"]["manifest"]["candidate_revision"].clone()},"input_digest":c.input_digest,"report_digest":report_digest,"fresh_until":10000,"requirements_evidence":report["requirements_defect"].get("evidence").cloned().unwrap_or_else(||json!([]))})
        };
        let mut doc = document_kind(
          builds,
          requirement.kind(),
          requirement.schema(),
          serde_json::to_vec(&facts).unwrap(),
          requirement.tool().clone(),
          requirement.output_kind(),
        );
        doc.plugin = requirement.plugin().clone();
        doc
      };
      documents.push(doc);
    }
    let template: FlowDefinitionTemplate = crate::factory_node_test_support::fixtures::supplied_base_flow_template();
    let catalogue = template.data_schemas().unwrap();
    for doc in &documents {
      if let Some(contract) = catalogue.values().find(|contract| contract.reference() == &doc.schema) {
        FlowPayload::new(contract, serde_json::from_slice(&doc.bytes).unwrap()).unwrap_or_else(|error| {
          panic!(
            "qualified fixture {} {}: {error:?}",
            p.node,
            doc.record.identity().logical_name
          )
        });
      }
    }
    let output_bytes = documents.iter().map(|doc| doc.bytes.len() as u64).sum();
    Some(FactoryNodeBuildOutputs {
      execution: FactoryNodeBuildExecutionFacts {
        profile: p.clone(),
        input_digest: c.input_digest,
        permissions: request.effective_permissions.clone(),
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
      fresh_until: at(if p.result_output == key("specification_review") {
        500
      } else {
        10000
      }),
    })
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublicationCase {
  Normal,
  LostResponse,
  ChangedAncestry,
  StaleReview,
  HumanDeclined,
}
pub(crate) struct Publisher {
  pub(crate) behavior: PublicationCase,
  pub(crate) lost: AtomicBool,
  pub(crate) writes: Mutex<BTreeMap<FactoryDigest, FactoryCandidatePublication>>,
  pub(crate) requests: Mutex<Vec<FactoryCandidatePublicationRequest>>,
}
#[async_trait]
impl FactoryCandidatePublisher for Publisher {
  async fn observe(
    &self,
    request: &FactoryCandidatePublicationRequest,
  ) -> Result<Option<FactoryCandidatePublication>, ApplicationError> {
    if self.behavior == PublicationCase::ChangedAncestry {
      return Err(ApplicationError::PreconditionFailed);
    }
    Ok(self.writes.lock().unwrap().get(&request.operation_id).cloned())
  }
  async fn publish(
    &self,
    request: &FactoryCandidatePublicationRequest,
  ) -> Result<FactoryCandidatePublication, ApplicationError> {
    assert_eq!(request.candidate.changeset_digest(), request.manifest.content_digest());
    let publication = FactoryCandidatePublication {
      operation_id: request.operation_id,
      candidate: request.candidate.clone(),
      receipt: request.operation_id,
      facts: crate::FactoryPublicationFacts::new(
        "refs/heads/reviewed-candidate".into(),
        request.candidate.candidate_revision().clone(),
        "23".into(),
        crate::FactoryReviewState::Open,
      )
      .unwrap(),
      usage: BudgetUsage::default(),
    };
    self.requests.lock().unwrap().push(request.clone());
    self
      .writes
      .lock()
      .unwrap()
      .insert(request.operation_id, publication.clone());
    if self.behavior == PublicationCase::LostResponse && !self.lost.swap(true, Ordering::SeqCst) {
      return Err(ApplicationError::unavailable());
    }
    Ok(publication)
  }
  async fn cancel(
    &self,
    _: &FactoryCandidatePublicationRequest,
    _: octacity_server_domain::Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    panic!("completed-publication contract fixture")
  }
}

pub(crate) fn specified_fixture(
  kind: &str,
  size: &str,
  route: &str,
  feedback: Option<&str>,
  behavior: PublicationCase,
) -> (PublishedFactoryAdmission, Arc<Execution>) {
  let mut wire: Value =
    serde_json::to_value(crate::factory_node_test_support::fixtures::supplied_base_flow_template()).unwrap();
  let plugin = reference("fixture.publisher");
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![plugin.clone()],
    ..Default::default()
  })
  .unwrap();
  wire["execution"]["permissions"] = json!(permissions);
  for node in wire["nodes"].as_array_mut().unwrap() {
    node.as_object_mut().unwrap().remove("pool");
    if node["key"] == "specification_publication" {
      node["permissions"] = json!(permissions);
      node["action"]["plugin"] = json!(plugin.clone());
    }
  }
  if behavior == PublicationCase::HumanDeclined {
    let budget = wire["nodes"][0]["budget"].clone();
    wire["nodes"].as_array_mut().unwrap().push(json!({"key":"owner_confirmation","kind":"human_gate","input_schema":"spec_review","outcomes":[{"key":"confirmed","kind":"success","schema":"spec_review"},{"key":"declined","kind":"failure","schema":"spec_review"}],"budget":budget,"required":true,"input_binding":{"sources":{"review":{"source":"predecessor","view":"payload","path":""}},"mapping":{"value":{"mapping":"pointer","path":"/review"}}}}));
    for edge in wire["transitions"].as_array_mut().unwrap() {
      if edge["predecessor"] == "specification_review_gate" && edge["outcome"] == "publication_pending" {
        edge["target"] = json!({"kind":"node","key":"owner_confirmation"});
      }
    }
    wire["transitions"].as_array_mut().unwrap().extend([
      json!({"predecessor":"owner_confirmation","outcome":"confirmed","target":{"kind":"node","key":"specification_publication"},"max_repeats":0,"exhausted_terminal":null}),
      json!({"predecessor":"owner_confirmation","outcome":"declined","target":{"kind":"terminal","key":"human_declined"},"max_repeats":0,"exhausted_terminal":null})]);
    wire["terminals"]["human_declined"] = json!("spec_review");
  }
  let template: FlowDefinitionTemplate = serde_json::from_value(wire).unwrap();
  let work = testing::factory_node_journal_contract_fixture().admission.work;
  let mut work_wire = serde_json::to_value(work).unwrap();
  work_wire["artifacts"]["specifications"] = json!(
    (1..=6)
      .map(|number| FactoryArtifactReference::new(
        octacity_server_domain::ArtifactId::generate(),
        FactoryDigest::from_bytes([number; 32]),
        100
      )
      .unwrap())
      .collect::<Vec<_>>()
  );
  let work: WorkEnvelope = serde_json::from_value(work_wire).unwrap();
  let (admission, execution) = configured_fixture_with_work(template, work);
  let mut execution = Arc::try_unwrap(execution).ok().unwrap();
  execution.output_builder = Some(Arc::new(QualifiedOutputs {
    feedback: feedback.map(key),
    validation: None,
  }));
  let classification = execution.reports.get_mut(&key("classification")).unwrap();
  classification["work_kind"] = json!(kind);
  classification["size"] = json!(size);
  classification["recommended_route"] = json!(route);
  classification["reproducibility"] = json!(if kind == "defect" {
    "reproduced"
  } else {
    "not_applicable"
  });
  let facts = execution.facts.get_mut(&key("classification")).unwrap();
  facts["work_kind"] = json!(kind);
  facts["size"] = json!(size);
  facts["reproduced"] = json!(kind == "defect");
  execution.reports.insert(key("feature_research"),json!({"outcome":"proposed","problem":"Add an operator-configured check","acceptance_boundary":"Declared contract passes","out_of_scope":"Deployment"}));
  execution.facts.insert(
    key("feature_research"),
    json!({"proposal_verified":true,"already_fixed":false}),
  );
  (admission, Arc::new(execution))
}

pub(crate) async fn claim_run<S: FactoryRunStore>(store: &S, run: FactoryRunId, time: i64) -> FactoryRunClaimRecord {
  let snapshot = store.factory_run_snapshot(run).await.unwrap();
  let claim = FactoryRunClaimRecord::new(
    run,
    key("requirements.owner"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::sha256("requirements.test.claim", &[&time.to_be_bytes()])),
      at(time),
      at(2000),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run,
      expected_version: snapshot.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run,
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
  claim
}

fn publication_registry(
  admission: &PublishedFactoryAdmission,
  execution: Arc<Execution>,
  behavior: PublicationCase,
) -> (Arc<FactoryNodeExecutorRegistry>, Arc<Publisher>) {
  let publisher = Arc::new(Publisher {
    behavior,
    lost: AtomicBool::new(false),
    writes: Mutex::new(BTreeMap::new()),
    requests: Mutex::new(vec![]),
  });
  let action = Arc::new(FactoryFlowActionAdapter::new(Arc::new(
    FactoryCandidatePublicationAction::new(
      reference("fixture.publisher"),
      publisher.clone(),
      admission
        .flow
        .closure()
        .definition(admission.flow.root_run().definition())
        .unwrap()
        .node(&key("specification_publication"))
        .unwrap()
        .outcome(&key("published"))
        .unwrap()
        .clone(),
    ),
  )));
  let build = Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution.clone()));
  let tool = fixtures_build_tool(admission);
  let registry = Arc::new(
    FactoryNodeExecutorRegistry::new(vec![
      (
        FlowNodeKind::Reasoning,
        NodeExecutionIdentity::External(tool.clone()),
        build.clone(),
      ),
      (FlowNodeKind::BuildCommand, NodeExecutionIdentity::External(tool), build),
      (
        FlowNodeKind::TrustedAction,
        NodeExecutionIdentity::External(reference("fixture.publisher")),
        action,
      ),
    ])
    .unwrap(),
  );
  (registry, publisher)
}

pub(crate) async fn verify_specified_owner<
  S: FactoryRunStore + FactoryPhasePoolStore + FactoryRunControlStore + 'static,
>(
  store: Arc<S>,
  restarted: Arc<S>,
  admission: &PublishedFactoryAdmission,
  execution: Arc<Execution>,
  behavior: PublicationCase,
  feedback: Option<&str>,
) {
  let (registry, publisher) = publication_registry(admission, execution.clone(), behavior);
  let mut claim = claim_run(store.as_ref(), admission.run.id(), 10).await;
  let mut terminal = None;
  let mut blocked = false;
  let mut unknown_responses = 0;
  let mut contracts = BTreeMap::new();
  for time in 31..170 {
    execution.clock.store(time, Ordering::SeqCst);
    let backend = if time % 2 == 0 {
      store.clone()
    } else {
      restarted.clone()
    };
    let snapshot = backend.factory_run_snapshot(admission.run.id()).await.unwrap();
    for record in snapshot
      .flow
      .data
      .records
      .iter()
      .filter(|row| row.input().node() == &key("specification_ready"))
    {
      let contract = AcceptedWorkContract::freeze(
        &snapshot.work,
        &snapshot.admitted_flow,
        &snapshot.flow.data,
        snapshot.flow.runtime(),
        record.node_attempt_id(),
      )
      .unwrap();
      let digest = contract.digest().unwrap();
      if let Some(previous) = contracts.insert(record.node_attempt_id(), digest) {
        assert_eq!(previous, digest, "later records cannot alter an accepted contract");
      }
      assert_eq!(
        record.payload().value()["specification"]["handoff"]["declared_requirements"],
        json!(snapshot.work.artifacts().specifications())
      );
    }
    let stale = behavior == PublicationCase::StaleReview
      && snapshot
        .flow
        .attempts
        .iter()
        .any(|row| row.node_key() == &key("specification_publication"));
    let owner = FactoryFlowRunner::new(
      backend.clone(),
      registry.clone(),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    match crate::factory_node_test_support::reconcile_admission_then_observe(
      &owner,
      backend.as_ref(),
      admission.run.id(),
      claim.id,
      at(if stale { 1500 } else { time }),
    )
    .await
    {
      Ok(FactoryFlowStep::Advanced) => {}
      Ok(FactoryFlowStep::Resolved(outcome)) => {
        terminal = Some(outcome);
        break;
      }
      Ok(FactoryFlowStep::Waiting) if behavior == PublicationCase::HumanDeclined => {
        let snapshot = backend.factory_run_snapshot(admission.run.id()).await.unwrap();
        let pending = snapshot
          .flow
          .attempts
          .iter()
          .find(|row| {
            row.node_kind() == FlowNodeKind::HumanGate
              && !snapshot
                .flow
                .completions
                .iter()
                .any(|done| done.node_attempt_id() == row.id())
          })
          .unwrap();
        let input = snapshot
          .flow
          .data
          .inputs
          .iter()
          .find(|input| input.digest().unwrap() == pending.input_digest())
          .unwrap();
        let context =
          ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
        FactoryControlHandlers::new(backend.clone())
          .execute_management_command(
            &context,
            &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
            ResolveFactoryNodeCommand {
              run_id: admission.run.id(),
              node_attempt_id: pending.id(),
              outcome: key("declined"),
              payload: input.payload().clone(),
              expected_version: snapshot.run.version(),
              idempotency_key: IdempotencyKey::new("declined-contract").unwrap(),
              requested_at: at(time),
            },
          )
          .await
          .unwrap();
        claim = claim_run(backend.as_ref(), admission.run.id(), time).await;
      }
      Err(ApplicationError::Store(StoreError::Unavailable))
        if behavior == PublicationCase::LostResponse
          && unknown_responses == 0
          && publisher.lost.load(Ordering::SeqCst) =>
      {
        unknown_responses += 1;
      }
      Err(ApplicationError::PreconditionFailed)
        if behavior == PublicationCase::ChangedAncestry
          && snapshot
            .flow
            .attempts
            .iter()
            .any(|row| row.node_key() == &key("specification_publication")) =>
      {
        blocked = true;
        break;
      }
      Err(ApplicationError::InvalidInput) if stale => {
        blocked = true;
        break;
      }
      other => panic!(
        "configured requirements at {time}: {other:?}; records {:?}",
        snapshot
          .flow
          .data
          .records
          .iter()
          .map(|row| (row.input().node().as_str(), row.metadata().unwrap()["outcome"].clone()))
          .collect::<Vec<_>>()
      ),
    }
  }
  let snapshot = restarted.factory_run_snapshot(admission.run.id()).await.unwrap();
  let requests = publisher.requests.lock().unwrap();
  match behavior {
    PublicationCase::ChangedAncestry | PublicationCase::StaleReview => {
      assert!(blocked);
      assert!(requests.is_empty());
      assert!(contracts.is_empty());
    }
    PublicationCase::HumanDeclined => {
      assert_eq!(terminal, Some(key("human_declined")));
      assert!(requests.is_empty());
      assert!(contracts.is_empty());
    }
    PublicationCase::Normal | PublicationCase::LostResponse => {
      assert_eq!(terminal, Some(key("verified_candidate")));
      let count = if feedback.is_some() { 2 } else { 1 };
      assert_eq!(snapshot.flow.cycles.len(), count);
      assert_eq!(contracts.len(), count);
      assert_eq!(requests.len(), count);
      assert_eq!(
        unknown_responses,
        usize::from(behavior == PublicationCase::LostResponse)
      );
      for cycle in &snapshot.flow.cycles {
        let accepted = snapshot
          .flow
          .data
          .records
          .iter()
          .find(|row| row.input().cycle_id() == cycle.id() && row.input().node() == &key("specification_ready"))
          .unwrap();
        let specification = &accepted.payload().value()["specification"];
        for name in ["protected_tests", "implementation"] {
          let input = snapshot
            .flow
            .data
            .inputs
            .iter()
            .find(|row| row.cycle_id() == cycle.id() && row.node() == &key(name))
            .unwrap();
          let context = input.context().unwrap();
          assert_eq!(
            context
              .subject()
              .candidate()
              .expect("accepted specification must select the execution candidate")
              .candidate_revision()
              .as_str(),
            specification["manifest"]["candidate_revision"].as_str().unwrap(),
          );
          assert_eq!(
            context.entries().len(),
            3,
            "only task and the accepted specification artifacts are granted"
          );
          for artifact_name in ["manifest", "bundle"] {
            let expected: FactoryArtifactReference =
              serde_json::from_value(specification["artifacts"][artifact_name].clone()).unwrap();
            assert!(
              context
                .entries()
                .iter()
                .any(|entry| entry.source() == &FactoryContextReference::Artifact(expected.clone()))
            );
          }
          assert!(context.entries().iter().any(
            |entry| entry.source() == &FactoryContextReference::Artifact(snapshot.work.artifacts().task().clone())
          ));
        }
        for name in [
          "specification_gate",
          "specification_review",
          "specification_review_gate",
          "specification_publication",
          "specification_ready",
          "protected_tests",
          "implementation",
          "implementation_result_gate",
        ] {
          assert_eq!(
            snapshot
              .flow
              .data
              .records
              .iter()
              .filter(|row| row.input().cycle_id() == cycle.id() && row.input().node() == &key(name))
              .count(),
            1,
            "fresh {name} required for each cycle"
          );
        }
      }
      if feedback.is_some() {
        let first = admission.flow.initial_cycle();
        let second = snapshot
          .flow
          .cycles
          .iter()
          .find(|row| row.predecessor() == Some(first.id()))
          .unwrap();
        assert!(!snapshot.flow.runtime().is_current_cycle(first.id()));
        assert!(snapshot.flow.runtime().is_current_cycle(second.id()));
        let original = snapshot
          .flow
          .data
          .records
          .iter()
          .find(|r| r.input().cycle_id() == first.id() && r.input().node() == &key("specification_gate"))
          .unwrap();
        let corrected = snapshot
          .flow
          .data
          .records
          .iter()
          .find(|r| r.input().cycle_id() == second.id() && r.input().node() == &key("specification_gate"))
          .unwrap();
        assert_eq!(
          corrected.payload().value()["manifest"]["base_revision"],
          original.payload().value()["manifest"]["candidate_revision"]
        );
        assert_eq!(
          corrected.input().payload().value()["expected_predecessor"],
          original.payload().value()["manifest"]["candidate_revision"]
        );
        assert_ne!(requests[0].candidate, requests[1].candidate);
      }
    }
  }
  for review in snapshot
    .flow
    .data
    .inputs
    .iter()
    .filter(|row| row.node() == &key("specification_review"))
  {
    let context = review.context().unwrap();
    assert_eq!(
      context.entries().len(),
      3 + snapshot.work.artifacts().specifications().len()
    );
    assert!(
      context
        .entries()
        .iter()
        .all(|entry| entry.logical_identity() != &key("task") && entry.logical_identity() != &key("author_transcript"))
    );
    assert!(review.payload().value().get("author_transcript").is_none());
  }
  if blocked || behavior == PublicationCase::HumanDeclined {
    assert!(
      !snapshot
        .flow
        .attempts
        .iter()
        .any(|row| row.node_key() == &key("protected_tests") || row.node_key() == &key("implementation"))
    );
  }
}
fn fixtures_build_tool(admission: &PublishedFactoryAdmission) -> ImmutableReference {
  admission
    .flow
    .closure()
    .definition(admission.flow.root_run().definition())
    .unwrap()
    .nodes()
    .iter()
    .find_map(|node| node.build().map(|build| build.tool.clone()))
    .unwrap()
}

#[test]
fn accepted_specification_selects_the_author_and_implementation_execution_context() {
  let (admission, execution) = specified_fixture(
    "feature_request",
    "large",
    "requirements",
    None,
    PublicationCase::Normal,
  );
  let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  let audit = testing::management_mutation_with_request((), "accepted-execution-context")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  crate::factory_flow_tests::run_ready(verify_specified_owner(
    store.clone(),
    store,
    &admission,
    execution,
    PublicationCase::Normal,
    None,
  ));
}

#[test]
fn specified_feature_defect_researched_and_small_work_reach_review_publication_and_readiness() {
  for (kind, size, route) in [
    ("feature_request", "large", "requirements"),
    ("defect", "large", "requirements"),
    ("feature_request", "large", "research"),
    ("feature_request", "small", "requirements"),
  ] {
    let (admission, execution) = specified_fixture(kind, size, route, None, PublicationCase::Normal);
    let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
    let audit = testing::management_mutation_with_request((), "specified-owner")
      .audit()
      .clone();
    store
      .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    crate::factory_flow_tests::run_ready(verify_specified_owner(
      store.clone(),
      store,
      &admission,
      execution,
      PublicationCase::Normal,
      None,
    ));
  }
}

#[test]
fn specification_publication_recovers_unknown_responses_and_blocks_conflicts_expiry_and_human_refusal() {
  for behavior in [
    PublicationCase::LostResponse,
    PublicationCase::ChangedAncestry,
    PublicationCase::StaleReview,
    PublicationCase::HumanDeclined,
  ] {
    let (admission, execution) = specified_fixture("feature_request", "large", "requirements", None, behavior);
    let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
    let audit = testing::management_mutation_with_request((), "publication-owner")
      .audit()
      .clone();
    store
      .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    crate::factory_flow_tests::run_ready(verify_specified_owner(
      store.clone(),
      store,
      &admission,
      execution,
      behavior,
      None,
    ));
  }
}

#[test]
fn implementation_and_verification_defects_rerun_requirements_without_reusing_prior_acceptance() {
  for feedback in ["implementation", "verification"] {
    let (admission, execution) = specified_fixture(
      "feature_request",
      "large",
      "requirements",
      Some(feedback),
      PublicationCase::Normal,
    );
    let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
    let audit = testing::management_mutation_with_request((), "correction-owner")
      .audit()
      .clone();
    store
      .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    crate::factory_flow_tests::run_ready(verify_specified_owner(
      store.clone(),
      store,
      &admission,
      execution,
      PublicationCase::Normal,
      Some(feedback),
    ));
  }
}

pub(crate) struct ValidationCase {
  classification: &'static str,
  exhaust: bool,
  candidates: Mutex<Vec<Value>>,
}

async fn validation_case_snapshot(
  kind: &str,
  size: &str,
  case: Arc<ValidationCase>,
) -> (FactoryRunSnapshot, FactoryKey) {
  let (admission, mut execution) = specified_fixture(
    kind,
    size,
    if size == "small" { "development" } else { "requirements" },
    None,
    PublicationCase::Normal,
  );
  Arc::get_mut(&mut execution).unwrap().output_builder = Some(Arc::new(QualifiedOutputs {
    feedback: None,
    validation: Some(case),
  }));
  let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  let audit = testing::management_mutation_with_request((), "validation-routing")
    .audit()
    .clone();
  store
    .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
    .unwrap();
  let claim = claim_run(store.as_ref(), admission.run.id(), 10).await;
  let (registry, _) = publication_registry(&admission, execution.clone(), PublicationCase::Normal);
  let mut resolved = None;
  for time in 31..220 {
    execution.clock.store(time, Ordering::SeqCst);
    // Recreate both owner and input preparer on every iteration: no model session or transient routing state survives.
    let owner = FactoryFlowRunner::new(
      store.clone(),
      registry.clone(),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    match crate::factory_node_test_support::reconcile_admission_then_observe(
      &owner,
      store.as_ref(),
      admission.run.id(),
      claim.id,
      at(time),
    )
    .await
    .unwrap()
    {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Resolved(outcome) => {
        resolved = Some(outcome);
        break;
      }
      other => panic!("unexpected validation progress {other:?}"),
    }
  }
  (
    store.factory_run_snapshot(admission.run.id()).await.unwrap(),
    resolved.expect("the bounded configured route must resolve"),
  )
}

#[test]
fn validation_rework_retains_the_exact_candidate_and_closed_context_after_restart() {
  crate::factory_flow_tests::run_ready(async {
    let case = Arc::new(ValidationCase {
      classification: "implementation_defect",
      exhaust: false,
      candidates: Mutex::new(vec![]),
    });
    let (snapshot, terminal) = validation_case_snapshot("defect", "small", case.clone()).await;
    assert_eq!(terminal, key("verified_candidate"));
    let implementations = snapshot
      .flow
      .data
      .inputs
      .iter()
      .filter(|row| row.node() == &key("implementation"))
      .collect::<Vec<_>>();
    assert_eq!(implementations.len(), 2);
    let initial = implementations
      .iter()
      .find(|input| input.payload().value().get("remediation").is_none())
      .unwrap();
    let rework = implementations
      .iter()
      .find(|input| input.payload().value().get("remediation").is_some())
      .unwrap();
    let candidates = case.candidates.lock().unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(rework.payload().value()["parent"]["candidate_revision"], candidates[0]);
    assert_eq!(
      initial.payload().value()["accepted_contract"],
      rework.payload().value()["accepted_contract"]
    );
    for input in &implementations {
      let wire = serde_json::to_string(input).unwrap();
      assert!(!wire.contains("protected_inventory") && !wire.contains("author_transcript"));
      assert!(input.context().unwrap().entries().iter().all(|entry| matches!(
        entry.source_kind(),
        ContextSourceKind::Task | ContextSourceKind::Evidence
      )));
    }
    let classifier = snapshot
      .flow
      .data
      .inputs
      .iter()
      .find(|input| input.node() == &key("validation_classification"))
      .unwrap();
    assert_eq!(classifier.context().unwrap().entries().len(), 1);
    assert_eq!(
      classifier
        .payload()
        .value()
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>(),
      vec!["request", "task"]
    );
    assert!(
      classifier.payload().value()["request"]
        .get("protected_inventory")
        .is_none()
    );
    let reviewer = snapshot
      .flow
      .data
      .inputs
      .iter()
      .find(|input| input.node() == &key("review"))
      .unwrap();
    assert_eq!(reviewer.payload().value()["candidate_revision"], candidates[1]);
    assert_eq!(reviewer.context().unwrap().entries().len(), 3);
    let review = snapshot
      .flow
      .data
      .build_intents
      .iter()
      .find(|intent| intent.input().node() == &key("review"))
      .unwrap();
    let implementation = snapshot
      .flow
      .data
      .build_intents
      .iter()
      .find(|intent| intent.input().node() == &key("implementation"))
      .unwrap();
    assert_ne!(review.profile().task_digest, implementation.profile().task_digest);
    assert_ne!(review.profile().model_or_tool, implementation.profile().model_or_tool);
  });
}

#[test]
fn validation_classification_owner_routes_are_finite_bounded_and_restartable() {
  crate::factory_flow_tests::run_ready(async {
    for (classification, exhaust, expected_terminal) in [
      ("implementation_defect", true, "validation_rework_exhausted"),
      ("test_defect", false, "verified_candidate"),
      ("indeterminate", false, "validation_classification_escalation"),
      ("reject", false, "validation_classification_rejection"),
    ] {
      let case = Arc::new(ValidationCase {
        classification,
        exhaust,
        candidates: Mutex::new(vec![]),
      });
      let (snapshot, terminal) = validation_case_snapshot("defect", "small", case).await;
      assert_eq!(terminal, key(expected_terminal), "{classification}");
      let count = |name: &str| {
        snapshot
          .flow
          .data
          .inputs
          .iter()
          .filter(|input| input.node() == &key(name))
          .count()
      };
      assert_eq!(
        count("implementation"),
        if classification == "implementation_defect" || classification == "test_defect" {
          2
        } else {
          1
        }
      );
      assert_eq!(
        count("protected_tests"),
        if classification == "test_defect" { 2 } else { 1 }
      );
      if classification == "test_defect" {
        let authors = snapshot
          .flow
          .data
          .inputs
          .iter()
          .filter(|input| input.node() == &key("protected_tests"))
          .collect::<Vec<_>>();
        let corrected = authors
          .iter()
          .find(|input| input.payload().value().get("correction").is_some())
          .expect("test correction must receive the bounded defect reason");
        assert_eq!(
          corrected.payload().value()["correction"]["reason"],
          "Preserve the public contract"
        );
        let classification = snapshot
          .flow
          .data
          .inputs
          .iter()
          .find(|input| input.node() == &key("validation_classification"))
          .unwrap();
        assert_eq!(
          corrected.payload().value()["correction"]["candidate_revision"],
          classification.payload().value()["request"]["candidate_revision"]
        );
        assert_eq!(
          corrected.payload().value()["correction"]["validation_digest"],
          classification.payload().value()["request"]["validation_digest"]
        );
        let first = authors
          .iter()
          .find(|input| input.payload().value().get("correction").is_none())
          .unwrap();
        assert_eq!(
          first.payload().value()["accepted"],
          corrected.payload().value()["accepted"]
        );
        for implementation in snapshot
          .flow
          .data
          .inputs
          .iter()
          .filter(|input| input.node() == &key("implementation"))
        {
          assert!(
            !serde_json::to_string(implementation)
              .unwrap()
              .contains("Preserve the public contract"),
            "author-only correction must not propagate to implementation"
          );
        }
      }
      assert_eq!(count("review"), usize::from(expected_terminal == "verified_candidate"));
    }
  });
}

#[test]
fn classified_requirements_correction_restarts_every_accepted_gate_in_a_fresh_cycle() {
  crate::factory_flow_tests::run_ready(async {
    let case = Arc::new(ValidationCase {
      classification: "requirements_defect",
      exhaust: false,
      candidates: Mutex::new(vec![]),
    });
    let (snapshot, terminal) = validation_case_snapshot("feature_request", "large", case).await;
    assert_eq!(terminal, key("verified_candidate"));
    assert_eq!(snapshot.flow.cycles.len(), 2);
    for name in ["specification_author", "specification_correction"] {
      assert_eq!(
        snapshot
          .flow
          .data
          .inputs
          .iter()
          .filter(|input| input.node() == &key(name))
          .count(),
        1,
        "{name}"
      );
    }
    for name in [
      "specification_gate",
      "specification_review",
      "specification_publication",
      "specification_ready",
      "protected_tests",
      "implementation",
      "protected_validation",
    ] {
      assert_eq!(
        snapshot
          .flow
          .data
          .inputs
          .iter()
          .filter(|input| input.node() == &key(name))
          .count(),
        2,
        "{name}"
      );
    }
    assert_eq!(
      snapshot
        .flow
        .data
        .inputs
        .iter()
        .filter(|input| input.node() == &key("review"))
        .count(),
      1
    );
  });
}
