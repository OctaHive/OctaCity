// Only public admission, Build and store ports are substituted. Flow history and
// readiness use the production contracts; the graph fixes every typed projection.
use super::super::*;
use super::{build_fixture, research_fixture};

pub(super) fn draft(definition: &FlowDefinition) -> FlowDefinitionInput {
  FlowDefinitionInput {
    id: definition.reference().id(),
    version: definition.reference().version(),
    input_schema: definition.input_schema().cloned(),
    entry: definition.entry().clone(),
    nodes: definition.nodes().to_vec(),
    transitions: definition.transitions().to_vec(),
    terminals: definition.terminals().to_vec(),
    context_projections: definition.context_projections().to_vec(),
    data_projections: definition.data_projections().to_vec(),
    execution: definition.execution().clone(),
  }
}

pub(super) fn research_journey(kind: WorkKind) -> (FactoryFlowConfiguration, ValidatedFlowDefinitionClosure) {
  let total = BudgetLimit::new(24, 30_000, 3_000, 30_000, 30_000).unwrap();
  let attempt = BudgetLimit::new(1, 1000, 100, 1000, 2500).unwrap();
  let research = research_fixture::research_closure_with_budget(
    kind,
    &ResearchRoute::ALL,
    BudgetLimit::new(3, 3000, 300, 3000, 7500).unwrap(),
    attempt,
  );
  let mut flow = journey::journey(false, false, total);
  let original_root = flow.closure.root();
  let mut root = draft(flow.closure.definition(original_root).unwrap());
  let input = ResearchSchema::Input.reference().unwrap();
  let decision = ResearchSchema::Decision.reference().unwrap();
  let handoff = ResearchSchema::Handoff.reference().unwrap();
  let node = |name: &str,
              schema: ImmutableReference,
              output: ImmutableReference,
              outcome: &str,
              subflow: Option<FlowDefinitionRef>| {
    FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: key(name),
      kind: if subflow.is_some() {
        FlowNodeKind::SubflowCall
      } else {
        FlowNodeKind::DeterministicGate
      },
      input_schema: Some(schema),
      outcomes: vec![FlowOutcomeDefinition::new(
        key(outcome),
        FlowOutcomeKind::Success,
        output,
      )],
      budget: attempt,
      permissions: FactoryPermissionSet::deny_all(),
      required: true,
      subflow,
    })
    .unwrap()
  };
  root.nodes.retain(|node| node.key().as_str() != "research");
  root.nodes.push(node(
    "research",
    TriageSchema::Decision.reference().unwrap(),
    input.clone(),
    "ready",
    None,
  ));
  root
    .transitions
    .retain(|edge| edge.predecessor().as_str() != "research");
  root.transitions.push(FlowTransition::new(
    key("research"),
    key("ready"),
    FlowTransitionTarget::Node(key("research_execution")),
  ));
  root.data_projections.push(FlowDataProjection::new(
    key("research"),
    key("ready"),
    key("research_execution"),
    input.clone(),
  ));
  root.nodes.push(
    FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: key("research_execution"),
      kind: FlowNodeKind::SubflowCall,
      input_schema: Some(input),
      outcomes: ResearchRoute::ALL
        .into_iter()
        .map(|route| FlowOutcomeDefinition::new(key(route.as_str()), FlowOutcomeKind::Success, decision.clone()))
        .collect(),
      budget: research.limits().budget(),
      permissions: FactoryPermissionSet::deny_all(),
      required: true,
      subflow: Some(research.closure().root()),
    })
    .unwrap(),
  );
  for route in ResearchRoute::ALL {
    let projection = key(&format!("handoff.{}", route.as_str()));
    let phase = key(&format!("researched.{}", route.as_str()));
    root.nodes.push(node(
      projection.as_str(),
      decision.clone(),
      handoff.clone(),
      "ready",
      None,
    ));
    root.transitions.push(FlowTransition::new(
      key("research_execution"),
      key(route.as_str()),
      FlowTransitionTarget::Node(projection.clone()),
    ));
    root.data_projections.push(FlowDataProjection::new(
      key("research_execution"),
      key(route.as_str()),
      projection.clone(),
      decision.clone(),
    ));
    let terminal = matches!(
      route,
      ResearchRoute::Rejection | ResearchRoute::Escalation | ResearchRoute::TerminalResolution
    );
    if !terminal {
      let pool = match route {
        ResearchRoute::ProtectedTest => TriageRoute::ProtectedTest,
        ResearchRoute::Verification => TriageRoute::VerificationOnly,
        _ => TriageRoute::Requirements,
      };
      root.nodes.push(
        node(phase.as_str(), handoff.clone(), handoff.clone(), "ready", None)
          .with_phase_pool(flow.triage.pools[&pool].phase.clone()),
      );
      root.data_projections.push(FlowDataProjection::new(
        projection.clone(),
        key("ready"),
        phase.clone(),
        handoff.clone(),
      ));
      root.transitions.push(FlowTransition::new(
        phase.clone(),
        key("ready"),
        FlowTransitionTarget::Terminal(key("research.finished")),
      ));
    } else {
      root
        .terminals
        .push(FlowTerminalDefinition::new(phase.clone(), handoff.clone()));
    }
    root.transitions.push(FlowTransition::new(
      projection,
      key("ready"),
      if terminal {
        FlowTransitionTarget::Terminal(phase)
      } else {
        FlowTransitionTarget::Node(phase)
      },
    ));
  }
  root
    .terminals
    .push(FlowTerminalDefinition::new(key("research.finished"), handoff));
  let root = FlowDefinition::new(root).unwrap();
  let definitions = std::iter::once(root.clone())
    .chain(
      flow
        .closure
        .definitions()
        .iter()
        .filter(|def| def.reference() != original_root)
        .cloned(),
    )
    .chain(research.closure().definitions().iter().cloned())
    .collect();
  flow.closure = PinnedFlowDefinitionClosure::new(root.reference(), definitions).unwrap();
  (flow, research)
}

pub(super) async fn persist(f: &Fixture, snapshot: &FactoryRunSnapshot, history: FactoryFlowHistory, at: i64) {
  commit_intake(
    f.store.as_ref(),
    snapshot,
    FactoryRunHistoryAppend {
      flow: difference(&snapshot.flow, &history),
      ..Default::default()
    },
    None,
    fixtures::time(at),
  )
  .await
  .unwrap();
}

pub(super) async fn research_intent(
  kind: WorkKind,
  size: WorkSize,
  selected: Option<ResearchRoute>,
) -> (Fixture, ResearchPolicy, ResearchBuildIntent) {
  let (flow, closure) = research_journey(kind);
  let f = fixture_with_flow(
    ProjectFit::InScope,
    DuplicateStatus::Distinct,
    classified(
      kind,
      size,
      if kind == WorkKind::Defect {
        PreliminaryReproducibility::Inconclusive
      } else {
        PreliminaryReproducibility::NotApplicable
      },
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
  let TriageJournalRecord::Classification(receipt) = snapshot.flow.triage.last().unwrap() else {
    unreachable!()
  };
  assert_eq!(receipt.decision.route, TriageRoute::Research);
  let base = research_fixture::input(&snapshot.work, &snapshot.admitted_flow, *receipt.clone(), kind);
  let mut settings = research_fixture::settings(kind);
  settings.budget = closure.limits().budget();
  settings.attempt_budget = BudgetLimit::new(1, 1000, 100, 1000, 2500).unwrap();
  settings.max_proposal_bytes = 2000;
  settings.small_work_max_risk = RiskClass::Medium;
  if let Some(route) = selected {
    settings.outcomes.insert(
      if kind == WorkKind::Defect {
        ResearchOutcome::Defect(DefectResearchOutcome::Reproduced)
      } else {
        ResearchOutcome::FeatureProposal
      },
      route,
    );
  }
  let policy = ResearchPolicy::new(
    snapshot.work.configuration().clone(),
    &closure,
    kind,
    settings,
    research_fixture::profiles(&closure, snapshot.work.subject().project_id()),
  )
  .unwrap();
  let input = ResearchInput::new(
    &snapshot.work,
    &snapshot.admitted_flow,
    base.accepted_triage().clone(),
    base.context().clone(),
    base.retrieval().to_vec(),
    base.details().clone(),
    policy.digest().unwrap(),
  )
  .unwrap();
  let root = snapshot.admitted_flow.root_run();
  let cycle = snapshot.admitted_flow.initial_cycle();
  let mut history = snapshot.flow.clone();
  let preparation = make_attempt(
    &snapshot,
    &mut history,
    root,
    cycle,
    &key("research"),
    TriageDisposition::Classification(receipt.decision.clone())
      .digest()
      .unwrap(),
    NodeExecutionIdentity::BuiltIn,
  )
  .unwrap();
  complete(
    &snapshot,
    &mut history,
    root,
    &preparation,
    TypedFlowOutput {
      outcome: key("ready"),
      schema: ResearchSchema::Input.reference().unwrap(),
      digest: input.digest().unwrap(),
      usage: BudgetUsage::default(),
    },
    research_fixture::time(24),
  )
  .unwrap();
  let child = enter(
    &snapshot,
    &mut history,
    root,
    cycle,
    &key("research_execution"),
    input.digest().unwrap(),
  )
  .unwrap();
  let cycle = history
    .cycles
    .iter()
    .find(|cycle| cycle.flow_run_id() == child.id())
    .unwrap()
    .clone();
  let node = make_attempt(
    &snapshot,
    &mut history,
    &child,
    &cycle,
    &key("research"),
    input.digest().unwrap(),
    NodeExecutionIdentity::External(research_fixture::reference("research-tool")),
  )
  .unwrap();
  persist(&f, &snapshot, history, 24).await;
  let intent = ResearchBuildIntent::new(input, &policy, child, node, BudgetUsage::default(), 7).unwrap();
  (f, policy, intent)
}

pub(super) async fn publish_research_outcome(
  f: &Fixture,
  intent: &ResearchBuildIntent,
  done: &FactoryResearchCompletion,
) {
  let snapshot = f.store.factory_run_snapshot(f.run).await.unwrap();
  let at = (done.completion.observed_at().unix_millis() + 1).max(32);
  let mut history = snapshot.flow.clone();
  history.completions.push(done.completion.clone());
  let cycle = history
    .cycles
    .iter()
    .find(|cycle| cycle.flow_run_id() == intent.flow().id())
    .unwrap()
    .clone();
  let gate = make_attempt(
    &snapshot,
    &mut history,
    intent.flow(),
    &cycle,
    &key("research_policy"),
    done.result.digest().unwrap(),
    NodeExecutionIdentity::BuiltIn,
  )
  .unwrap();
  let decision = FactoryDigest::content_sha256(&serde_json::to_vec(&done.acceptance.decision).unwrap());
  let route = done.acceptance.decision.route;
  complete(
    &snapshot,
    &mut history,
    intent.flow(),
    &gate,
    TypedFlowOutput {
      outcome: key(route.as_str()),
      schema: ResearchSchema::Decision.reference().unwrap(),
      digest: decision,
      usage: BudgetUsage::default(),
    },
    fixtures::time(at),
  )
  .unwrap();
  complete_parent(
    &snapshot,
    &mut history,
    intent.flow(),
    &key(route.as_str()),
    decision,
    fixtures::time(at),
  )
  .unwrap();
  let root = snapshot.admitted_flow.root_run();
  let cycle = snapshot.admitted_flow.initial_cycle();
  let projection = make_attempt(
    &snapshot,
    &mut history,
    root,
    cycle,
    &key(&format!("handoff.{}", route.as_str())),
    decision,
    NodeExecutionIdentity::BuiltIn,
  )
  .unwrap();
  let handoff = done.acceptance.handoff.digest().unwrap();
  complete(
    &snapshot,
    &mut history,
    root,
    &projection,
    TypedFlowOutput {
      outcome: key("ready"),
      schema: ResearchSchema::Handoff.reference().unwrap(),
      digest: handoff,
      usage: BudgetUsage::default(),
    },
    fixtures::time(at),
  )
  .unwrap();
  persist(f, &snapshot, history, at).await;
  let snapshot = f.store.factory_run_snapshot(f.run).await.unwrap();
  let terminal = matches!(
    route,
    ResearchRoute::Rejection | ResearchRoute::Escalation | ResearchRoute::TerminalResolution
  );
  let validated = snapshot.admitted_flow.validated().unwrap();
  let completion = snapshot
    .flow
    .completions
    .iter()
    .find(|row| row.node_attempt_id() == projection.id())
    .unwrap();
  let directives = FlowInterpreter::new(&validated)
    .advance(
      root,
      cycle,
      &projection,
      completion,
      &snapshot.flow.attempts,
      &snapshot.flow.completions,
    )
    .unwrap();
  if terminal {
    assert_eq!(
      directives,
      vec![FlowDirective::Complete {
        terminal: key(&format!("researched.{}", route.as_str())),
        schema_digest: ResearchSchema::Handoff.reference().unwrap().digest()
      }]
    );
    let mut dependency = PhasePoolDependency {
      run_id: f.run,
      work_id: snapshot.work.id(),
      work_digest: phase_pool_work_digest(&snapshot),
      terminal: key(&format!("researched.{}", route.as_str())),
    };
    assert!(phase_pool_dependency_ready(
      &dependency,
      snapshot.work.subject().project_id(),
      &snapshot
    ));
    dependency.terminal = key("research.finished");
    assert!(!phase_pool_dependency_ready(
      &dependency,
      snapshot.work.subject().project_id(),
      &snapshot
    ));
    let research_pool = &f.configuration.triage.pools[&TriageRoute::Research];
    assert!(
      f.store
        .phase_ready_entries(research_pool.digest(), None, 10)
        .await
        .unwrap()
        .is_empty()
    );
    let terminal_input = PhasePoolInput {
      run_id: f.run,
      run_version: snapshot.run.version(),
      flow_run_id: root.id(),
      cycle_id: cycle.id(),
      node: key(&format!("researched.{}", route.as_str())),
      severity: FindingSeverity::High,
      project_priority: 30,
      phase_input_digest: handoff,
      capabilities: Default::default(),
      dependencies: vec![],
      reservation: BudgetUsage {
        attempts: 1,
        ..Default::default()
      },
    };
    for policy in f.configuration.triage.pools.values() {
      assert!(
        f.store
          .publish_phase_ready(policy.clone(), terminal_input.clone())
          .await
          .is_err()
      );
    }
    assert!(snapshot.stage_attempts.is_empty());
    return;
  }
  let pool_route = match route {
    ResearchRoute::ProtectedTest => TriageRoute::ProtectedTest,
    ResearchRoute::Verification => TriageRoute::VerificationOnly,
    _ => TriageRoute::Requirements,
  };
  let policy = f.configuration.triage.pools[&pool_route].clone();
  let input = PhasePoolInput {
    run_id: f.run,
    run_version: snapshot.run.version(),
    flow_run_id: root.id(),
    cycle_id: cycle.id(),
    node: key(&format!("researched.{}", route.as_str())),
    severity: FindingSeverity::High,
    project_priority: 30,
    phase_input_digest: handoff,
    capabilities: Default::default(),
    dependencies: vec![],
    reservation: BudgetUsage {
      attempts: 1,
      elapsed_millis: 1000,
      tokens: 100,
      cost_micro_units: 1000,
      output_bytes: 2500,
    },
  };
  let entry = f
    .store
    .publish_phase_ready(policy.clone(), input.clone())
    .await
    .unwrap();
  assert_eq!(entry.input.phase_input_digest, handoff);
  let mut substituted = input;
  substituted.phase_input_digest = done.result.digest().unwrap();
  assert!(f.store.publish_phase_ready(policy, substituted).await.is_err());
  let next_claim_at = snapshot
    .current_claim
    .as_ref()
    .unwrap()
    .claim
    .expires_at()
    .unix_millis()
    + 1;
  let request = SelectPhasePool {
    policy_digest: entry.policy_digest,
    request_id: digest(99),
    owner: key("phase.worker"),
    observed_at: fixtures::time(next_claim_at),
    expires_at: fixtures::time(next_claim_at + 100),
    capabilities: Default::default(),
    limit: 1,
  };
  let selected = f.store.select_phase_ready(request.clone()).await.unwrap();
  assert_eq!(selected.len(), 1);
  assert_eq!(selected[0].entry, entry);
  assert_eq!(f.store.select_phase_ready(request).await.unwrap(), selected);
  assert!(snapshot.stage_attempts.is_empty());
  assert_eq!(snapshot.run.state(), FactoryRunState::Admitted);
}

pub(super) fn defect_outputs(
  intent: &ResearchBuildIntent,
  policy: &ResearchPolicy,
  builds: &build_fixture::Builds,
  outcome: DefectResearchOutcome,
) -> FactoryResearchBuildOutputs {
  use DefectReproductionObservation::{MissingInput, NotReproduced, Reproduced};
  let ResearchDetails::Defect { environment, .. } = intent.input().details() else {
    unreachable!()
  };
  let observations = match outcome {
    DefectResearchOutcome::Reproduced => vec![(false, Reproduced)],
    DefectResearchOutcome::Intermittent => vec![(false, Reproduced), (false, NotReproduced)],
    DefectResearchOutcome::EnvironmentSpecific => vec![(false, NotReproduced), (true, Reproduced)],
    DefectResearchOutcome::CannotReproduce => vec![(false, NotReproduced)],
    DefectResearchOutcome::NeedsHumanInput => vec![(false, MissingInput)],
  };
  build_fixture::observed_outputs(
    intent,
    policy,
    builds,
    observations
      .into_iter()
      .map(|(alternate, observation)| DefectReproductionCheck {
        environment_digest: if alternate {
          digest(98)
        } else {
          environment.content_digest()
        },
        observation,
      })
      .collect(),
    outcome,
  )
}
