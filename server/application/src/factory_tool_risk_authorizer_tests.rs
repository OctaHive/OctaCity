//! Public immutable installation contracts for lease-bound tool-risk signals.
use crate::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;

#[test]
fn tool_risk_policy_pin_versions_provider_authority_and_finite_call_allowance() {
  let binding = binding_fixture();
  let original = binding.policy_digest().unwrap();
  let base = FactoryDigest::sha256("test.task", &[b"operator task"]);
  let task = factory_tool_risk_task_digest(base, original);
  assert_ne!(task, base);
  assert_eq!(
    task,
    factory_tool_risk_task_digest(base, binding_fixture().policy_digest().unwrap())
  );

  let mut changed = binding.clone();
  changed.credential_profile = key("another-provider-credential");
  assert_ne!(original, changed.policy_digest().unwrap());
  let mut changed = binding.clone();
  changed.network_host = NetworkHost::new("other-provider.internal").unwrap();
  assert_ne!(original, changed.policy_digest().unwrap());
  let mut changed = binding.clone();
  changed.max_calls = 3;
  assert_ne!(original, changed.policy_digest().unwrap());
  assert_ne!(
    task,
    factory_tool_risk_task_digest(base, changed.policy_digest().unwrap())
  );
  let mut changed = binding.clone();
  changed.plan.threshold = DecisionSignalThreshold::new(
    DecisionSignalPurpose::ToolRisk,
    changed.plan.capability.probability_semantics().clone(),
    ImmutableReference::new(key("calibration"), key("v2"), FactoryDigest::from_bytes([72; 32])),
    DecisionSignalProbability::new(600_000).unwrap(),
    DecisionSignalProbability::new(100_000).unwrap(),
  )
  .unwrap();
  assert_ne!(original, changed.policy_digest().unwrap());
  let mut changed = binding.clone();
  changed.mapping = ToolRiskChoiceMapping::try_new(
    template().choices(),
    vec![
      (key("allow"), DecisionSignalDisposition::Deny),
      (key("deny"), DecisionSignalDisposition::Deny),
      (key("escalate"), DecisionSignalDisposition::Escalate),
    ],
  )
  .unwrap();
  assert_ne!(original, changed.policy_digest().unwrap());

  assert_eq!(
    binding.signal_allowance().unwrap(),
    BudgetLimit::new(2, 200, 20, 200, 200).unwrap()
  );
  let mut invalid = binding;
  invalid.max_calls = 0;
  assert!(invalid.policy_digest().is_err());
}

#[test]
fn tool_risk_request_binds_the_proposal_without_renewing_deadline_or_reading_task_context() {
  let (input, node) = crate::factory_tool_action_tests::node_fixture();
  let mut binding = binding_fixture();
  binding.project_id = input.subject().project_id();
  binding.max_calls = 1;
  let summary = octacity_protocol::FactoryToolActionSummaryV3 {
    argument_count: 1,
    read_path_count: 1,
    write_path_count: 0,
    network_host_count: 0,
    secret_profile_count: 0,
    workload_identity_profile_count: 0,
    descendants: 1,
    output_kind_count: 0,
  };
  let proposal = FactoryDigest::from_bytes([11; 32]);
  let prepared = binding.prepare_request(&input, &node, proposal, &summary).unwrap();
  let replay = binding.prepare_request(&input, &node, proposal, &summary).unwrap();
  assert_eq!(prepared, replay);
  assert_eq!(prepared.request().purpose(), DecisionSignalPurpose::ToolRisk);
  assert_eq!(prepared.request().frozen_input_digest(), input.digest().unwrap());
  assert_eq!(prepared.deadline(), node.deadline());
  assert_eq!(
    prepared.state_document().as_str(),
    "{}",
    "only the configured redacted summary projection belongs in provider input"
  );
  let other = binding
    .prepare_request(&input, &node, FactoryDigest::from_bytes([12; 32]), &summary)
    .unwrap();
  assert_ne!(prepared.id(), other.id());
  assert_ne!(prepared.digest(), other.digest());
}

pub(crate) fn binding_fixture() -> FactoryToolRiskBinding {
  let template = template();
  let profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
    purpose: DecisionSignalPurpose::ToolRisk,
    provider: template.provider().clone(),
    adapter: template.adapter().clone(),
    model: template.model().clone(),
    question_set: template.question_set().clone(),
    policy: template.policy().clone(),
    mode: DecisionSignalMode::Shadow,
    fallback: DecisionSignalFallback::Deny,
    budget: template.request().budget(),
    routes: None,
  })
  .unwrap();
  FactoryToolRiskBinding {
    project_id: ProjectId::from_uuid(uuid::Uuid::from_u128(71)).unwrap(),
    plan: FactoryDecisionSignalRequestPlan {
      profile,
      capability: crate::factory_decision_signal_tests::capability(&template),
      questions: template.input().questions().to_vec(),
      consuming_question: key("tool-risk"),
      threshold: template.threshold().clone(),
      state_schema: FlowDataSchema::new(
        key("tool-risk.state"),
        key("v1"),
        FlowValueSchema::Object {
          fields: std::collections::BTreeMap::new(),
        },
      )
      .unwrap(),
      state_projection: FlowDataMapping::new(FlowValueMapping::Constant {
        value: serde_json::json!({}),
      })
      .unwrap(),
      call_budget: BudgetLimit::new(1, 100, 10, 100, 100).unwrap(),
    },
    mapping: ToolRiskChoiceMapping::try_new(
      template.choices(),
      vec![
        (key("allow"), DecisionSignalDisposition::Allow),
        (key("deny"), DecisionSignalDisposition::Deny),
        (key("escalate"), DecisionSignalDisposition::Escalate),
      ],
    )
    .unwrap(),
    max_calls: 2,
    credential_profile: key("decision-provider"),
    network_host: NetworkHost::new("provider.internal").unwrap(),
    project_permissions: FactoryPermissionSet::deny_all(),
    local_permissions: LocalPermissionCeiling::fully_enforced(FactoryPermissionSet::deny_all()),
  }
}
fn template() -> DecisionSignalProviderRequest {
  crate::factory_tool_action_tests::signal_request(FactoryText::new("{}").unwrap(), DecisionSignalFallback::Deny)
}
fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
