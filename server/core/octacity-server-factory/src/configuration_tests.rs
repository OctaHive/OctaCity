use octacity_server_domain::{
  ArtifactId, BuildConfigurationId, BuildConfigurationVersion, ImmutableRevision, ProjectId, RepositoryId, Timestamp,
};

use crate::{
  AdmittedFlow, BudgetLimit, BuildConfigurationRef, DecisionSignalChoices, DecisionSignalFallback, DecisionSignalMode,
  DecisionSignalProfileDefinition, DecisionSignalPurpose, DecisionSignalRouteSet, ExactSubject, ExternalWorkIdentity,
  FactoryArtifactReference, FactoryChoiceKind, FactoryClaim, FactoryClaimFence, FactoryClaimOwnership,
  FactoryConfiguration, FactoryConfigurationChoiceEntries, FactoryConfigurationChoices, FactoryConfigurationDraft,
  FactoryConfigurationId, FactoryConfigurationVersion, FactoryDigest, FactoryError, FactoryKey, FactoryMetadata,
  FactoryPermissionSet, FactoryReferenceChoice, FactoryRun, FactoryRunId, FactoryStageKind, FactoryWipLimits,
  FlowAdmissionLimits, FlowNodeKind, ImmutableReference, NodeAttempt, NodeAttemptId, NodeAttemptInput,
  NodeAttemptNumber, NodeExecutionIdentity, RiskClass, WorkArtifacts, WorkClassification, WorkEnvelope, WorkEnvelopeId,
  WorkPriority,
};

struct Fixture {
  project_id: ProjectId,
  choices: FactoryConfigurationChoices,
  draft: FactoryConfigurationDraft,
}

fn budget(attempts: u32, elapsed: u64, tokens: u64, cost: u64, output: u64) -> BudgetLimit {
  match BudgetLimit::new(attempts, elapsed, tokens, cost, output) {
    Ok(value) => value,
    Err(_) => panic!("test budget must be valid"),
  }
}

fn hard_budget() -> BudgetLimit {
  budget(20, 20_000, 2_000, 20_000, 20_000)
}

fn nested_budget() -> BudgetLimit {
  budget(4, 4_000, 400, 4_000, 4_000)
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).expect("fixture key is valid")
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}

fn artifact(value: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), digest(value), 1).expect("fixture artifact")
}

fn exact(identity: &str, version: &str, digest_byte: u8) -> ImmutableReference {
  ImmutableReference::new(key(identity), key(version), digest(digest_byte))
}

fn stage(stage_key: &str, kind: FactoryStageKind, build_alias: &str) -> crate::testing::FactoryStageDraft {
  crate::testing::FactoryStageDraft {
    key: key(stage_key),
    kind,
    build_configuration: key(build_alias),
    budget: nested_budget(),
  }
}

fn choice_entries(
  project_id: ProjectId,
  signal_provider: &str,
  signal_model_version: &str,
) -> FactoryConfigurationChoiceEntries {
  FactoryConfigurationChoiceEntries {
    references: vec![
      reference_choice(
        FactoryChoiceKind::AdmissionPolicy,
        "admission-default",
        exact("manual.medium-risk", "v2", 12),
      ),
      reference_choice(
        FactoryChoiceKind::PermissionCeiling,
        "permissions-default",
        exact("permissions.restricted", "v3", 2),
      ),
      reference_choice(
        FactoryChoiceKind::DecisionSignalProvider,
        "signal-provider-default",
        exact(signal_provider, "v1", 3),
      ),
      reference_choice(
        FactoryChoiceKind::DecisionSignalAdapter,
        "signal-adapter-default",
        exact("signal.protocol", "v1", 13),
      ),
      reference_choice(
        FactoryChoiceKind::DecisionSignalModel,
        "signal-model-default",
        exact("routing-model", signal_model_version, 4),
      ),
      reference_choice(
        FactoryChoiceKind::DecisionSignalQuestionSet,
        "questions-default",
        exact("routing.questions", "v2", 5),
      ),
      reference_choice(
        FactoryChoiceKind::DecisionSignalPolicy,
        "signal-policy-default",
        exact("routing.policy", "v4", 6),
      ),
      reference_choice(
        FactoryChoiceKind::CriterionPack,
        "criteria-default",
        exact("quality.standard", "v7", 7),
      ),
      reference_choice(
        FactoryChoiceKind::Evaluator,
        "evaluator-default",
        exact("codex.review", "v5", 8),
      ),
      reference_choice(
        FactoryChoiceKind::DeliveryAdapter,
        "delivery-adapter-default",
        exact("github.review", "v2", 9),
      ),
      reference_choice(
        FactoryChoiceKind::DeliveryPolicy,
        "delivery-policy-default",
        exact("human-review", "v1", 10),
      ),
    ],
    build_configurations: vec![(
      key("build-default"),
      BuildConfigurationRef::new(
        BuildConfigurationId::generate(),
        BuildConfigurationVersion::INITIAL,
        project_id,
        digest(1),
      ),
    )],
  }
}

fn reference_choice(kind: FactoryChoiceKind, alias: &str, reference: ImmutableReference) -> FactoryReferenceChoice {
  FactoryReferenceChoice {
    kind,
    alias: key(alias),
    reference,
  }
}

fn valid_fixture() -> Fixture {
  valid_fixture_for_project(ProjectId::generate())
}
fn valid_fixture_for_project(project_id: ProjectId) -> Fixture {
  let choices = FactoryConfigurationChoices::try_new(choice_entries(project_id, "jev", "2026-10"))
    .expect("fixture choices are valid");
  let draft = FactoryConfigurationDraft {
    flow: None,
    admission_policy: key("admission-default"),
    wip_limits: FactoryWipLimits::new(8, 16).expect("fixture WIP limits are valid"),
    hard_budget: hard_budget(),
    permission_ceiling: key("permissions-default"),
    enabled: true,
  };
  let draft = crate::testing::with_contract_flow_with_rework_limit(
    draft,
    vec![
      stage("implement", FactoryStageKind::Implementation, "build-default"),
      stage("validate", FactoryStageKind::Validation, "build-default"),
      stage("evaluate", FactoryStageKind::Evaluation, "build-default"),
      stage("rework", FactoryStageKind::Rework, "build-default"),
    ],
    &choices,
    2,
  );
  Fixture {
    project_id,
    choices,
    draft,
  }
}

fn publish(fixture: &Fixture) -> FactoryConfiguration {
  FactoryConfiguration::publish(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    fixture.project_id,
    digest(11),
    fixture.draft.clone(),
    &fixture.choices,
  )
  .expect("fixture configuration is valid")
}

fn admitted_run(fixture: &Fixture, configuration: &FactoryConfiguration) -> FactoryRun {
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    configuration.reference().clone(),
    ExternalWorkIdentity::new("tracker:flow-1").expect("external identity"),
    ExactSubject::new(
      fixture.project_id,
      RepositoryId::generate(),
      ImmutableRevision::new("0123456789abcdef").expect("revision"),
    ),
    WorkArtifacts::new(artifact(90), artifact(91), vec![]).expect("work artifacts"),
    WorkClassification::new(
      WorkPriority::new(10).expect("priority"),
      RiskClass::Medium,
      FactoryMetadata::default(),
    ),
  )
  .expect("work");
  FactoryRun::admitted(FactoryRunId::generate(), &work)
}

#[test]
fn publication_resolves_every_alias_to_an_exact_identity() {
  let fixture = valid_fixture();
  let configuration = publish(&fixture);

  assert_eq!(
    configuration.admission_policy().identity().as_str(),
    "manual.medium-risk"
  );
  assert_eq!(
    configuration.permission_ceiling().identity().as_str(),
    "permissions.restricted"
  );
  assert!(configuration.is_enabled());
}

#[test]
fn admission_retains_the_explicit_pinned_flow_without_inventing_edges() {
  let fixture = valid_fixture();
  let configuration = publish(&fixture);
  let run = admitted_run(&fixture, &configuration);
  let admitted = AdmittedFlow::from_configuration(&configuration, &run).expect("Flow projection");
  let definition = &admitted.closure().definitions()[0];

  assert_eq!(admitted.closure().definitions().len(), 1);
  assert_eq!(definition.reference(), admitted.closure().root());
  assert_eq!(definition.entry().as_str(), "implement");
  assert_eq!(definition.nodes().len(), 4);
  assert!(
    definition
      .nodes()
      .iter()
      .all(|node| node.kind() == FlowNodeKind::TrustedAction)
  );
  assert_eq!(admitted.root_run().factory_run_id(), run.id());
  assert_eq!(admitted.initial_cycle().flow_run_id(), admitted.root_run().id());
}

#[test]
fn admitted_flow_round_trip_retains_the_exact_configured_limits() {
  let fixture = valid_fixture();
  let configuration = publish(&fixture);
  let run = admitted_run(&fixture, &configuration);
  let projected = AdmittedFlow::from_configuration(&configuration, &run).unwrap();
  let root = projected.closure().definition(projected.closure().root()).unwrap();
  let expanded_nodes = projected.validated().unwrap().expanded_nodes();
  let exact = FlowAdmissionLimits::new(
    1,
    expanded_nodes,
    12,
    4,
    root.execution().max_active_nodes(),
    root.execution().budget(),
    FactoryPermissionSet::deny_all(),
  )
  .unwrap();
  let admitted = AdmittedFlow::new(
    projected.closure().clone(),
    exact.clone(),
    projected.root_run().clone(),
    projected.initial_cycle().clone(),
    projected.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(projected.data_schemas().to_vec())
  .unwrap();
  let restored: AdmittedFlow = serde_json::from_slice(&serde_json::to_vec(&admitted).unwrap()).unwrap();

  assert_eq!(restored.limits(), &exact);
  assert_eq!(restored.validated().unwrap().limits(), &exact);
}

#[test]
fn flow_closure_accepts_only_the_closed_primitive_set_and_exact_subflow_versions() {
  let fixture = valid_fixture();
  let configuration = publish(&fixture);
  let run = admitted_run(&fixture, &configuration);
  let admitted = AdmittedFlow::from_configuration(&configuration, &run).expect("Flow projection");
  let root = &admitted.closure().definitions()[0];
  let flow_run = admitted.root_run();
  let cycle = admitted.initial_cycle();
  let node = root.node(root.entry()).expect("entry node");
  let ownership = FactoryClaimOwnership::new(
    key("worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(digest(40)),
      Timestamp::from_unix_millis(1).unwrap(),
      Timestamp::from_unix_millis(100).unwrap(),
    )
    .unwrap(),
  );
  let attempt = NodeAttempt::new(
    flow_run,
    cycle,
    root,
    NodeAttemptInput {
      started_at: (ownership).claim().claimed_at(),
      id: NodeAttemptId::generate(),
      node_key: node.key().clone(),
      node_kind: node.kind(),
      number: NodeAttemptNumber::INITIAL,
      input_digest: digest(42),
      budget: node.budget(),
      deadline: Timestamp::from_unix_millis(99).unwrap(),
      execution: NodeExecutionIdentity::External(exact("build-executor", "v1", 43)),
      ownership,
    },
  )
  .expect("generic Node Attempt");

  assert_eq!(admitted.closure().definitions().len(), 1);
  assert_eq!(attempt.node_kind(), FlowNodeKind::TrustedAction);
}

#[test]
fn node_attempt_preserves_the_supplied_identity_number_and_frozen_input() {
  let (_, admitted, _, input, attempt) = crate::flow_test_support::execution_fixture(FlowNodeKind::BuildCommand);
  let id = NodeAttemptId::generate();
  let declared = &admitted.closure().definitions()[0];
  let row = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    declared,
    NodeAttemptInput {
      started_at: attempt.started_at(),
      id,
      node_key: attempt.node_key().clone(),
      node_kind: attempt.node_kind(),
      number: NodeAttemptNumber::new(7).unwrap(),
      input_digest: input.digest().unwrap(),
      budget: attempt.budget(),
      deadline: attempt.deadline(),
      execution: attempt.execution().clone(),
      ownership: FactoryClaimOwnership::new(attempt.owner().clone(), attempt.claim()),
    },
  )
  .unwrap();
  assert_eq!(row.id(), id);
  assert_eq!(row.number().get(), 7);
  assert_eq!(row.input_digest(), input.digest().unwrap());
  assert_eq!(row.factory_run_id(), admitted.root_run().factory_run_id());
}

#[test]
fn choice_catalogs_reject_duplicates_and_publication_rejects_unknown_aliases() {
  let duplicate = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: vec![
      reference_choice(FactoryChoiceKind::PermissionCeiling, "default", exact("one", "v1", 1)),
      reference_choice(FactoryChoiceKind::PermissionCeiling, "default", exact("two", "v1", 2)),
    ],
    ..FactoryConfigurationChoiceEntries::default()
  });
  assert_eq!(
    duplicate,
    Err(FactoryError::DuplicateChoiceAlias {
      kind: FactoryChoiceKind::PermissionCeiling,
    })
  );

  let mut fixture = valid_fixture();
  fixture.draft.permission_ceiling = key("missing");
  assert_eq!(
    FactoryConfiguration::publish(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      fixture.project_id,
      digest(11),
      fixture.draft,
      &fixture.choices,
    ),
    Err(FactoryError::UnknownChoiceAlias {
      kind: FactoryChoiceKind::PermissionCeiling,
    })
  );
}

#[test]
fn evaluation_policy_reject_ambiguous_or_unsafe_values() {
  assert_eq!(
    crate::EvaluationPolicy::try_new(
      vec![exact("quality", "v1", 7)],
      vec![exact("reviewer", "v1", 8)],
      2,
      nested_budget()
    ),
    Err(FactoryError::InvalidConfiguration {
      field: "evaluator quorum"
    }),
  );
}

#[test]
fn wip_and_replacement_versions_are_strictly_bounded() {
  assert_eq!(
    FactoryWipLimits::new(0, 1),
    Err(FactoryError::InvalidConfiguration {
      field: "max_active_runs",
    })
  );
  let fixture = valid_fixture();
  let configuration = publish(&fixture);
  assert_eq!(
    configuration.replace(
      FactoryConfigurationVersion::new(3).expect("version is valid"),
      digest(12),
      fixture.draft,
      &fixture.choices,
    ),
    Err(FactoryError::InvalidConfiguration {
      field: "replacement version",
    })
  );
}

#[test]
fn replacement_does_not_mutate_the_configuration_bound_to_an_admitted_run() {
  let fixture = valid_fixture();
  let original = publish(&fixture);
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    original.reference().clone(),
    ExternalWorkIdentity::new("tracker:work-42").expect("external identity is valid"),
    ExactSubject::new(
      fixture.project_id,
      RepositoryId::generate(),
      ImmutableRevision::new("0123456789abcdef").expect("revision is valid"),
    ),
    WorkArtifacts::new(artifact(90), artifact(91), vec![]).expect("work artifacts are distinct"),
    WorkClassification::new(
      WorkPriority::new(10).expect("priority is valid"),
      RiskClass::Medium,
      FactoryMetadata::default(),
    ),
  )
  .expect("work fixture is valid");
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);

  let mut replacement_draft = fixture.draft.clone();
  replacement_draft.enabled = false;
  let replacement_choices =
    FactoryConfigurationChoices::try_new(choice_entries(fixture.project_id, "decisions-api", "2027-01"))
      .expect("replacement choices are valid");
  let replacement = original
    .replace(
      FactoryConfigurationVersion::new(2).expect("version is valid"),
      digest(12),
      replacement_draft,
      &replacement_choices,
    )
    .expect("replacement configuration is valid");

  assert_eq!(original.reference().version(), FactoryConfigurationVersion::INITIAL);
  assert_eq!(original.reference().definition_digest(), digest(11));
  assert!(original.is_enabled());
  assert_eq!(run.configuration(), original.reference());
  assert_eq!(replacement.reference().version().get(), 2);
  assert_eq!(replacement.reference().definition_digest(), digest(12));
  assert!(!replacement.is_enabled());
}

#[test]
fn publication_requires_an_explicit_flow_instead_of_inventing_a_stage_journey() {
  let mut fixture = valid_fixture();
  fixture.draft.flow = None;
  assert!(matches!(
    FactoryConfiguration::publish(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      fixture.project_id,
      digest(11),
      fixture.draft,
      &fixture.choices,
    ),
    Err(FactoryError::InvalidConfiguration { field: "required Flow" })
  ));
}

#[test]
fn an_explicit_custom_flow_needs_no_fixed_stage_roles() {
  let mut fixture = valid_fixture();
  let (_, admitted, schema, _, _) = crate::flow_test_support::execution_fixture(FlowNodeKind::BuildCommand);
  fixture.draft.flow = Some(
    crate::FactoryFlowConfiguration::new(
      admitted.closure().clone(),
      admitted.limits().clone(),
      vec![schema],
      Default::default(),
    )
    .unwrap(),
  );
  let published = FactoryConfiguration::publish(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    fixture.project_id,
    digest(11),
    fixture.draft,
    &fixture.choices,
  )
  .expect("an explicit operator flow must not require unrelated fixed roles");
  assert_eq!(published.flow().closure.root(), admitted.closure().root());
}

#[test]
fn a_simple_flow_publishes_without_candidate_review_or_delivery_configuration() {
  let mut fixture = valid_fixture();
  let (_, admitted, schema, _, _) = crate::flow_test_support::execution_fixture(FlowNodeKind::BuildCommand);
  fixture.draft.flow = Some(
    crate::FactoryFlowConfiguration::new(
      admitted.closure().clone(),
      admitted.limits().clone(),
      vec![schema],
      Default::default(),
    )
    .unwrap(),
  );
  let mut document = serde_json::to_value(&fixture.draft).unwrap();
  for field in ["stages", "evaluation", "rework", "delivery", "credential_profiles"] {
    document.as_object_mut().unwrap().remove(field);
  }
  let draft: FactoryConfigurationDraft =
    serde_json::from_value(document).expect("a generic Flow has no mandatory candidate-workflow configuration");
  let published = FactoryConfiguration::publish(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    fixture.project_id,
    digest(11),
    draft,
    &fixture.choices,
  )
  .expect("only explicitly selected capabilities require their configuration");
  assert_eq!(published.flow().closure.root(), admitted.closure().root());
}

#[test]
fn runtime_admission_requires_the_exact_data_catalogue_even_before_execution() {
  let (_, admitted, _, _, _) = crate::flow_test_support::execution_fixture(FlowNodeKind::BuildCommand);
  let incomplete = AdmittedFlow::new(
    admitted.closure().clone(),
    admitted.limits().clone(),
    admitted.root_run().clone(),
    admitted.initial_cycle().clone(),
    admitted.wip_limits(),
  )
  .unwrap();
  assert!(
    incomplete.validated().is_err(),
    "a graph without its pinned schemas cannot become a runtime admission"
  );
}

#[test]
fn a_node_contract_cannot_enable_a_legacy_stage_projection() {
  let (_, admitted, _, _, _) = crate::flow_test_support::execution_fixture(FlowNodeKind::BuildCommand);
  let node = &admitted.closure().definitions()[0].nodes()[0];
  let mut document = serde_json::to_value(node).unwrap();
  document["stage_projection"] = serde_json::json!("implementation");
  assert!(
    serde_json::from_value::<crate::FlowNodeDefinition>(document).is_err(),
    "node declarations must not enable alternate stage readiness or journal validation"
  );
}

#[test]
fn attempts_use_the_common_observer_protocol_after_fence_replacement() {
  let (_, _, _, _, attempt) = crate::flow_test_support::execution_fixture(FlowNodeKind::BuildCommand);
  let at = |value| Timestamp::from_unix_millis(value).unwrap();
  let replaced = FactoryClaimOwnership::new(
    key("worker"),
    FactoryClaim::new(FactoryClaimFence::new(digest(33)), at(11), at(101)).unwrap(),
  );
  attempt
    .verify_observer(&replaced, at(12))
    .expect("the durable owner still checks the current stored fence");
  let overlapping = FactoryClaimOwnership::new(key("unrelated"), replaced.claim());
  assert!(attempt.verify_observer(&overlapping, at(12)).is_err());
  let successor = FactoryClaimOwnership::new(
    key("successor"),
    FactoryClaim::new(FactoryClaimFence::new(digest(34)), at(100), at(110)).unwrap(),
  );
  assert!(attempt.verify_observer(&successor, at(101)).is_ok());
}

#[test]
fn a_published_configuration_cannot_restore_without_its_explicit_flow() {
  let configuration = publish(&valid_fixture());
  let mut wire = serde_json::to_value(configuration).unwrap();
  wire.as_object_mut().unwrap().remove("flow");
  assert!(serde_json::from_value::<FactoryConfiguration>(wire).is_err());
}

#[test]
fn common_configuration_does_not_publish_unused_capability_defaults() {
  let configuration = publish(&valid_fixture());
  let wire = serde_json::to_value(&configuration).unwrap();
  for field in [
    "evaluation",
    "delivery",
    "credential_profiles",
    "rework",
    "decision_signals",
    "stages",
  ] {
    assert!(
      wire.get(field).is_none(),
      "{field} belongs to a selected operation, not the common configuration"
    );
  }
}

#[test]
fn a_selected_signal_profile_validates_its_own_route_contract() {
  let mut definition = DecisionSignalProfileDefinition {
    purpose: DecisionSignalPurpose::Routing,
    provider: exact("jev", "v1", 3),
    adapter: exact("protocol", "v1", 13),
    model: exact("routing-model", "2026-10", 4),
    question_set: exact("routing.questions", "v2", 5),
    policy: exact("routing.policy", "v4", 6),
    mode: DecisionSignalMode::Shadow,
    fallback: DecisionSignalFallback::Escalate,
    budget: nested_budget(),
    routes: None,
  };
  assert_eq!(
    crate::DecisionSignalProfile::new(definition.clone()),
    Err(FactoryError::InvalidConfiguration {
      field: "decision_signal_routes"
    })
  );
  definition.routes = Some(DecisionSignalRouteSet::new(
    key("review-complete"),
    DecisionSignalChoices::try_new(vec![key("continue"), key("ask-owner")]).unwrap(),
  ));
  let profile = crate::DecisionSignalProfile::new(definition).unwrap();
  assert_eq!(profile.provider().identity().as_str(), "jev");
  assert_eq!(profile.model().version().as_str(), "2026-10");
  assert_eq!(profile.question_set().digest(), digest(5));
  assert_eq!(profile.policy().digest(), digest(6));
  assert!(profile.routes().is_some());
}
#[test]
fn common_wip_limits_address_nodes_without_a_stage_catalogue() {
  let limits = crate::FactoryWipLimits::new(3, 7).unwrap();
  assert_eq!(
    serde_json::to_value(limits).unwrap(),
    serde_json::json!({"max_active_runs": 3, "max_active_nodes": 7})
  );
}

#[test]
fn configuration_budget_and_wip_wire_values_cannot_bypass_constructor_bounds() {
  let valid_budget = serde_json::json!({
    "max_attempts":1,"max_elapsed_millis":1,"max_tokens":1,
    "max_cost_micro_units":1,"max_output_bytes":1,
  });
  assert!(serde_json::from_value::<BudgetLimit>(valid_budget.clone()).is_ok());
  for (field, value) in [
    ("max_attempts", 0u64),
    ("max_attempts", 1001),
    ("max_elapsed_millis", 0),
    ("max_tokens", 1000000001),
    ("max_cost_micro_units", 1000000000001),
    ("max_output_bytes", 1099511627777),
  ] {
    let mut invalid = valid_budget.clone();
    invalid[field] = serde_json::json!(value);
    assert!(
      serde_json::from_value::<BudgetLimit>(invalid).is_err(),
      "{field}={value}"
    );
  }
  for invalid in [
    serde_json::json!({"max_active_runs":0,"max_active_nodes":1}),
    serde_json::json!({"max_active_runs":10001,"max_active_nodes":1}),
    serde_json::json!({"max_active_runs":1,"max_active_nodes":0}),
    serde_json::json!({"max_active_runs":1,"max_active_nodes":1001}),
  ] {
    assert!(serde_json::from_value::<FactoryWipLimits>(invalid).is_err());
  }
  let valid_wip = FactoryWipLimits::new(1, 1).unwrap();
  assert_eq!(
    serde_json::from_value::<FactoryWipLimits>(serde_json::to_value(valid_wip).unwrap()).unwrap(),
    valid_wip
  );
}

#[test]
fn flow_admission_wire_limits_and_closure_preserve_constructor_invariants() {
  let limits = FlowAdmissionLimits::new(1, 1, 1, 1, 1, hard_budget(), FactoryPermissionSet::deny_all()).unwrap();
  let valid = serde_json::to_value(&limits).unwrap();
  assert_eq!(
    serde_json::from_value::<FlowAdmissionLimits>(valid.clone()).unwrap(),
    limits
  );
  for (field, value) in [
    ("max_depth", 17),
    ("max_expanded_nodes", 16385),
    ("max_repeat_count", 33),
    ("max_fan_out", 65),
    ("max_wip", 1001),
  ] {
    let mut invalid = valid.clone();
    invalid[field] = serde_json::json!(value);
    assert!(
      serde_json::from_value::<FlowAdmissionLimits>(invalid).is_err(),
      "{field}={value}"
    );
  }
  let execution = crate::FlowExecutionPolicy::new(hard_budget(), FactoryPermissionSet::deny_all(), 1).unwrap();
  let mut invalid = serde_json::to_value(execution).unwrap();
  invalid["max_active_nodes"] = serde_json::json!(0);
  assert!(serde_json::from_value::<crate::FlowExecutionPolicy>(invalid).is_err());
  let fixture = valid_fixture();
  let flow = fixture.draft.flow.unwrap();
  let mut invalid = serde_json::to_value(&flow.closure).unwrap();
  let duplicate = invalid["definitions"][0].clone();
  invalid["definitions"].as_array_mut().unwrap().push(duplicate);
  assert!(serde_json::from_value::<crate::PinnedFlowDefinitionClosure>(invalid).is_err());
}

#[test]
fn zero_consumable_budgets_allow_control_work_but_deny_any_consumption() {
  let limit = BudgetLimit::new(1, 1000, 0, 0, 0).expect("control nodes need no model or output allowance");
  crate::BudgetUsage::default().validate(limit).unwrap();
  for usage in [
    crate::BudgetUsage {
      tokens: 1,
      ..Default::default()
    },
    crate::BudgetUsage {
      cost_micro_units: 1,
      ..Default::default()
    },
    crate::BudgetUsage {
      output_bytes: 1,
      ..Default::default()
    },
  ] {
    assert!(usage.validate(limit).is_err());
  }
  assert_eq!(
    serde_json::from_value::<BudgetLimit>(serde_json::to_value(limit).unwrap()).unwrap(),
    limit
  );
}
