use octacity_server_domain::{BuildConfigurationId, BuildConfigurationVersion, ProjectId};
use octacity_server_factory::*;
fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn digest(byte: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([byte; 32])
}
pub struct Fixture {
  pub choices: FactoryConfigurationChoices,
  pub draft: FactoryConfigurationDraft,
}

pub fn fixture(project_id: ProjectId) -> Fixture {
  let references = [
    (FactoryChoiceKind::AdmissionPolicy, "admission", "manual.medium", 1),
    (FactoryChoiceKind::PermissionCeiling, "permissions", "restricted", 2),
    (FactoryChoiceKind::CriterionPack, "criteria", "quality", 3),
    (FactoryChoiceKind::Evaluator, "evaluator", "review", 4),
    (FactoryChoiceKind::DeliveryAdapter, "delivery-adapter", "github", 5),
    (FactoryChoiceKind::DeliveryPolicy, "delivery-policy", "human-review", 6),
  ]
  .into_iter()
  .map(|(kind, alias, identity, value)| FactoryReferenceChoice {
    kind,
    alias: key(alias),
    reference: ImmutableReference::new(key(identity), key("v1"), digest(value)),
  })
  .collect();
  let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references,
    build_configurations: vec![(
      key("build"),
      BuildConfigurationRef::new(
        BuildConfigurationId::generate(),
        BuildConfigurationVersion::INITIAL,
        project_id,
        digest(7),
      ),
    )],
  })
  .unwrap();
  let budget = || BudgetLimit::new(20, 10_000, 1_000, 10_000, 10_000).unwrap();
  let stage = |name, kind| octacity_server_factory::testing::FactoryStageDraft {
    key: key(name),
    kind,
    build_configuration: key("build"),
    budget: budget(),
  };
  let draft = FactoryConfigurationDraft {
    flow: None,
    admission_policy: key("admission"),
    wip_limits: FactoryWipLimits::new(20, 20).unwrap(),
    hard_budget: budget(),
    permission_ceiling: key("permissions"),
    enabled: true,
  };
  let draft = octacity_server_factory::testing::with_contract_flow(
    draft,
    vec![
      stage("implement", FactoryStageKind::Implementation),
      stage("validate", FactoryStageKind::Validation),
      stage("evaluate", FactoryStageKind::Evaluation),
    ],
    &choices,
  );
  Fixture { choices, draft }
}
