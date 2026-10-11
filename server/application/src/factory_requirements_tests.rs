//! Owner/store contracts with qualified Build-output and protected-publication ports.
use crate::factory_flow_tests::{Execution, FactoryTestOutputBuilder, Inputs, configured_fixture_with_work};
use crate::factory_node_test_support::{
  Builds, document_kind,
  fixtures::{key, reference},
};
use crate::{
  ApplicationError, CreateFactoryBuild, FactoryCandidatePublication, FactoryCandidatePublicationAction,
  FactoryCandidatePublicationRequest, FactoryCandidatePublisher, FactoryControlHandlers, FactoryFlowActionAdapter,
  FactoryFlowCoordinator, FactoryFlowRunner, FactoryFlowStep, FactoryNodeBuildAdapter, FactoryNodeBuildExecutionFacts,
  FactoryNodeBuildOutputs, FactoryNodeExecutorRegistry, ManagementAuthorizationGrant, ManagementCommandUseCase,
  ManagementRequestContext, ManagementRequestId, ManagementVisibility, ResolveFactoryNodeCommand,
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

struct QualifiedOutputs {
  feedback: Option<FactoryKey>,
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
    let report = if p.result_output == key("change-set-manifest.json") {
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
        let facts = if requirement.kind() == &key("specification_review_facts") {
          json!({"accepted":true,"input_digest":c.input_digest,"report_digest":report_digest,"fresh_until":500})
        } else {
          json!({"outcome":if report.get("requirements_defect").is_some(){"requirements_defect"}else{"observed"},"candidate_revision":report["specification"]["manifest"]["candidate_revision"],"input_digest":c.input_digest,"report_digest":report_digest,"fresh_until":10000,"requirements_evidence":report["requirements_defect"].get("evidence").cloned().unwrap_or_else(||json!([]))})
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
    let template: FlowDefinitionTemplate =
      serde_json::from_str(include_str!("../../../config/factory/flows/base-intake.json")).unwrap();
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
struct Publisher {
  behavior: PublicationCase,
  lost: AtomicBool,
  writes: Mutex<BTreeMap<FactoryDigest, FactoryCandidatePublication>>,
  requests: Mutex<Vec<FactoryCandidatePublicationRequest>>,
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
  let mut wire: Value = serde_json::from_str(include_str!("../../../config/factory/flows/base-intake.json")).unwrap();
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

async fn claim_run<S: FactoryRunStore>(store: &S, run: FactoryRunId, time: i64) -> FactoryRunClaimRecord {
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
    match owner
      .reconcile_flow(admission.run.id(), claim.id, at(if stale { 1500 } else { time }))
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
