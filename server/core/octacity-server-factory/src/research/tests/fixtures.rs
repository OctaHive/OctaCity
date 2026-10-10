use super::factory::*;
use octacity_server_domain::{ArtifactId, BuildConfigurationId, BuildConfigurationVersion, Timestamp};
use uuid::Uuid;

pub(crate) fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

pub(crate) fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}

pub(crate) fn artifact(value: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), digest(value), 10).unwrap()
}

pub(crate) fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

pub(crate) fn reference(name: &str) -> ImmutableReference {
  ImmutableReference::new(key(name), key("v1"), digest(1))
}

pub(crate) fn research_budget() -> BudgetLimit {
  BudgetLimit::new(3, 3000, 300, 3000, 3000).unwrap()
}

pub(crate) fn attempt_budget() -> BudgetLimit {
  BudgetLimit::new(1, 1000, 100, 1000, 1000).unwrap()
}

pub(crate) fn research_closure(kind: WorkKind, routes: &[ResearchRoute]) -> ValidatedFlowDefinitionClosure {
  research_closure_with_budget(kind, routes, research_budget(), attempt_budget())
}
pub(crate) fn research_closure_with_budget(
  kind: WorkKind,
  routes: &[ResearchRoute],
  budget: BudgetLimit,
  attempt: BudgetLimit,
) -> ValidatedFlowDefinitionClosure {
  research_closure_for_node(kind, routes, budget, attempt, FlowNodeKind::Reasoning)
}

pub(crate) fn research_closure_for_node(
  kind: WorkKind,
  routes: &[ResearchRoute],
  budget: BudgetLimit,
  attempt: BudgetLimit,
  node_kind: FlowNodeKind,
) -> ValidatedFlowDefinitionClosure {
  let result_schema = ResearchSchema::result(kind).reference().unwrap();
  let decision_schema = ResearchSchema::Decision.reference().unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::from_uuid(Uuid::from_u128(if kind == WorkKind::Defect { 200 } else { 201 })).unwrap(),
    version: FlowDefinitionVersion::new(1).unwrap(),
    input_schema: Some(ResearchSchema::Input.reference().unwrap()),
    entry: key("research"),
    nodes: vec![
      FlowNodeDefinition::new(FlowNodeDefinitionInput {
        key: key("research"),
        kind: node_kind,
        input_schema: Some(ResearchSchema::Input.reference().unwrap()),
        outcomes: vec![FlowOutcomeDefinition::new(
          key("observed"),
          FlowOutcomeKind::Success,
          result_schema.clone(),
        )],
        budget: attempt,
        permissions: FactoryPermissionSet::deny_all(),
        required: true,
        subflow: None,
      })
      .unwrap(),
      FlowNodeDefinition::new(FlowNodeDefinitionInput {
        key: key("research_policy"),
        kind: FlowNodeKind::DeterministicGate,
        input_schema: Some(result_schema.clone()),
        outcomes: routes
          .iter()
          .map(|route| {
            FlowOutcomeDefinition::new(key(route.as_str()), FlowOutcomeKind::Success, decision_schema.clone())
          })
          .collect(),
        budget: attempt,
        permissions: FactoryPermissionSet::deny_all(),
        required: true,
        subflow: None,
      })
      .unwrap(),
    ],
    transitions: std::iter::once(FlowTransition::new(
      key("research"),
      key("observed"),
      FlowTransitionTarget::Node(key("research_policy")),
    ))
    .chain(routes.iter().map(|route| {
      FlowTransition::new(
        key("research_policy"),
        key(route.as_str()),
        FlowTransitionTarget::Terminal(key(route.as_str())),
      )
    }))
    .collect(),
    terminals: routes
      .iter()
      .map(|route| FlowTerminalDefinition::new(key(route.as_str()), decision_schema.clone()))
      .collect(),
    context_projections: vec![],
    data_projections: vec![FlowDataProjection::new(
      key("research"),
      key("observed"),
      key("research_policy"),
      result_schema,
    )],
    execution: FlowExecutionPolicy::new(budget, FactoryPermissionSet::deny_all(), 1).unwrap(),
  })
  .unwrap();
  PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition])
    .unwrap()
    .validate(FlowAdmissionLimits::product_defaults(1, budget, FactoryPermissionSet::deny_all()).unwrap())
    .unwrap()
}

pub(crate) fn profiles(
  closure: &ValidatedFlowDefinitionClosure,
  project: octacity_server_domain::ProjectId,
) -> Vec<FlowBuildProfile> {
  vec![FlowBuildProfile {
    definition: closure.closure().root(),
    node: key("research"),
    tool: reference("research-tool"),
    plugin: reference("runner"),
    model_or_tool: reference("research-model"),
    task_digest: digest(70),
    build_configuration: BuildConfigurationRef::new(
      BuildConfigurationId::from_uuid(Uuid::from_u128(202)).unwrap(),
      BuildConfigurationVersion::new(1).unwrap(),
      project,
      digest(75),
    ),
    result_output: key("research.result"),
  }]
}

pub(crate) fn settings(kind: WorkKind) -> ResearchPolicySettings {
  let outcomes = if kind == WorkKind::Defect {
    [
      (DefectResearchOutcome::Reproduced, ResearchRoute::ProtectedTest),
      (DefectResearchOutcome::Intermittent, ResearchRoute::Requirements),
      (DefectResearchOutcome::EnvironmentSpecific, ResearchRoute::Verification),
      (DefectResearchOutcome::CannotReproduce, ResearchRoute::Rejection),
      (DefectResearchOutcome::NeedsHumanInput, ResearchRoute::Escalation),
    ]
    .into_iter()
    .map(|(outcome, route)| (ResearchOutcome::Defect(outcome), route))
    .collect()
  } else {
    [(ResearchOutcome::FeatureProposal, ResearchRoute::Requirements)]
      .into_iter()
      .collect()
  };
  ResearchPolicySettings {
    budget: research_budget(),
    attempt_budget: attempt_budget(),
    permissions: FactoryPermissionSet::deny_all(),
    small_work_max_risk: RiskClass::Low,
    max_proposal_bytes: 128,
    outcomes,
    exhausted_route: ResearchRoute::Escalation,
    evidence: [
      ResearchEvidenceKind::Reproduction,
      ResearchEvidenceKind::Proposal,
      ResearchEvidenceKind::Verification,
      ResearchEvidenceKind::TerminalResolution,
    ]
    .into_iter()
    .map(|kind| {
      (
        kind,
        EvidenceRequirement::new(
          key("research-evidence"),
          EvidenceOutputKind::Report,
          reference("research-report"),
          reference("validator"),
          reference("runner"),
        ),
      )
    })
    .collect(),
  }
}

pub(crate) fn input(
  work: &WorkEnvelope,
  admitted: &AdmittedFlow,
  accepted: ClassificationReceipt,
  kind: WorkKind,
) -> ResearchInput {
  let subject = FactoryTaskSubject::Exact(work.subject().clone());
  let symptoms = artifact(50);
  let environment = artifact(51);
  let source = artifact(52);
  let eligibility = &accepted.eligibility.input;
  let artifacts = [
    eligibility.task().clone(),
    eligibility.acceptance().clone(),
    accepted.result.provenance().result.clone(),
    eligibility.project_goals().clone(),
    symptoms.clone(),
    environment.clone(),
    source.clone(),
  ];
  let entries = artifacts
    .into_iter()
    .enumerate()
    .map(|(index, artifact)| {
      ContextManifestEntry::new(
        ContextSourceKind::Task,
        key(&format!("input.{index}")),
        subject.clone(),
        FactoryContextReference::Artifact(artifact.clone()),
        artifact.content_digest(),
        artifact.encoded_size(),
        FactorySafeText::new("Declared research input").unwrap(),
        digest(60),
      )
      .unwrap()
    })
    .collect();
  let context = ContextManifest::new(ContextManifestId::generate(), subject, digest(61), entries).unwrap();
  let details = if kind == WorkKind::Defect {
    ResearchDetails::Defect {
      symptoms,
      environment,
      prior_observations: vec![],
    }
  } else {
    ResearchDetails::Feature {
      project_goals: eligibility.project_goals().clone(),
      sources: vec![ResearchSourceReference {
        source_kind: ContextSourceKind::Task,
        logical_identity: key("input.6"),
        content_digest: source.content_digest(),
      }],
    }
  };
  let closure = research_closure(kind, &ResearchRoute::ALL);
  let selected = ResearchPolicy::new(
    work.configuration().clone(),
    &closure,
    kind,
    settings(kind),
    profiles(&closure, work.subject().project_id()),
  )
  .unwrap();
  ResearchInput::new(
    work,
    admitted,
    accepted,
    context,
    vec![],
    details,
    selected.digest().unwrap(),
  )
  .unwrap()
}
