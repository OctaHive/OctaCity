// Shared pure admission fixtures for application and composed source-worker contracts.
fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}

fn exact(identity: &str, value: u8) -> ImmutableReference {
  ImmutableReference::new(key(identity), key("v1"), digest(value))
}

pub(super) fn configuration(project_id: ProjectId, id: FactoryConfigurationId) -> FactoryConfiguration {
  configuration_with_flow(project_id, id, None)
}

pub(super) fn configuration_with_flow(
  project_id: ProjectId,
  id: FactoryConfigurationId,
  flow: Option<octacity_server_factory::FactoryFlowConfiguration>,
) -> FactoryConfiguration {
  let build = BuildConfigurationRef::new(
    BuildConfigurationId::generate(),
    BuildConfigurationVersion::INITIAL,
    project_id,
    digest(7),
  );
  let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: vec![
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::AdmissionPolicy,
        alias: key("admission"),
        reference: exact("manual.medium", 1),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::PermissionCeiling,
        alias: key("permissions"),
        reference: exact("restricted", 2),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::CriterionPack,
        alias: key("criteria"),
        reference: exact("quality", 3),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::Evaluator,
        alias: key("evaluator"),
        reference: exact("review", 4),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::DeliveryAdapter,
        alias: key("delivery-adapter"),
        reference: exact("github", 5),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::DeliveryPolicy,
        alias: key("delivery-policy"),
        reference: exact("human-review", 6),
      },
    ],
    build_configurations: vec![(key("build"), build)],
  })
  .unwrap();
  let limit = flow.as_ref().map_or_else(
    || BudgetLimit::new(10, 10_000, 1_000, 10_000, 10_000).unwrap(),
    |configuration| configuration.limits.budget(),
  );
  let budget = || limit;
  let stage = |name, kind| octacity_server_factory::testing::FactoryStageDraft {
    key: key(name),
    kind,
    build_configuration: key("build"),
    budget: budget(),
  };
  FactoryConfiguration::publish(
    id,
    FactoryConfigurationVersion::INITIAL,
    project_id,
    digest(20),
    octacity_server_factory::testing::with_contract_flow(
      FactoryConfigurationDraft {
        flow,
        admission_policy: key("admission"),
        wip_limits: FactoryWipLimits::new(2, 4).unwrap(),
        hard_budget: budget(),
        permission_ceiling: key("permissions"),
        enabled: true,
      },
      vec![
        stage("implement", FactoryStageKind::Implementation),
        stage("validate", FactoryStageKind::Validation),
        stage("evaluate", FactoryStageKind::Evaluation),
      ],
      &choices,
    ),
    &choices,
  )
  .unwrap()
}

pub(super) fn repository(
  project_id: ProjectId,
  id: RepositoryId,
  default_reference: Option<SourceReference>,
) -> PublishedRepository {
  let main = SourceReference::new("refs/heads/main").unwrap();
  PublishedRepository {
    id,
    project_id,
    name: RepositoryName::new("source").unwrap(),
    version: RepositoryVersion::INITIAL,
    definition: RepositoryDefinition {
      vcs_integration_id: IntegrationId::generate(),
      repository_locator: RepositoryLocator::new("https://example.test/source.git").unwrap(),
      selection: RepositorySelectionPolicy {
        allowed_references: BTreeSet::from([main]),
        default_reference,
        allow_exact_revision: true,
      },
    },
    published_at: time(1),
  }
}
