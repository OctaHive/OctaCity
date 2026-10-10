use crate::{
  factory_admission_tests as fixtures,
  factory_triage::{
    commit_intake,
    history::{TypedFlowOutput, complete, complete_parent, difference, enter, make_attempt},
  },
  *,
};
use async_trait::async_trait;
use octacity_server_domain::{ArtifactId, ProjectId, RepositoryId, SourceReference};
use octacity_server_factory::*;
use octacity_server_store::{testing::InMemoryFactoryConfigurationStore, *};
use std::{
  collections::BTreeMap,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
  },
};

#[path = "factory_triage_tests/intake_journey.rs"]
mod intake_journey;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}
fn artifact(value: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), digest(value), 1).unwrap()
}
fn exact(name: &str) -> ImmutableReference {
  ImmutableReference::new(key(name), key("v1"), digest(8))
}
fn observation<T>(value: T) -> TriageObservation<T> {
  TriageObservation::new(value, artifact(40))
}

use octacity_server_factory as factory;
#[path = "../../core/octacity-server-factory/src/triage/tests/journey.rs"]
mod journey;
fn journey() -> FactoryFlowConfiguration {
  journey::journey(
    false,
    false,
    BudgetLimit::new(10, 10_000, 1_000, 10_000, 10_000).unwrap(),
  )
}

struct Discovery {
  candidates: Vec<DuplicateCandidate>,
  calls: Mutex<u32>,
}
#[async_trait]
impl FactoryTriageDiscovery for Discovery {
  async fn duplicate_candidates(&self, _: &WorkEnvelope) -> Result<Vec<DuplicateCandidate>, ApplicationError> {
    *self.calls.lock().unwrap() += 1;
    Ok(self.candidates.clone())
  }
  async fn phase_dependencies(
    &self,
    _: &WorkEnvelope,
    dependencies: &[TriageDependency],
  ) -> Result<Vec<PhasePoolDependency>, ApplicationError> {
    assert!(dependencies.is_empty());
    Ok(vec![])
  }
}
struct Evidence {
  fact: Option<TriageEvidenceFact>,
}
#[async_trait]
impl FactoryTriageEvidenceValidator for Evidence {
  async fn eligibility_evidence(
    &self,
    input: &EligibilityInput,
    result: &EligibilityResult,
  ) -> Result<Vec<AcceptedTriageEvidence>, ApplicationError> {
    let mut accepted = vec![
      AcceptedTriageEvidence::new(
        input.subject().clone(),
        input.digest().unwrap(),
        TriageEvidenceFact::ProjectFit(*result.project_fit().value()),
        result.project_fit().evidence().clone(),
        DeterministicGateOutcome::Passed,
      )
      .unwrap(),
    ];
    for row in result.duplicates() {
      accepted.push(
        AcceptedTriageEvidence::new(
          input.subject().clone(),
          input.digest().unwrap(),
          TriageEvidenceFact::Duplicate(row.value().clone()),
          row.evidence().clone(),
          DeterministicGateOutcome::Passed,
        )
        .unwrap(),
      );
    }
    Ok(accepted)
  }
  async fn classification_evidence(
    &self,
    input: &ClassificationInput,
    result: &TriageResult,
  ) -> Result<Vec<AcceptedTriageEvidence>, ApplicationError> {
    Ok(
      self
        .fact
        .clone()
        .map(|fact| {
          AcceptedTriageEvidence::new(
            input.eligibility_input().subject().clone(),
            input.digest().unwrap(),
            fact,
            result.classification().recommended_route.evidence().clone(),
            DeterministicGateOutcome::Passed,
          )
          .unwrap()
        })
        .into_iter()
        .collect(),
    )
  }
}
struct Executor {
  store: Arc<InMemoryFactoryConfigurationStore>,
  fit: ProjectFit,
  duplicate: DuplicateStatus,
  classification: TriageClassification,
  calls: Mutex<Vec<&'static str>>,
  pending: AtomicBool,
  lost_response: AtomicBool,
  wrong_profile: AtomicBool,
  incomplete_export: AtomicBool,
  results: Mutex<BTreeMap<FlowRunId, FactoryTriagePhaseResult>>,
}
#[async_trait]
impl FactoryTriagePhaseExecutor for Executor {
  async fn observe_or_dispatch(
    &self,
    request: FactoryTriagePhaseRequest,
  ) -> Result<Option<FactoryTriagePhaseResult>, ApplicationError> {
    if self.pending.load(Ordering::Acquire) {
      return Ok(None);
    }
    let snapshot = request.snapshot;
    let phase = request.flow_run;
    if let Some(result) = self.results.lock().unwrap().get(&phase.id()).cloned() {
      return Ok(Some(result));
    }
    let at = fixtures::time(
      snapshot
        .current_claim
        .as_ref()
        .unwrap()
        .claim
        .claimed_at()
        .unix_millis()
        .max(10),
    );
    let input_digest = match &request.input {
      FactoryTriagePhaseInput::Eligibility(input) => input.digest().unwrap(),
      FactoryTriagePhaseInput::Classification(input) => input.digest().unwrap(),
    };
    let mut history = snapshot.flow.clone();
    let mut producer_flow = phase.clone();
    loop {
      let definition = snapshot
        .admitted_flow
        .closure()
        .definition(producer_flow.definition())
        .unwrap();
      if definition.node(definition.entry()).unwrap().kind() != FlowNodeKind::SubflowCall {
        break;
      }
      let cycle = history
        .cycles
        .iter()
        .find(|row| row.flow_run_id() == producer_flow.id())
        .unwrap()
        .clone();
      producer_flow = enter(
        &snapshot,
        &mut history,
        &producer_flow,
        &cycle,
        definition.entry(),
        input_digest,
      )?;
    }
    let cycle = history
      .cycles
      .iter()
      .find(|row| row.flow_run_id() == producer_flow.id())
      .unwrap()
      .clone();
    let definition = snapshot
      .admitted_flow
      .closure()
      .definition(producer_flow.definition())
      .unwrap();
    let attempt = make_attempt(
      &snapshot,
      &mut history,
      &producer_flow,
      &cycle,
      definition.entry(),
      input_digest,
      NodeExecutionIdentity::External(exact("fake.triage")),
    )?;
    commit_intake(
      self.store.as_ref(),
      &snapshot,
      FactoryRunHistoryAppend {
        flow: difference(&snapshot.flow, &history),
        ..Default::default()
      },
      None,
      at,
    )
    .await?;
    let snapshot = self.store.factory_run_snapshot(snapshot.run.id()).await?;
    let mut provenance = TriageProvenance {
      input_digest,
      definition: producer_flow.definition(),
      node_attempt_id: attempt.id(),
      producer: exact("fake.triage"),
      model_or_tool: exact("fake.model"),
      task_digest: digest(32),
      result: artifact(33),
      observed_at: at,
    };
    if self.wrong_profile.load(Ordering::Acquire) {
      provenance.model_or_tool = exact("substituted.model");
    }
    let (result, output, schema) = match request.input {
      FactoryTriagePhaseInput::Eligibility(input) => {
        self.calls.lock().unwrap().push("eligibility");
        let result = EligibilityResult::new(
          &input,
          provenance,
          observation(self.fit),
          input
            .duplicate_candidates()
            .iter()
            .map(|candidate| {
              observation(DuplicateAssessment {
                candidate_id: candidate.work_id,
                status: self.duplicate,
              })
            })
            .collect(),
        )
        .unwrap();
        let output = result.digest().unwrap();
        (
          FactoryTriagePhaseResult::Eligibility(Box::new(result)),
          output,
          TriageSchema::EligibilityResult,
        )
      }
      FactoryTriagePhaseInput::Classification(input) => {
        self.calls.lock().unwrap().push("classification");
        let result = TriageResult::new(&input, provenance, self.classification.clone()).unwrap();
        let output = result.digest().unwrap();
        (
          FactoryTriagePhaseResult::Classification(Box::new(result)),
          output,
          TriageSchema::TriageResult,
        )
      }
    };
    let mut history = snapshot.flow.clone();
    complete(
      &snapshot,
      &mut history,
      &producer_flow,
      &attempt,
      TypedFlowOutput {
        outcome: key("observed"),
        schema: schema.reference().unwrap(),
        digest: output,
        usage: BudgetUsage {
          attempts: 1,
          tokens: 1,
          ..Default::default()
        },
      },
      at,
    )?;
    if !self.incomplete_export.load(Ordering::Acquire) {
      if definition.node(&key("verify")).is_some() {
        let gate = make_attempt(
          &snapshot,
          &mut history,
          &producer_flow,
          &cycle,
          &key("verify"),
          output,
          NodeExecutionIdentity::BuiltIn,
        )?;
        complete(
          &snapshot,
          &mut history,
          &producer_flow,
          &gate,
          TypedFlowOutput {
            outcome: key("observed"),
            schema: schema.reference().unwrap(),
            digest: output,
            usage: BudgetUsage::default(),
          },
          at,
        )?;
      }
      let mut child = producer_flow.clone();
      while child.id() != phase.id() {
        complete_parent(&snapshot, &mut history, &child, &key("observed"), output, at)?;
        child = history
          .runs
          .iter()
          .find(|row| row.id() == child.parent().unwrap().flow_run_id())
          .unwrap()
          .clone();
      }
    }
    commit_intake(
      self.store.as_ref(),
      &snapshot,
      FactoryRunHistoryAppend {
        flow: difference(&snapshot.flow, &history),
        ..Default::default()
      },
      None,
      at,
    )
    .await?;
    self.results.lock().unwrap().insert(phase.id(), result.clone());
    if self.lost_response.swap(false, Ordering::AcqRel) {
      return Err(ApplicationError::unavailable());
    }
    Ok(Some(result))
  }
}
fn classified(
  kind: WorkKind,
  size: WorkSize,
  reproduction: PreliminaryReproducibility,
  route: TriageRoute,
) -> TriageClassification {
  TriageClassification {
    work_kind: observation(kind),
    component: observation(key("engine")),
    severity: observation(FindingSeverity::High),
    size: observation(size),
    risk: observation(RiskClass::Low),
    dependencies: vec![],
    reproducibility: observation(reproduction),
    recommended_route: observation(route),
  }
}

struct Fixture {
  store: Arc<InMemoryFactoryConfigurationStore>,
  runner: FactoryTriageRunner<InMemoryFactoryConfigurationStore>,
  executor: Arc<Executor>,
  discovery: Arc<Discovery>,
  run: FactoryRunId,
  claim: FactoryRunClaimRecord,
  configuration: FactoryFlowConfiguration,
}
async fn fixture(
  fit: ProjectFit,
  duplicate: DuplicateStatus,
  classified: TriageClassification,
  fact: Option<TriageEvidenceFact>,
) -> Fixture {
  fixture_with_flow(fit, duplicate, classified, fact, journey()).await
}
async fn fixture_with_flow(
  fit: ProjectFit,
  duplicate: DuplicateStatus,
  classified: TriageClassification,
  fact: Option<TriageEvidenceFact>,
  flow: FactoryFlowConfiguration,
) -> Fixture {
  let project = ProjectId::generate();
  let config = FactoryConfigurationId::generate();
  let repository = RepositoryId::generate();
  let store = Arc::new(InMemoryFactoryConfigurationStore::new());
  store.seed_project(project).unwrap();
  let published = fixtures::configuration_with_flow(project, config, Some(flow.clone()));
  store
    .seed_factory_configuration_version(PublishedFactoryConfiguration {
      configuration: published,
      published_at: fixtures::time(1),
    })
    .unwrap();
  store
    .seed_repository_version(fixtures::repository(
      project,
      repository,
      Some(SourceReference::new("refs/heads/main").unwrap()),
    ))
    .unwrap();
  let handlers = FactoryAdmissionHandlers::new(store.clone(), Arc::new(fixtures::RecordingResolver::default()));
  let outcome = fixtures::admit(&handlers, fixtures::command(project, config, repository))
    .await
    .unwrap();
  let snapshot = store.factory_run_snapshot(outcome.factory_run_id).await.unwrap();
  assert_eq!(snapshot.admitted_flow.closure(), &flow.closure);
  let claim = store
    .claim_factory_runs(ClaimFactoryRuns::new(key("intake.worker"), fixtures::time(3), fixtures::time(100), 1).unwrap())
    .await
    .unwrap()
    .remove(0)
    .record;
  let candidates = if duplicate == DuplicateStatus::Duplicate {
    vec![DuplicateCandidate {
      work_id: WorkEnvelopeId::generate(),
      subject: snapshot.work.subject().clone(),
      evidence: artifact(34),
    }]
  } else {
    vec![]
  };
  let discovery = Arc::new(Discovery {
    candidates,
    calls: Mutex::new(0),
  });
  let executor = Arc::new(Executor {
    store: store.clone(),
    fit,
    duplicate,
    classification: classified,
    calls: Mutex::new(vec![]),
    pending: AtomicBool::new(false),
    lost_response: AtomicBool::new(false),
    wrong_profile: AtomicBool::new(false),
    incomplete_export: AtomicBool::new(false),
    results: Mutex::new(BTreeMap::new()),
  });
  let runner = FactoryTriageRunner::new(
    store.clone(),
    discovery.clone(),
    executor.clone(),
    Arc::new(Evidence { fact }),
  );
  Fixture {
    store,
    runner,
    executor,
    discovery,
    run: outcome.factory_run_id,
    claim,
    configuration: flow,
  }
}
#[test]
fn reconciler_runs_configured_intake_and_counts_terminal_commit() {
  fixtures::run_ready(async {
    let f = fixture(
      ProjectFit::OutOfScope,
      DuplicateStatus::Distinct,
      classified(
        WorkKind::FeatureRequest,
        WorkSize::Large,
        PreliminaryReproducibility::NotApplicable,
        TriageRoute::Development,
      ),
      None,
    )
    .await;
    let worker = |at| {
      crate::factory_reconciliation_tests::reconciler(f.store.clone(), "intake.reconciler", 1, 1, at).with_intake(
        Arc::new(FactoryTriageRunner::new(
          f.store.clone(),
          f.discovery.clone(),
          f.executor.clone(),
          Arc::new(Evidence { fact: None }),
        )),
      )
    };
    let shutdown = FactoryReconciliationShutdown::default();
    for at in [101, 202] {
      let result = worker(at)
        .run_once(fixtures::time(at), fixtures::time(at + 100), &shutdown)
        .await
        .unwrap();
      assert_eq!(result.claimed, 1);
      assert_eq!(result.actions_committed, 1);
    }
    let snapshot = f.store.factory_run_snapshot(f.run).await.unwrap();
    assert_eq!(snapshot.run.state(), FactoryRunState::Rejected);
    assert!(snapshot.stage_attempts.is_empty());
    assert_eq!(snapshot.flow.triage.len(), 2);
    assert_eq!(f.executor.calls.lock().unwrap().as_slice(), ["eligibility"]);
  });
}

#[test]
fn duplicate_and_out_of_scope_work_resolve_before_classification_and_retain_evidence() {
  fixtures::run_ready(async {
    for (fit, status, reason) in [
      (ProjectFit::InScope, DuplicateStatus::Duplicate, TriageReason::Duplicate),
      (
        ProjectFit::OutOfScope,
        DuplicateStatus::Distinct,
        TriageReason::OutOfScope,
      ),
    ] {
      let f = fixture(
        fit,
        status,
        classified(
          WorkKind::Defect,
          WorkSize::Small,
          PreliminaryReproducibility::Inconclusive,
          TriageRoute::Development,
        ),
        None,
      )
      .await;
      assert_eq!(
        f.runner.run_once(f.run, f.claim.id, fixtures::time(10)).await.unwrap(),
        FactoryTriageStep::Advanced
      );
      let FactoryTriageStep::Resolved(TriageDisposition::Eligibility(decision)) =
        f.runner.run_once(f.run, f.claim.id, fixtures::time(10)).await.unwrap()
      else {
        panic!("expected eligibility resolution")
      };
      assert_eq!(decision.reason, reason);
      assert_eq!(f.executor.calls.lock().unwrap().as_slice(), ["eligibility"]);
      let snapshot = f.store.factory_run_snapshot(f.run).await.unwrap();
      assert_eq!(snapshot.run.state(), FactoryRunState::Rejected);
      assert!(snapshot.stage_attempts.is_empty());
      assert!(snapshot.flow.triage[1].artifacts().len() > 1);
    }
  });
}
#[test]
fn manual_triage_routes_research_small_contract_large_requirements_verification_and_already_fixed() {
  fixtures::run_ready(async {
    for (classification, fact, expected) in [
      (
        classified(
          WorkKind::Defect,
          WorkSize::Small,
          PreliminaryReproducibility::Inconclusive,
          TriageRoute::Development,
        ),
        None,
        TriageRoute::Research,
      ),
      (
        classified(
          WorkKind::FeatureRequest,
          WorkSize::Small,
          PreliminaryReproducibility::NotApplicable,
          TriageRoute::Development,
        ),
        None,
        TriageRoute::ProtectedTest,
      ),
      (
        classified(
          WorkKind::FeatureRequest,
          WorkSize::Large,
          PreliminaryReproducibility::NotApplicable,
          TriageRoute::Development,
        ),
        None,
        TriageRoute::Requirements,
      ),
      (
        classified(
          WorkKind::FeatureRequest,
          WorkSize::Small,
          PreliminaryReproducibility::NotApplicable,
          TriageRoute::VerificationOnly,
        ),
        Some(TriageEvidenceFact::VerificationOnly),
        TriageRoute::VerificationOnly,
      ),
      (
        classified(
          WorkKind::FeatureRequest,
          WorkSize::Small,
          PreliminaryReproducibility::NotApplicable,
          TriageRoute::AlreadyFixed,
        ),
        Some(TriageEvidenceFact::AlreadyFixed),
        TriageRoute::AlreadyFixed,
      ),
    ] {
      let f = fixture(ProjectFit::InScope, DuplicateStatus::Distinct, classification, fact).await;
      for _ in 0..2 {
        assert_eq!(
          f.runner.run_once(f.run, f.claim.id, fixtures::time(10)).await.unwrap(),
          FactoryTriageStep::Advanced
        );
      }
      let outcome = f.runner.run_once(f.run, f.claim.id, fixtures::time(10)).await.unwrap();
      let snapshot = f.store.factory_run_snapshot(f.run).await.unwrap();
      let Some(TriageJournalRecord::Classification(row)) = snapshot.flow.triage.last() else {
        panic!("classification receipt missing")
      };
      assert_eq!(row.decision.route, expected);
      assert_eq!(
        f.executor.calls.lock().unwrap().as_slice(),
        ["eligibility", "classification"]
      );
      assert!(snapshot.stage_attempts.is_empty());
      assert!(snapshot.linked_builds.is_empty());
      match outcome {
        FactoryTriageStep::Ready(entry) => {
          assert_ne!(entry.input.phase_input_digest, row.decision.input_digest);
          assert_eq!(
            entry.input.node.as_str(),
            f.configuration.successor(expected).unwrap().as_str()
          );
          assert_eq!(
            f.store
              .phase_ready_entries(entry.policy_digest, None, 10)
              .await
              .unwrap()
              .len(),
            1
          );
        }
        FactoryTriageStep::Resolved(_) => assert_eq!(expected, TriageRoute::AlreadyFixed),
        other => panic!("unexpected {other:?}"),
      }
    }
  });
}

#[test]
fn frozen_input_and_lost_phase_response_recover_after_restart_and_claim_takeover() {
  fixtures::run_ready(async {
    let f = fixture(
      ProjectFit::InScope,
      DuplicateStatus::Distinct,
      classified(
        WorkKind::FeatureRequest,
        WorkSize::Small,
        PreliminaryReproducibility::NotApplicable,
        TriageRoute::Development,
      ),
      None,
    )
    .await;
    assert_eq!(
      f.runner.run_once(f.run, f.claim.id, fixtures::time(10)).await.unwrap(),
      FactoryTriageStep::Advanced
    );
    f.executor.lost_response.store(true, Ordering::Release);
    assert!(f.runner.run_once(f.run, f.claim.id, fixtures::time(10)).await.is_err());
    let before = f.store.factory_run_snapshot(f.run).await.unwrap();
    assert_eq!(before.flow.triage.len(), 1);
    assert_eq!(f.executor.calls.lock().unwrap().len(), 1);
    let replacement = f
      .store
      .claim_factory_runs(
        ClaimFactoryRuns::new(key("restart.worker"), fixtures::time(101), fixtures::time(200), 1).unwrap(),
      )
      .await
      .unwrap()
      .remove(0)
      .record;
    assert!(f.runner.run_once(f.run, f.claim.id, fixtures::time(101)).await.is_err());
    let runner = FactoryTriageRunner::new(
      f.store.clone(),
      f.discovery.clone(),
      f.executor.clone(),
      Arc::new(Evidence { fact: None }),
    );
    assert_eq!(
      runner
        .run_once(f.run, replacement.id, fixtures::time(101))
        .await
        .unwrap(),
      FactoryTriageStep::Advanced
    );
    let ready = runner
      .run_once(f.run, replacement.id, fixtures::time(101))
      .await
      .unwrap();
    assert!(matches!(ready, FactoryTriageStep::Ready(_)));
    assert_eq!(
      runner
        .run_once(f.run, replacement.id, fixtures::time(101))
        .await
        .unwrap(),
      ready
    );
    assert_eq!(*f.discovery.calls.lock().unwrap(), 1);
    assert_eq!(
      f.executor.calls.lock().unwrap().as_slice(),
      ["eligibility", "classification"]
    );
    let after = f.store.factory_run_snapshot(f.run).await.unwrap();
    for record in &after.flow.triage {
      let encoded = serde_json::to_vec(record).unwrap();
      let restored: TriageJournalRecord = serde_json::from_slice(&encoded).unwrap();
      assert_eq!(&restored, record);
      restored.validate(&after.work, &after.admitted_flow).unwrap();
    }
    assert!(after.flow.completions.iter().any(|row| {
      row.claim() == replacement.claim
        && before
          .flow
          .attempts
          .iter()
          .any(|attempt| attempt.id() == row.node_attempt_id() && attempt.claim() == f.claim.claim)
    }));
  });
}

#[test]
fn substituted_profile_and_expired_owner_cannot_advance_gates_or_publish_pools() {
  fixtures::run_ready(async {
    let f = fixture(
      ProjectFit::InScope,
      DuplicateStatus::Distinct,
      classified(
        WorkKind::FeatureRequest,
        WorkSize::Small,
        PreliminaryReproducibility::NotApplicable,
        TriageRoute::Development,
      ),
      None,
    )
    .await;
    f.executor.wrong_profile.store(true, Ordering::Release);
    assert_eq!(
      f.runner.run_once(f.run, f.claim.id, fixtures::time(10)).await.unwrap(),
      FactoryTriageStep::Advanced
    );
    assert!(f.runner.run_once(f.run, f.claim.id, fixtures::time(10)).await.is_err());
    let snapshot = f.store.factory_run_snapshot(f.run).await.unwrap();
    assert_eq!(snapshot.flow.triage.len(), 1);
    assert!(
      !snapshot
        .flow
        .attempts
        .iter()
        .any(|row| row.node_key().as_str() == "eligibility_policy")
    );
    assert_eq!(f.executor.calls.lock().unwrap().as_slice(), ["eligibility"]);
    assert!(f.runner.run_once(f.run, f.claim.id, fixtures::time(100)).await.is_err());
    assert_eq!(f.executor.calls.lock().unwrap().len(), 1);
  });
}

#[test]
fn journal_cannot_rewrite_a_phase_or_forge_a_route_without_accepted_observations() {
  fixtures::run_ready(async {
    let f = fixture(
      ProjectFit::InScope,
      DuplicateStatus::Distinct,
      classified(
        WorkKind::FeatureRequest,
        WorkSize::Large,
        PreliminaryReproducibility::NotApplicable,
        TriageRoute::Development,
      ),
      None,
    )
    .await;
    for _ in 0..3 {
      f.runner.run_once(f.run, f.claim.id, fixtures::time(10)).await.unwrap();
    }
    let snapshot = f.store.factory_run_snapshot(f.run).await.unwrap();
    let mut record = snapshot.flow.triage.last().unwrap().clone();
    if let TriageJournalRecord::Classification(row) = &mut record {
      row.decision.route = TriageRoute::Development;
    }
    assert!(record.validate(&snapshot.work, &snapshot.admitted_flow).is_err());
    let append = FactoryRunHistoryAppend {
      flow: FactoryFlowHistory {
        triage: vec![record],
        ..Default::default()
      },
      ..Default::default()
    };
    assert!(
      commit_intake(f.store.as_ref(), &snapshot, append, None, fixtures::time(10))
        .await
        .is_err()
    );
    assert_eq!(f.store.factory_run_snapshot(f.run).await.unwrap(), snapshot);
  });
}

#[test]
fn replaceable_phases_accept_gated_and_nested_producers_but_require_completed_exports() {
  fixtures::run_ready(async {
    for nested in [false, true] {
      let f = fixture_with_flow(
        ProjectFit::OutOfScope,
        DuplicateStatus::Distinct,
        classified(
          WorkKind::Defect,
          WorkSize::Small,
          PreliminaryReproducibility::Inconclusive,
          TriageRoute::Research,
        ),
        None,
        journey::journey(true, nested, journey().limits.budget()),
      )
      .await;
      assert_eq!(
        f.runner.run_once(f.run, f.claim.id, fixtures::time(10)).await.unwrap(),
        FactoryTriageStep::Advanced
      );
      assert!(matches!(
        f.runner.run_once(f.run, f.claim.id, fixtures::time(10)).await.unwrap(),
        FactoryTriageStep::Resolved(_)
      ));
      assert_eq!(
        f.store.factory_run_snapshot(f.run).await.unwrap().run.state(),
        FactoryRunState::Rejected
      );
      let stopped = fixture_with_flow(
        ProjectFit::OutOfScope,
        DuplicateStatus::Distinct,
        f.executor.classification.clone(),
        None,
        journey::journey(true, nested, journey().limits.budget()),
      )
      .await;
      stopped.executor.incomplete_export.store(true, Ordering::Release);
      stopped
        .runner
        .run_once(stopped.run, stopped.claim.id, fixtures::time(10))
        .await
        .unwrap();
      assert!(
        stopped
          .runner
          .run_once(stopped.run, stopped.claim.id, fixtures::time(10))
          .await
          .is_err()
      );
      let snapshot = stopped.store.factory_run_snapshot(stopped.run).await.unwrap();
      assert_eq!(snapshot.flow.triage.len(), 1);
      assert_eq!(snapshot.run.state(), FactoryRunState::Admitted);
    }
    let f = fixture_with_flow(
      ProjectFit::InScope,
      DuplicateStatus::Distinct,
      classified(
        WorkKind::FeatureRequest,
        WorkSize::Small,
        PreliminaryReproducibility::NotApplicable,
        TriageRoute::Development,
      ),
      None,
      journey::journey(true, false, journey().limits.budget()),
    )
    .await;
    for _ in 0..2 {
      f.runner.run_once(f.run, f.claim.id, fixtures::time(10)).await.unwrap();
    }
    assert!(matches!(
      f.runner.run_once(f.run, f.claim.id, fixtures::time(10)).await.unwrap(),
      FactoryTriageStep::Ready(_)
    ));
  });
}

/// Shared accepted intake fixture for isolated downstream research execution tests.
pub(crate) async fn accepted_research_work() -> FactoryRunSnapshot {
  accepted_research_work_for(WorkKind::Defect).await
}

pub(crate) async fn accepted_research_work_for(kind: WorkKind) -> FactoryRunSnapshot {
  let f = fixture(
    ProjectFit::InScope,
    DuplicateStatus::Distinct,
    classified(
      kind,
      WorkSize::Small,
      if kind == WorkKind::Defect {
        PreliminaryReproducibility::Inconclusive
      } else {
        PreliminaryReproducibility::NotApplicable
      },
      TriageRoute::Research,
    ),
    None,
  )
  .await;
  for at in [4, 21, 22] {
    f.runner.run_once(f.run, f.claim.id, fixtures::time(at)).await.unwrap();
  }
  f.store.factory_run_snapshot(f.run).await.unwrap()
}
