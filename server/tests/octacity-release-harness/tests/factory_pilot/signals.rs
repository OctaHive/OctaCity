//! Operator-owned shadow declarations for the real HTTPS pilot connector.
use octacity_server_application::{
  FactoryDecisionSignalNodeBinding, FactoryDecisionSignalNodeOutput, FactoryDecisionSignalRequestPlan,
};
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
use serde_json::{Value, json};
fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn reference(value: &str, version: &str) -> ImmutableReference {
  ImmutableReference::new(
    key(value),
    key(version),
    FactoryDigest::sha256(
      "octacity.pilot.installed-signal.v1",
      &[value.as_bytes(), version.as_bytes()],
    ),
  )
}
pub fn plan(
  purpose: DecisionSignalPurpose,
  state_schema: FlowDataSchema,
  state_projection: FlowDataMapping,
  routes: &[FactoryKey],
) -> FactoryDecisionSignalRequestPlan {
  let budget = BudgetLimit::new(1, 10_000, 1_000, 1_000, 16 * 1024).unwrap();
  let choices = DecisionSignalChoices::try_new(routes.to_vec()).unwrap();
  let profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
    purpose,
    provider: reference("jev", "v1"),
    adapter: reference("jev-http", "v1"),
    model: reference("jev-1.13", "jev-1.13-20260917"),
    question_set: reference("pilot-public-choice", "v1"),
    policy: reference("pilot-shadow-policy", "v1"),
    mode: DecisionSignalMode::Shadow,
    fallback: DecisionSignalFallback::Escalate,
    budget,
    routes: (purpose == DecisionSignalPurpose::Routing)
      .then(|| DecisionSignalRouteSet::new(state_schema.reference().identity().clone(), choices.clone())),
  })
  .unwrap();
  let capability = DecisionSignalProviderCapability::new(
    profile.provider().clone(),
    profile.adapter().clone(),
    profile.model().clone(),
    reference("jev-choice-probability", "v1"),
    reference("pilot-redacted-only", "v1"),
    vec![DecisionSignalPurpose::Routing, DecisionSignalPurpose::ToolRisk],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(MAX_FACTORY_TEXT_BYTES as u32, 20, 24).unwrap(),
    budget,
  )
  .unwrap();
  FactoryDecisionSignalRequestPlan {
    profile,
    capability: capability.clone(),
    questions: vec![
      DecisionSignalQuestion::new(
        reference("pilot-choice", "v1"),
        FactoryText::new("Choose only among the declared labels using the explicitly projected public state.").unwrap(),
        DecisionSignalQuestionDomain::FiniteChoice(choices.clone()),
        DecisionSignalQuestionCriteria::FiniteChoice(
          choices
            .as_slice()
            .iter()
            .map(|choice| {
              DecisionSignalChoiceCriterion::new(
                choice.clone(),
                FactoryText::new(format!(
                  "Compare the explicitly declared choice {} against the selected public state.",
                  choice.as_str()
                ))
                .unwrap(),
              )
            })
            .collect(),
        ),
      )
      .unwrap(),
    ],
    consuming_question: key("pilot-choice"),
    threshold: DecisionSignalThreshold::new(
      purpose,
      capability.probability_semantics().clone(),
      reference(
        if purpose == DecisionSignalPurpose::Routing {
          "pilot-routing-calibration"
        } else {
          "pilot-tool-risk-calibration"
        },
        "v1",
      ),
      DecisionSignalProbability::new(600000).unwrap(),
      DecisionSignalProbability::new(100000).unwrap(),
    )
    .unwrap(),
    state_schema,
    state_projection,
    call_budget: budget,
  }
}
pub fn routing_binding(
  project_id: ProjectId,
  action: FactoryKey,
  outcome: FactoryKey,
  plan: FactoryDecisionSignalRequestPlan,
  projection: FlowDataMapping,
  baseline_route: FactoryKey,
  network_host: NetworkHost,
) -> FactoryDecisionSignalNodeBinding {
  assert_eq!(plan.profile.purpose(), DecisionSignalPurpose::Routing);
  let mut grants = serde_json::to_value(FactoryPermissionSet::deny_all()).unwrap();
  grants["plugins"] = json!([plan.capability.adapter()]);
  grants["secret_profiles"] = json!(["pilot-signal-credential"]);
  grants["network_hosts"] = json!([network_host.as_str()]);
  let grants: FactoryPermissionSet = serde_json::from_value(grants).unwrap();
  FactoryDecisionSignalNodeBinding {
    action: FlowActionBinding::new(plan.capability.adapter().clone(), action, json!({})).unwrap(),
    project_id,
    plan,
    baseline_route: baseline_route.clone(),
    credential_profile: key("pilot-signal-credential"),
    network_host,
    project_permissions: grants.clone(),
    local_permissions: LocalPermissionCeiling::fully_enforced(grants),
    outputs: vec![FactoryDecisionSignalNodeOutput {
      disposition: DecisionSignalDisposition::Route(baseline_route),
      outcome,
      projection,
    }],
  }
  .pin_policy()
  .unwrap()
}
pub fn routing_entry(binding: &FactoryDecisionSignalNodeBinding) -> Value {
  json!({"action":binding.action,"project_id":binding.project_id,"plan":binding.plan,"baseline_route":binding.baseline_route,"credential_profile":binding.credential_profile,"network_host":binding.network_host.as_str(),"project_permissions":binding.project_permissions,"local_permissions":{"grants":binding.local_permissions.grants(),"enforceable":PermissionCategory::ALL},"outputs":binding.outputs.iter().map(|output|json!({"disposition":output.disposition,"outcome":output.outcome,"projection":output.projection})).collect::<Vec<_>>()})
}
