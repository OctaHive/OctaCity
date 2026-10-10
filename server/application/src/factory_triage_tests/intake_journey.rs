use super::*;

#[path = "intake_journey/fixture.rs"]
mod journey_fixture;
use journey_fixture::*;

#[test]
fn configured_flow_can_publish_a_second_phase_only_from_its_exact_declared_handoff() {
  fixtures::run_ready(async {
    for (projected, assigned_pool) in [(false, false), (true, false), (true, true)] {
      let mut flow = journey();
      let root_ref = flow.closure.root();
      let mut root = draft(flow.closure.definition(root_ref).unwrap());
      let requirements = root
        .nodes
        .iter_mut()
        .find(|node| node.key() == &key("requirements"))
        .unwrap();
      if assigned_pool {
        *requirements = requirements.clone().with_phase_pool(key("requirements"));
      }
      let edge = root
        .transitions
        .iter_mut()
        .find(|edge| edge.predecessor().as_str() == "research")
        .unwrap();
      *edge = FlowTransition::new(
        key("research"),
        key("ready"),
        FlowTransitionTarget::Node(key("requirements")),
      );
      if projected {
        root.data_projections.push(FlowDataProjection::new(
          key("research"),
          key("ready"),
          key("requirements"),
          TriageSchema::Decision.reference().unwrap(),
        ));
      }
      let root = FlowDefinition::new(root).unwrap();
      let definitions = std::iter::once(root.clone())
        .chain(
          flow
            .closure
            .definitions()
            .iter()
            .filter(|def| def.reference() != root_ref)
            .cloned(),
        )
        .collect();
      flow.closure = PinnedFlowDefinitionClosure::new(root.reference(), definitions).unwrap();
      if !projected {
        assert!(flow.closure.clone().validate(flow.limits.clone()).is_err());
        continue;
      }
      let f = fixture_with_flow(
        ProjectFit::InScope,
        DuplicateStatus::Distinct,
        classified(
          WorkKind::Defect,
          WorkSize::Small,
          PreliminaryReproducibility::Inconclusive,
          TriageRoute::Research,
        ),
        None,
        flow,
      )
      .await;
      for at in [4, 21, 22] {
        f.runner.run_once(f.run, f.claim.id, fixtures::time(at)).await.unwrap();
      }
      let snapshot = f.store.factory_run_snapshot(f.run).await.unwrap();
      let root = snapshot.admitted_flow.root_run();
      let cycle = snapshot.admitted_flow.initial_cycle();
      let TriageJournalRecord::Classification(row) = snapshot.flow.triage.last().unwrap() else {
        unreachable!()
      };
      let original = TriageDisposition::Classification(row.decision.clone())
        .digest()
        .unwrap();
      let handoff = digest(88);
      let mut history = snapshot.flow.clone();
      let attempt = make_attempt(
        &snapshot,
        &mut history,
        root,
        cycle,
        &key("research"),
        original,
        NodeExecutionIdentity::BuiltIn,
      )
      .unwrap();
      complete(
        &snapshot,
        &mut history,
        root,
        &attempt,
        TypedFlowOutput {
          outcome: key("ready"),
          schema: TriageSchema::Decision.reference().unwrap(),
          digest: handoff,
          usage: BudgetUsage::default(),
        },
        fixtures::time(30),
      )
      .unwrap();
      commit_intake(
        f.store.as_ref(),
        &snapshot,
        FactoryRunHistoryAppend {
          flow: difference(&snapshot.flow, &history),
          ..Default::default()
        },
        None,
        fixtures::time(30),
      )
      .await
      .unwrap();
      let snapshot = f.store.factory_run_snapshot(f.run).await.unwrap();
      let declaration = snapshot
        .admitted_flow
        .closure()
        .definition(root.definition())
        .unwrap()
        .node(&key("requirements"))
        .unwrap();
      let budget = declaration.budget();
      let input = PhasePoolInput {
        run_id: f.run,
        run_version: snapshot.run.version(),
        flow_run_id: root.id(),
        cycle_id: cycle.id(),
        node: key("requirements"),
        severity: FindingSeverity::High,
        project_priority: 30,
        phase_input_digest: handoff,
        capabilities: Default::default(),
        dependencies: vec![],
        reservation: BudgetUsage {
          attempts: budget.max_attempts(),
          elapsed_millis: budget.max_elapsed_millis(),
          tokens: budget.max_tokens(),
          cost_micro_units: budget.max_cost_micro_units(),
          output_bytes: budget.max_output_bytes(),
        },
      };
      let policy = f.configuration.triage.pools[&TriageRoute::Requirements].clone();
      // Readiness and the handoff are valid, but a different phase cannot choose this node's queue.
      let wrong_pool = f.configuration.triage.pools[&TriageRoute::ProtectedTest].clone();
      assert!(f.store.publish_phase_ready(wrong_pool, input.clone()).await.is_err());
      if !assigned_pool {
        assert!(f.store.publish_phase_ready(policy, input).await.is_err());
        continue;
      }
      let entry = f
        .store
        .publish_phase_ready(policy.clone(), input.clone())
        .await
        .unwrap();
      assert_eq!(entry.input.phase_input_digest, handoff);
      assert_eq!(entry.input.node.as_str(), "requirements");
      let mut substituted = input.clone();
      substituted.phase_input_digest = original;
      assert!(f.store.publish_phase_ready(policy.clone(), substituted).await.is_err());
      let mut substituted = input;
      substituted.node = key("accepted_work_contract");
      assert!(f.store.publish_phase_ready(policy, substituted).await.is_err());
    }
  });
}

use crate::factory_research_tests as build_fixture;
use crate::factory_research_tests::fixtures as research_fixture;
use octacity_server_orchestrator::BuildState;

#[test]
fn manual_intake_all_research_branches_require_declared_routes_and_independent_terminal_facts() {
  fixtures::run_ready(async {
    use DefectResearchOutcome::*;
    // Expected routes are stated independently of policy evaluation. A configured
    // verification/terminal route still needs evidence from its separate gate.
    for (kind, size, outcome, configured, fact, expected) in [
      (
        WorkKind::Defect,
        WorkSize::Small,
        Reproduced,
        None,
        None,
        ResearchRoute::ProtectedTest,
      ),
      (
        WorkKind::Defect,
        WorkSize::Large,
        Reproduced,
        None,
        None,
        ResearchRoute::Requirements,
      ),
      (
        WorkKind::Defect,
        WorkSize::Large,
        Intermittent,
        None,
        None,
        ResearchRoute::Requirements,
      ),
      (
        WorkKind::Defect,
        WorkSize::Small,
        EnvironmentSpecific,
        None,
        None,
        ResearchRoute::Escalation,
      ),
      (
        WorkKind::Defect,
        WorkSize::Small,
        EnvironmentSpecific,
        None,
        Some(ResearchEvidenceFact::Verification),
        ResearchRoute::Verification,
      ),
      (
        WorkKind::Defect,
        WorkSize::Small,
        CannotReproduce,
        None,
        None,
        ResearchRoute::Rejection,
      ),
      (
        WorkKind::Defect,
        WorkSize::Small,
        NeedsHumanInput,
        None,
        None,
        ResearchRoute::Escalation,
      ),
      (
        WorkKind::FeatureRequest,
        WorkSize::Small,
        Reproduced,
        None,
        None,
        ResearchRoute::Requirements,
      ),
      (
        WorkKind::FeatureRequest,
        WorkSize::Small,
        Reproduced,
        Some(ResearchRoute::TerminalResolution),
        None,
        ResearchRoute::Escalation,
      ),
      (
        WorkKind::FeatureRequest,
        WorkSize::Small,
        Reproduced,
        Some(ResearchRoute::TerminalResolution),
        Some(ResearchEvidenceFact::TerminalResolution),
        ResearchRoute::TerminalResolution,
      ),
      (
        WorkKind::FeatureRequest,
        WorkSize::Small,
        Reproduced,
        Some(ResearchRoute::Verification),
        Some(ResearchEvidenceFact::Verification),
        ResearchRoute::Verification,
      ),
    ] {
      let (f, policy, intent) = research_intent(kind, size, configured).await;
      let builds = Arc::new(build_fixture::Builds::new());
      *builds.state.lock().unwrap() = BuildState::Succeeded;
      let outputs = if kind == WorkKind::Defect {
        defect_outputs(&intent, &policy, &builds, outcome)
      } else {
        build_fixture::feature_outputs(&intent, &policy, &builds)
      };
      let adapter = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(build_fixture::Outputs(Some(outputs))));
      let FactoryResearchStep::Completed(mut done) = adapter
        .observe_or_dispatch(
          &intent,
          &policy,
          build_fixture::intent_ownership(&intent),
          fixtures::time(31),
        )
        .await
        .unwrap()
      else {
        panic!("research completion required")
      };
      if let Some(fact) = fact {
        // Test double for the independently trusted verification gate, with a
        // distinct retained Artifact; model observations cannot construct proof.
        let mut record = done.evidence.record().clone();
        record.fact = fact;
        record.artifact = artifact(93);
        let proof =
          AcceptedResearchEvidence::new(intent.input(), &done.result, record, DeterministicGateOutcome::Passed)
            .unwrap();
        done.acceptance = policy
          .accept(
            done.acceptance.handoff.id(),
            intent.input(),
            &done.result,
            &[done.evidence.clone(), proof],
            done.usage,
            fixtures::time(31),
          )
          .unwrap();
      }
      assert_eq!(done.acceptance.decision.route, expected, "{kind:?}/{outcome:?}");
      publish_research_outcome(&f, &intent, &done).await;
      assert_eq!(builds.requests.lock().unwrap().len(), 1);
    }
  });
}

#[test]
fn nested_research_provider_failure_restart_and_ordinary_retry_keep_one_frozen_node() {
  fixtures::run_ready(async {
    for kind in [WorkKind::Defect, WorkKind::FeatureRequest] {
      let (f, policy, intent) = research_intent(kind, WorkSize::Large, None).await;
      let before = f.store.factory_run_snapshot(f.run).await.unwrap();
      let bytes = serde_json::to_vec(&intent).unwrap();
      let builds = Arc::new(build_fixture::Builds::new());
      builds.lose_response.store(true, Ordering::Release);
      let adapter = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(build_fixture::Outputs(None)));
      assert!(
        adapter
          .observe_or_dispatch(
            &intent,
            &policy,
            build_fixture::intent_ownership(&intent),
            fixtures::time(30)
          )
          .await
          .is_err()
      );
      assert_eq!(f.store.factory_run_snapshot(f.run).await.unwrap().flow, before.flow);
      *builds.state.lock().unwrap() = BuildState::Failed;
      assert!(matches!(
        adapter
          .observe_or_dispatch(
            &intent,
            &policy,
            build_fixture::intent_ownership(&intent),
            fixtures::time(30)
          )
          .await
          .unwrap(),
        FactoryResearchStep::ExecutionStopped {
          state: BuildState::Failed,
          ..
        }
      ));
      let snapshot = f.store.factory_run_snapshot(f.run).await.unwrap();
      let restored = ResearchBuildIntent::restore(
        &bytes,
        &snapshot.work,
        &snapshot.admitted_flow,
        &policy,
        snapshot
          .flow
          .runs
          .iter()
          .find(|row| row.id() == intent.flow().id())
          .unwrap(),
        snapshot
          .flow
          .attempts
          .iter()
          .find(|row| row.id() == intent.node().id())
          .unwrap(),
      )
      .unwrap();
      assert_eq!(restored, intent);
      let replacement = f
        .store
        .claim_factory_runs(
          ClaimFactoryRuns::new(key("research.restart"), fixtures::time(101), fixtures::time(200), 1).unwrap(),
        )
        .await
        .unwrap()
        .remove(0)
        .record;
      *builds.state.lock().unwrap() = BuildState::Succeeded;
      let (attempt_id, job_id) = build_fixture::complete_ordinary_retry(&builds);
      let mut outputs = if kind == WorkKind::Defect {
        defect_outputs(&restored, &policy, &builds, DefectResearchOutcome::Reproduced)
      } else {
        build_fixture::feature_outputs(&restored, &policy, &builds)
      };
      outputs.fresh_until = fixtures::time(800);
      let restarted = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(build_fixture::Outputs(Some(outputs))));
      let ownership = FactoryClaimOwnership::new(replacement.owner.clone(), replacement.claim);
      let FactoryResearchStep::Completed(done) = restarted
        .observe_or_dispatch(&restored, &policy, ownership.clone(), fixtures::time(101))
        .await
        .unwrap()
      else {
        panic!("retry completion required")
      };
      assert_eq!(done.result.provenance().producer.attempt_id(), attempt_id);
      assert_eq!(done.result.provenance().producer.job_id(), job_id);
      assert_eq!(done.usage.attempts, 1);
      assert_eq!(done.completion.claim(), replacement.claim);
      let FactoryResearchStep::Completed(replay) = restarted
        .observe_or_dispatch(&restored, &policy, ownership, fixtures::time(102))
        .await
        .unwrap()
      else {
        panic!("replay completion required")
      };
      assert_eq!(done.result, replay.result);
      assert_eq!(done.acceptance, replay.acceptance);
      {
        let requests = builds.requests.lock().unwrap();
        assert!(requests.iter().all(|request| request == &requests[0]));
      }
      publish_research_outcome(&f, &restored, &done).await;
      let after = f.store.factory_run_snapshot(f.run).await.unwrap();
      assert_eq!(
        after
          .flow
          .attempts
          .iter()
          .filter(|row| row.flow_run_id() == intent.flow().id() && row.node_key().as_str() == "research")
          .count(),
        1
      );
      assert_eq!(
        f.executor.calls.lock().unwrap().as_slice(),
        ["eligibility", "classification"]
      );
    }
  });
}

#[test]
fn manual_intake_can_skip_research_for_every_configured_triage_disposition() {
  fixtures::run_ready(async {
    for (route, fact, state) in [
      (TriageRoute::Requirements, None, None),
      (TriageRoute::ProtectedTest, Some(TriageEvidenceFact::Reproduced), None),
      (TriageRoute::Development, Some(TriageEvidenceFact::Reproduced), None),
      (
        TriageRoute::VerificationOnly,
        Some(TriageEvidenceFact::VerificationOnly),
        None,
      ),
      (
        TriageRoute::AlreadyFixed,
        Some(TriageEvidenceFact::AlreadyFixed),
        Some(FactoryRunState::Completed),
      ),
      (TriageRoute::Rejection, None, Some(FactoryRunState::Rejected)),
      (TriageRoute::Escalation, None, Some(FactoryRunState::Escalated)),
    ] {
      let mut flow = journey();
      if route == TriageRoute::Development {
        flow.triage.policy.small_work_route = Some(route);
      }
      let f = fixture_with_flow(
        ProjectFit::InScope,
        DuplicateStatus::Distinct,
        classified(
          WorkKind::Defect,
          WorkSize::Small,
          PreliminaryReproducibility::Reproduced,
          route,
        ),
        fact,
        flow,
      )
      .await;
      for at in [4, 21] {
        assert_eq!(
          f.runner.run_once(f.run, f.claim.id, fixtures::time(at)).await.unwrap(),
          FactoryTriageStep::Advanced
        );
      }
      let outcome = f.runner.run_once(f.run, f.claim.id, fixtures::time(22)).await.unwrap();
      let snapshot = f.store.factory_run_snapshot(f.run).await.unwrap();
      assert_eq!(snapshot.run.state(), state.unwrap_or(FactoryRunState::Admitted));
      assert_eq!(
        f.executor.calls.lock().unwrap().as_slice(),
        ["eligibility", "classification"]
      );
      assert!(snapshot.stage_attempts.is_empty());
      match outcome {
        FactoryTriageStep::Ready(entry) => {
          assert!(state.is_none());
          assert_eq!(
            entry.input.phase_input_digest,
            match snapshot.flow.triage.last().unwrap() {
              TriageJournalRecord::Classification(row) => TriageDisposition::Classification(row.decision.clone())
                .digest()
                .unwrap(),
              _ => unreachable!(),
            }
          );
          let request = SelectPhasePool {
            policy_digest: entry.policy_digest,
            request_id: digest(94),
            owner: key("next.worker"),
            observed_at: fixtures::time(101),
            expires_at: fixtures::time(200),
            capabilities: Default::default(),
            limit: 1,
          };
          assert_eq!(f.store.select_phase_ready(request).await.unwrap()[0].entry, entry);
        }
        FactoryTriageStep::Resolved(TriageDisposition::Classification(decision)) => {
          assert_eq!(decision.route, route);
          assert!(state.is_some());
        }
        other => panic!("unexpected {other:?}"),
      }
    }
    let f = fixture(
      ProjectFit::Inconclusive,
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
    f.runner.run_once(f.run, f.claim.id, fixtures::time(4)).await.unwrap();
    assert!(matches!(
      f.runner.run_once(f.run, f.claim.id, fixtures::time(21)).await.unwrap(),
      FactoryTriageStep::Resolved(TriageDisposition::Eligibility(EligibilityDecision {
        outcome: EligibilityOutcome::Escalation,
        ..
      }))
    ));
    assert_eq!(
      f.store.factory_run_snapshot(f.run).await.unwrap().run.state(),
      FactoryRunState::Escalated
    );
    assert_eq!(f.executor.calls.lock().unwrap().as_slice(), ["eligibility"]);
  });
}
