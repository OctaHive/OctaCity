//! Portable installation qualification before dependency I/O or owner dispatch.
use super::factory::NativeCodexNodeExecutor;
use super::*;
use octacity_server_application::*;
use octacity_server_domain::{BuildConfigurationId, BuildConfigurationVersion, PipelineNodeId, ProjectId};
use octacity_server_factory::*;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
#[allow(dead_code)]
#[path = "../../../application/src/factory_job_test_support.rs"]
pub(super) mod managed;
pub(super) fn resources() -> FactoryRuntimeResources {
  let pool = sqlx::postgres::PgPoolOptions::new()
    .connect_lazy("postgresql://fixture:fixture@127.0.0.1:1/unused")
    .unwrap();
  let canonical = managed::canonical_v3_spec();
  let toolchain = JobSpecToolchainPolicy {
    source: octacity_server_job::SourcePluginPolicy::new("git", "1.0.0", canonical.source.plugin_sha256.clone(), "url")
      .unwrap(),
    octa: canonical.octa.clone(),
    validity: octacity_server_job::JobSpecValidity::new(600).unwrap(),
  };
  let objects = octacity_artifact_s3::S3ArtifactStore::new(octacity_artifact_s3::S3ArtifactStoreConfig {
    endpoint: "http://127.0.0.1:1".to_owned(),
    region: "us-east-1".to_owned(),
    bucket: "unused-artifacts".to_owned(),
    prefix: String::new(),
    access_key: zeroize::Zeroizing::new("fixture".to_owned()),
    secret_key: zeroize::Zeroizing::new("fixture".to_owned()),
    force_path_style: true,
    operation_timeout: Duration::from_millis(1),
    capability_recheck_interval: Duration::from_secs(1),
  })
  .unwrap();
  FactoryRuntimeResources {
    runs: Arc::new(octacity_server_store_postgres::PostgresStore::new(pool.clone())),
    builds: Arc::new(octacity_server_store_postgres::PostgresAuthoritativeStore::new(
      pool,
      Arc::new(octacity_server_job::JobSpecSigner::new("fixture", [7; 32]).unwrap()),
    )),
    objects: Arc::new(objects),
    toolchain,
    revisions: Arc::new(ExactRevisionResolver),
  }
}

pub(super) fn review_planning_profile(project: ProjectId) -> FactoryReviewPlanningProfile {
  use managed::key;
  let reference = |value| managed::reference(value, "v1");
  let artifact = FactoryArtifactReference::new(
    octacity_server_domain::ArtifactId::generate(),
    FactoryDigest::from_bytes([6; 32]),
    1,
  )
  .unwrap();
  let pack = CriterionPack::new(
    project,
    ImmutableReference::new(key("criteria"), key("v1"), artifact.content_digest()),
    reference("criteria-schema"),
    artifact,
  )
  .unwrap();
  let evaluator = reference("installed-review-evaluator");
  let handling = reference("review-data-handling");
  let budget = BudgetLimit::new(1, 1000, 10, 10, 10000).unwrap();
  FactoryReviewPlanningProfile {
    action: key("prepare-review"),
    ready: FlowOutcomeDefinition::new(key("ready"), FlowOutcomeKind::Success, reference("plan-schema")),
    escalation: FlowOutcomeDefinition::new(
      key("unavailable"),
      FlowOutcomeKind::Failure,
      reference("escalation-schema"),
    ),
    purpose: ReviewPurpose::CodeReview,
    criterion_packs: vec![pack.clone()],
    branches: vec![ReviewBranch::new(key("independent"), evaluator.clone(), true)],
    policy: EvaluationPolicy::try_new(vec![pack.reference().clone()], vec![evaluator.clone()], 1, budget).unwrap(),
    data_handling: handling.clone(),
    capabilities: vec![
      ReviewEvaluatorCapability::try_new(evaluator, vec![ReviewPurpose::CodeReview], vec![handling], budget).unwrap(),
    ],
  }
}

#[tokio::test]
async fn native_catalog_installs_review_planning_by_exact_profile_identity_without_dependency_io() {
  use managed::key;
  struct Ready;
  #[async_trait::async_trait]
  impl FactoryRuntimeReadiness for Ready {
    async fn available(&self) -> bool {
      true
    }
  }
  let project = ProjectId::generate();
  let profile = review_planning_profile(project);
  let identity = ImmutableReference::new(key("installed-planner"), key("v1"), profile.digest().unwrap());
  let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: vec![],
    build_configurations: vec![],
  })
  .unwrap();
  let catalog = FactoryRuntimeProfileCatalog {
    projects: vec![(project, choices)],
    codex: vec![],
    signals: vec![],
    profile_selectors: vec![],
    tool_risk: vec![],
    review_planning: vec![(identity.clone(), profile)],
    executors: vec![],
  };
  let installed = FactoryProfileRuntimeInstaller::new(catalog, Arc::new(Ready), vec![])
    .unwrap()
    .install(resources())
    .await
    .unwrap();
  assert_eq!(installed.executors.len(), 1);
  let (kind, execution, _) = &installed.executors[0];
  assert_eq!(*kind, FlowNodeKind::TrustedAction);
  assert_eq!(*execution, NodeExecutionIdentity::External(identity));
}

#[tokio::test]
async fn native_reporting_worker_binds_installed_projectors_without_dependency_io() {
  use managed::key;
  struct Reporter;
  #[async_trait::async_trait]
  impl FactoryWorkStatusProjector for Reporter {
    async fn cancel_status(
      &self,
      _: FactoryWorkStatusRequest,
      _: octacity_server_domain::Timestamp,
    ) -> Result<Option<BudgetUsage>, ApplicationError> {
      panic!("installation must not cancel a projection")
    }
    async fn set_status(&self, _: FactoryWorkStatusRequest) -> Result<(FactoryDigest, BudgetUsage), ApplicationError> {
      panic!("installation must not write a status")
    }
  }
  let settings = FactoryReporterWorkerSettings::new(key("installed-reporter"), 1000, 4).unwrap();
  let resources = resources();
  assert!(matches!(
    resources.work_status_dispatcher(BTreeMap::new(), settings.clone()),
    Err(FactoryRuntimeInstallationError::Invalid)
  ));
  let projectors = BTreeMap::from([(
    managed::reference("installed-status-projector", "v1"),
    Arc::new(Reporter) as Arc<dyn FactoryWorkStatusProjector>,
  )]);
  assert!(resources.work_status_dispatcher(projectors, settings.clone()).is_ok());
  let root = tempfile::tempdir().unwrap();
  let registry = Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(root.path()).unwrap());
  let profile =
    super::FactoryReporterProfile::new(key("effects"), key("v1"), super::work_reporter_tests::draft("tracker"))
      .unwrap();
  assert!(
    resources
      .work_reporter_dispatcher(
        vec![profile],
        registry,
        settings,
        std::time::Duration::from_secs(1),
        std::time::Duration::from_millis(10),
        tokio_util::sync::CancellationToken::new()
      )
      .is_ok()
  );
}

#[tokio::test]
async fn native_review_installation_rejects_substituted_profiles_and_foreign_criterion_packs() {
  use managed::key;
  struct Ready;
  #[async_trait::async_trait]
  impl FactoryRuntimeReadiness for Ready {
    async fn available(&self) -> bool {
      true
    }
  }
  for forged_digest in [true, false] {
    let project = ProjectId::generate();
    let profile = review_planning_profile(if forged_digest { project } else { ProjectId::generate() });
    let identity = ImmutableReference::new(
      key("installed-planner"),
      key("v1"),
      if forged_digest {
        FactoryDigest::from_bytes([9; 32])
      } else {
        profile.digest().unwrap()
      },
    );
    let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
      references: vec![],
      build_configurations: vec![],
    })
    .unwrap();
    let catalog = FactoryRuntimeProfileCatalog {
      projects: vec![(project, choices)],
      codex: vec![],
      signals: vec![],
      profile_selectors: vec![],
      tool_risk: vec![],
      review_planning: vec![(identity, profile)],
      executors: vec![],
    };
    assert!(matches!(
      FactoryProfileRuntimeInstaller::new(catalog, Arc::new(Ready), vec![]),
      Err(FactoryRuntimeInstallationError::Invalid)
    ));
  }
}

#[tokio::test]
async fn signal_runtime_binding_requires_independent_authority_without_contacting_dependencies() {
  struct Provider(DecisionSignalProviderCapability);
  impl DecisionSignalProvider for Provider {
    fn capability(&self) -> &DecisionSignalProviderCapability {
      &self.0
    }
    fn network_destinations(&self) -> Option<&[NetworkHost]> {
      Some(&[])
    }
    fn evaluate<'a>(&'a self, _: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
      panic!("installation must not contact a provider");
    }
  }
  let resources = resources();
  let binding = signal_binding_fixture();
  let capability = binding.plan.capability.clone();
  let provider = Arc::new(Provider(capability.clone()));
  let mut denied = binding.clone();
  denied.project_permissions = FactoryPermissionSet::deny_all();
  assert!(matches!(
    resources.decision_signal_executor(vec![FactoryDecisionSignalNodeProfile {
      provider: provider.clone(),
      binding: denied
    }]),
    Err(FactoryRuntimeInstallationError::Invalid)
  ));
  let (kind, identity, _) = resources
    .decision_signal_executor(vec![FactoryDecisionSignalNodeProfile { provider, binding }])
    .unwrap();
  assert_eq!(kind, FlowNodeKind::DecisionSignal);
  assert_eq!(identity, NodeExecutionIdentity::External(capability.adapter().clone()));
}

fn signal_binding_fixture() -> FactoryDecisionSignalNodeBinding {
  let key = |value| FactoryKey::new(value).unwrap();
  let reference = |value| ImmutableReference::new(key(value), key("v1"), FactoryDigest::from_bytes([17; 32]));
  let budget = BudgetLimit::new(1, 100, 10, 10, 100).unwrap();
  let choices = DecisionSignalChoices::try_new(vec![key("stay"), key("redirect")]).unwrap();
  let profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
    purpose: DecisionSignalPurpose::Routing,
    provider: reference("installed-provider"),
    adapter: reference("installed-adapter"),
    model: reference("installed-model"),
    question_set: reference("installed-questions"),
    policy: reference("installed-policy"),
    mode: DecisionSignalMode::Shadow,
    fallback: DecisionSignalFallback::Escalate,
    budget,
    routes: Some(DecisionSignalRouteSet::new(key("projected-state"), choices.clone())),
  })
  .unwrap();
  let capability = DecisionSignalProviderCapability::new(
    profile.provider().clone(),
    profile.adapter().clone(),
    profile.model().clone(),
    reference("probability-semantics"),
    reference("data-handling"),
    vec![DecisionSignalPurpose::Routing],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(1024, 2, 2).unwrap(),
    budget,
  )
  .unwrap();
  let question = DecisionSignalQuestion::new(
    reference("selected-question"),
    FactoryText::new("Observe the explicitly selected state.").unwrap(),
    DecisionSignalQuestionDomain::FiniteChoice(choices),
    DecisionSignalQuestionCriteria::FiniteChoice(vec![
      DecisionSignalChoiceCriterion::new(
        key("stay"),
        FactoryText::new("Preserve the declared baseline.").unwrap(),
      ),
      DecisionSignalChoiceCriterion::new(
        key("redirect"),
        FactoryText::new("Recommend another declared route.").unwrap(),
      ),
    ]),
  )
  .unwrap();
  let schema = FlowDataSchema::new(key("selected-state"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let projection = FlowDataMapping::new(FlowValueMapping::Constant {
    value: serde_json::json!(true),
  })
  .unwrap();
  let plan = FactoryDecisionSignalRequestPlan {
    profile,
    capability: capability.clone(),
    questions: vec![question],
    consuming_question: key("selected-question"),
    threshold: DecisionSignalThreshold::new(
      DecisionSignalPurpose::Routing,
      capability.probability_semantics().clone(),
      reference("calibration"),
      DecisionSignalProbability::new(600000).unwrap(),
      DecisionSignalProbability::new(100000).unwrap(),
    )
    .unwrap(),
    state_schema: schema,
    state_projection: projection.clone(),
    call_budget: budget,
  };
  let mut grants = serde_json::to_value(FactoryPermissionSet::deny_all()).unwrap();
  grants["plugins"] = serde_json::json!([capability.adapter()]);
  grants["secret_profiles"] = serde_json::json!(["signal-credential"]);
  grants["network_hosts"] = serde_json::json!(["signal.internal"]);
  let grants: FactoryPermissionSet = serde_json::from_value(grants).unwrap();
  FactoryDecisionSignalNodeBinding {
    action: FlowActionBinding::new(
      capability.adapter().clone(),
      key("chosen-operation-45"),
      serde_json::json!({}),
    )
    .unwrap(),
    project_id: ProjectId::generate(),
    plan,
    baseline_route: key("stay"),
    credential_profile: key("signal-credential"),
    network_host: octacity_server_factory::NetworkHost::new("signal.internal").unwrap(),
    project_permissions: grants.clone(),
    local_permissions: LocalPermissionCeiling::fully_enforced(grants),
    outputs: vec![FactoryDecisionSignalNodeOutput {
      disposition: DecisionSignalDisposition::Route(key("stay")),
      outcome: key("declared-completion"),
      projection,
    }],
  }
  .pin_policy()
  .unwrap()
}

fn tool_risk_profile_fixture() -> FactoryToolRiskNodeProfile {
  struct Provider(DecisionSignalProviderCapability);
  impl DecisionSignalProvider for Provider {
    fn capability(&self) -> &DecisionSignalProviderCapability {
      &self.0
    }
    fn network_destinations(&self) -> Option<&[NetworkHost]> {
      Some(&[])
    }
    fn evaluate<'a>(&'a self, _: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
      panic!("installation must not contact a connector");
    }
  }
  let routing = signal_binding_fixture();
  let old = routing.plan.profile.clone();
  let mut plan = routing.plan;
  plan.profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
    purpose: DecisionSignalPurpose::ToolRisk,
    provider: old.provider().clone(),
    adapter: old.adapter().clone(),
    model: old.model().clone(),
    question_set: old.question_set().clone(),
    policy: old.policy().clone(),
    mode: DecisionSignalMode::Shadow,
    fallback: DecisionSignalFallback::Deny,
    budget: plan.call_budget,
    routes: None,
  })
  .unwrap();
  let old = plan.capability.clone();
  plan.capability = DecisionSignalProviderCapability::new(
    old.provider().clone(),
    old.adapter().clone(),
    old.model().clone(),
    old.probability_semantics().clone(),
    old.data_handling().clone(),
    vec![DecisionSignalPurpose::ToolRisk],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    old.limits(),
    old.budget_ceiling(),
  )
  .unwrap();
  plan.threshold = DecisionSignalThreshold::new(
    DecisionSignalPurpose::ToolRisk,
    old.probability_semantics().clone(),
    plan.threshold.calibration().clone(),
    DecisionSignalProbability::new(600000).unwrap(),
    DecisionSignalProbability::new(100000).unwrap(),
  )
  .unwrap();
  let build = qualified_codex_profile();
  let DecisionSignalQuestionDomain::FiniteChoice(choices) = plan.questions[0].domain() else {
    panic!("finite policy");
  };
  let binding = FactoryToolRiskBinding {
    project_id: build.build_configuration.project_id(),
    mapping: ToolRiskChoiceMapping::try_new(
      choices,
      vec![
        (FactoryKey::new("stay").unwrap(), DecisionSignalDisposition::Allow),
        (FactoryKey::new("redirect").unwrap(), DecisionSignalDisposition::Deny),
      ],
    )
    .unwrap(),
    plan,
    max_calls: 1,
    credential_profile: routing.credential_profile,
    network_host: routing.network_host,
    project_permissions: routing.project_permissions,
    local_permissions: routing.local_permissions,
  };
  FactoryToolRiskNodeProfile {
    provider: Arc::new(Provider(binding.plan.capability.clone())),
    binding,
    base_task_digest: build.base_task_digest(),
    build_configuration: build.build_configuration,
  }
}

#[tokio::test]
async fn installed_signal_transport_cannot_contact_a_host_outside_its_bound_authority() {
  let connector = |endpoint: &str, capability: &DecisionSignalProviderCapability| {
    Arc::new(
      octacity_server_decision_signal_jev::JevDecisionSignalProvider::new(
        endpoint,
        octacity_server_decision_signal_jev::JevApiKey::new("sk-glm5-network-scope-fixture".to_owned()).unwrap(),
        capability.clone(),
        octacity_server_decision_signal_jev::JevBillingPolicy::new(1).unwrap(),
        Duration::from_millis(100),
      )
      .unwrap(),
    )
  };
  let ungranted = "https://private-ungranted-host.invalid/api/v1/systemone/";
  let binding = signal_binding_fixture();
  assert!(
    matches!(
      resources().decision_signal_executor(vec![FactoryDecisionSignalNodeProfile {
        provider: connector(ungranted, &binding.plan.capability),
        binding: binding.clone(),
      }]),
      Err(FactoryRuntimeInstallationError::Invalid)
    ),
    "the transport must not escape declared network grants"
  );
  let mut tool_risk = tool_risk_profile_fixture();
  tool_risk.provider = connector(ungranted, &tool_risk.binding.plan.capability);
  assert!(matches!(
    resources().tool_risk_authorizer(vec![tool_risk]),
    Err(FactoryRuntimeInstallationError::Invalid)
  ));
  let permitted = connector("https://signal.internal/api/v1/systemone/", &binding.plan.capability);
  assert!(
    resources()
      .decision_signal_executor(vec![FactoryDecisionSignalNodeProfile {
        provider: permitted,
        binding,
      }])
      .is_ok(),
    "the independently granted transport remains available"
  );
}

#[tokio::test]
async fn native_tool_risk_connector_installs_exact_policy_without_dependency_io() {
  assert!(
    resources()
      .tool_risk_authorizer(vec![tool_risk_profile_fixture()])
      .is_ok()
  );
  assert!(matches!(
    resources().tool_risk_authorizer(vec![]),
    Err(FactoryRuntimeInstallationError::Invalid)
  ));
}

#[tokio::test]
async fn installed_tool_risk_catalog_retains_the_project_policy_for_agent_ingress() {
  struct Ready;
  #[async_trait::async_trait]
  impl FactoryRuntimeReadiness for Ready {
    async fn available(&self) -> bool {
      true
    }
  }
  let profile = tool_risk_profile_fixture();
  let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: vec![],
    build_configurations: vec![(
      FactoryKey::new("selected-workload").unwrap(),
      profile.build_configuration.clone(),
    )],
  })
  .unwrap();
  let catalog = FactoryRuntimeProfileCatalog {
    projects: vec![(profile.binding.project_id, choices)],
    codex: vec![],
    signals: vec![],
    profile_selectors: vec![],
    tool_risk: vec![profile],
    review_planning: vec![],
    executors: vec![],
  };
  let installed = FactoryProfileRuntimeInstaller::new(catalog.clone(), Arc::new(Ready), vec![])
    .unwrap()
    .install(resources())
    .await
    .unwrap();
  assert!(
    installed.tool_authorizer.is_some(),
    "installed policy must reach Agent ingress"
  );
  let mut foreign = catalog;
  foreign.projects[0].0 = ProjectId::generate();
  assert!(FactoryProfileRuntimeInstaller::new(foreign, Arc::new(Ready), vec![]).is_err());
}

#[tokio::test]
async fn private_signal_catalog_composes_the_openai_adapter_without_contacting_it() {
  let directory = tempfile::tempdir().unwrap();
  let mut capability = serde_json::to_value(signal_binding_fixture().plan.capability).unwrap();
  capability["provider"]["identity"] = serde_json::json!("openai");
  capability["adapter"]["identity"] = serde_json::json!("openai-decisions-http");
  capability["model"]["identity"] = serde_json::json!("gpt-6-luna");
  capability["model"]["version"] = serde_json::json!("unversioned-shadow-v1");
  capability["probability_semantics"]["identity"] = serde_json::json!("openai-choice-probabilities");
  capability["probability_semantics"]["version"] = serde_json::json!("v1");
  let configuration = directory.path().join("openai.json");
  let credential = directory.path().join("openai-key");
  let catalog = directory.path().join("connectors.json");
  std::fs::write(
    &configuration,
    serde_json::to_vec(&serde_json::json!({
      "version":1,"endpoint":"https://api.openai.com/v1/decisions","capability":capability,
      "timeout_milliseconds":1000,"input_cost_micro_units_per_million_tokens":100000
    }))
    .unwrap(),
  )
  .unwrap();
  std::fs::write(&credential, "sk-proj-composition-private-canary\n").unwrap();
  let mut document = serde_json::json!({"version":1,"connectors":[{
    "kind":"openai-decisions","credential_profile":"openai-shadow",
    "configuration_file":configuration,"credential_file":credential
  }]});
  std::fs::write(&catalog, serde_json::to_vec(&document).unwrap()).unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    for path in [&configuration, &credential, &catalog] {
      std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
  }
  let providers = load_factory_signal_connectors(&catalog).await.unwrap();
  assert_eq!(providers.len(), 1);
  assert_eq!(serde_json::to_value(providers[0].capability()).unwrap(), capability);
  assert_eq!(providers[0].credential_profile().as_str(), "openai-shadow");
  assert!(
    !providers[0].has_observed_failure(),
    "installation is not a remote health probe"
  );
  document["connectors"][0]["kind"] = serde_json::json!("unknown-adapter");
  std::fs::write(&catalog, serde_json::to_vec(&document).unwrap()).unwrap();
  assert!(load_factory_signal_connectors(&catalog).await.is_err());
}

#[tokio::test]
async fn private_signal_connector_catalog_installs_only_bounded_operator_owned_files() {
  let directory = tempfile::tempdir().unwrap();
  let capability = signal_binding_fixture().plan.capability;
  let configuration = directory.path().join("connector.json");
  let credential = directory.path().join("connector-key");
  let catalog = directory.path().join("connectors.json");
  let definition = serde_json::json!({
    "kind":"jev","credential_profile":"routing-key","configuration_file":configuration,"credential_file":credential,
  });
  let document = serde_json::json!({"version":1,"connectors":[definition.clone()]});
  std::fs::write(
    &configuration,
    serde_json::to_vec(&serde_json::json!({
      "version":1,"endpoint":"https://private-connector-canary.invalid/api/v1/systemone/",
      "capability":capability,"credit_cost_micro_units":7,"timeout_milliseconds":1000,
    }))
    .unwrap(),
  )
  .unwrap();
  std::fs::write(&credential, "sk-glm5-connector-private-canary\n").unwrap();
  std::fs::write(&catalog, serde_json::to_vec(&document).unwrap()).unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    for path in [&configuration, &credential, &catalog] {
      std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
  }
  let providers = load_factory_signal_connectors(&catalog).await.unwrap();
  assert_eq!(providers.len(), 1);
  assert_eq!(providers[0].capability(), &capability);
  assert_eq!(providers[0].credential_profile().as_str(), "routing-key");
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&credential, std::fs::Permissions::from_mode(0o644)).unwrap();
    let error = load_factory_signal_connectors(&catalog).await.err().unwrap();
    assert!(matches!(error, FactoryRuntimeInstallationError::Invalid));
    assert!(!error.to_string().contains("canary"));
    std::fs::set_permissions(&credential, std::fs::Permissions::from_mode(0o600)).unwrap();
  }
  std::fs::write(
    &catalog,
    serde_json::to_vec(&serde_json::json!({
      "version":1,"connectors":[definition.clone(),definition],
    }))
    .unwrap(),
  )
  .unwrap();
  assert!(
    load_factory_signal_connectors(&catalog).await.is_err(),
    "ambiguous exact connector fails closed"
  );
  std::fs::write(&catalog, vec![b' '; 4 * 1024 * 1024 + 1]).unwrap();
  assert!(load_factory_signal_connectors(&catalog).await.is_err());
}

#[tokio::test]
async fn deployment_routing_and_profile_policies_share_an_exact_installed_connector() {
  struct Provider(DecisionSignalProviderCapability);
  impl DecisionSignalProvider for Provider {
    fn network_destinations(&self) -> Option<&[NetworkHost]> {
      Some(&[])
    }
    fn capability(&self) -> &DecisionSignalProviderCapability {
      &self.0
    }
    fn evaluate<'a>(&'a self, request: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
      Box::pin(async move { Ok(signal_response_fixture(request, request.id())) })
    }
  }
  let mut binding = signal_binding_fixture();
  let mut capability = serde_json::to_value(&binding.plan.capability).unwrap();
  capability["supported_purposes"] = serde_json::json!(["routing", "profile_selection"]);
  binding.plan.capability = serde_json::from_value(capability).unwrap();
  binding = binding.pin_policy().unwrap();
  let mut selection = serde_json::to_value(&binding.plan).unwrap();
  selection["profile"]["purpose"] = serde_json::json!("profile_selection");
  selection["profile"]["routes"] = serde_json::Value::Null;
  selection["profile"]["policy"]["identity"] = serde_json::json!("profile-policy");
  selection["profile"]["question_set"]["identity"] = serde_json::json!("profile-questions");
  selection["threshold"]["purpose"] = serde_json::json!("profile_selection");
  let mut selection: FactoryDecisionSignalRequestPlan = serde_json::from_value(selection).unwrap();
  let criteria = DecisionSignalProviderInput::new(
    DecisionSignalInputMedia::CanonicalJson,
    FactoryText::new("true").unwrap(),
    selection.questions.clone(),
    selection.consuming_question.clone(),
    None,
  )
  .unwrap();
  selection.threshold = selection
    .threshold
    .qualify_profile_selection(&selection.profile, &criteria)
    .unwrap();
  let outputs = binding
    .outputs
    .iter()
    .map(|output| {
      serde_json::json!({
        "disposition":output.disposition,"outcome":output.outcome,"projection":output.projection,
      })
    })
    .collect::<Vec<_>>();
  let document = serde_json::json!({
    "version":1,"projects":[{"project_id":binding.project_id,"references":[],"build_configurations":[]}],
    "codex":[],"signals":[{
      "action":binding.action,"project_id":binding.project_id,"plan":binding.plan,
      "baseline_route":binding.baseline_route,"credential_profile":binding.credential_profile,
      "network_host":binding.network_host.as_str(),"project_permissions":binding.project_permissions,
      "local_permissions":{"grants":binding.local_permissions.grants(),"enforceable":PermissionCategory::ALL},
      "outputs":outputs,
    }],
    "profile_selectors":[{
      "selection_policy":selection.profile.policy(),
      "project_id":binding.project_id,"plan":selection,"baseline":"stay",
      "credential_profile":binding.credential_profile,"project_permissions":binding.project_permissions,
      "local_permissions":{"grants":binding.local_permissions.grants(),"enforceable":PermissionCategory::ALL},
    }],
  });
  let bytes = serde_json::to_vec(&document).unwrap();
  let provider = FactoryRuntimeSignalConnector::new(
    binding.credential_profile.clone(),
    Arc::new(Provider(binding.plan.capability.clone())),
  );
  let mut parsed =
    FactoryRuntimeProfileCatalog::parse_json_with_signal_providers(&bytes, std::slice::from_ref(&provider)).unwrap();
  assert_eq!(parsed.signals.len(), 1);
  assert_eq!(parsed.signals[0].provider.capability(), &binding.plan.capability);
  assert_eq!(parsed.signals[0].bindings[0].action, binding.action);
  assert_eq!(parsed.profile_selectors.len(), 1);
  let selector = &parsed.profile_selectors[0];
  let mut entries = parsed.projects[0].1.entries();
  entries.references.push(FactoryReferenceChoice {
    kind: FactoryChoiceKind::DecisionSignalPolicy,
    alias: FactoryKey::new("profile-selector").unwrap(),
    reference: selector.selection_policy.clone(),
  });
  parsed.projects[0].1 = FactoryConfigurationChoices::try_new(entries).unwrap();

  struct Ready;
  #[async_trait::async_trait]
  impl FactoryRuntimeReadiness for Ready {
    async fn available(&self) -> bool {
      true
    }
  }
  let installation = FactoryProfileRuntimeInstaller::new(parsed.clone(), Arc::new(Ready), vec![])
    .unwrap()
    .install(resources())
    .await
    .unwrap();
  assert!(
    installation.profile_selectors.is_some(),
    "the native Flow owner receives the installed consumer"
  );
  let octacity_server_store::FactoryConfigurationAvailability::Available(choices) = installation
    .capabilities
    .availability(binding.project_id)
    .await
    .unwrap()
  else {
    panic!("installed choices");
  };
  let metadata = serde_json::to_value(choices.choice_metadata(
    FactoryChoiceKind::DecisionSignalPolicy,
    &FactoryKey::new("profile-selector").unwrap(),
  ))
  .unwrap();
  assert_eq!(metadata["profile_selection"]["mode"], "shadow");
  assert_eq!(
    metadata["profile_selection"]["candidates"],
    serde_json::json!(["stay", "redirect"])
  );
  assert_eq!(
    metadata["profile_selection"]["minimum_probability"],
    serde_json::to_value(selection.threshold.minimum_probability()).unwrap()
  );
  assert_eq!(
    metadata["profile_selection"]["provider"],
    serde_json::to_value(selection.profile.provider()).unwrap()
  );
  assert!(!metadata.to_string().contains("instructions"));
  assert!(Arc::ptr_eq(
    &parsed.signals[0].provider,
    &parsed.profile_selectors[0].provider
  ));
  let node = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let routing = binding.plan.prepare(&node.input, &node.attempt).unwrap();
  let profiles = selection.prepare(&node.input, &node.attempt).unwrap();
  assert_ne!(routing.id(), profiles.id());
  let installed = &parsed.signals[0].provider;
  let route_receipt = consume_routing_signal(
    DecisionSignalReceiptId::generate(),
    &routing,
    DecisionSignalProviderObservation::Result(installed.evaluate(&routing).await.unwrap()),
    binding.baseline_route.clone(),
  )
  .unwrap();
  let profile_receipt = consume_profile_selection_signal(
    DecisionSignalReceiptId::generate(),
    &profiles,
    DecisionSignalProviderObservation::Result(installed.evaluate(&profiles).await.unwrap()),
    binding.baseline_route.clone(),
  )
  .unwrap();
  assert!(matches!(
    route_receipt.consumption().final_disposition(),
    DecisionSignalDisposition::Route(_)
  ));
  assert!(matches!(
    profile_receipt.consumption().final_disposition(),
    DecisionSignalDisposition::Profile(_)
  ));
  assert!(route_receipt.replay(&profiles).is_err());
  assert!(profile_receipt.replay(&routing).is_err());
  let mut foreign_profile_key = document.clone();
  foreign_profile_key["profile_selectors"][0]["credential_profile"] = serde_json::json!("other-profile-key");
  assert!(
    FactoryRuntimeProfileCatalog::parse_json_with_signal_providers(
      &serde_json::to_vec(&foreign_profile_key).unwrap(),
      std::slice::from_ref(&provider)
    )
    .is_err()
  );

  let mut crossed_credentials = document.clone();
  crossed_credentials["signals"][0]["credential_profile"] = serde_json::json!("another-routing-key");
  assert!(
    FactoryRuntimeProfileCatalog::parse_json_with_signal_providers(
      &serde_json::to_vec(&crossed_credentials).unwrap(),
      std::slice::from_ref(&provider)
    )
    .is_err(),
    "exact provider identity cannot grant another credential profile"
  );
  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join("routing-profiles.json");
  std::fs::write(&path, &bytes).unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
  }
  let loaded = FactoryRuntimeProfileCatalog::load_with_signal_providers(&path, std::slice::from_ref(&provider))
    .await
    .unwrap();
  assert_eq!(loaded.signals[0].bindings[0].action, binding.action);
  assert!(
    FactoryRuntimeProfileCatalog::parse_json(&bytes).is_err(),
    "metadata never creates a provider"
  );
  assert!(
    FactoryRuntimeProfileCatalog::parse_json_with_signal_providers(&bytes, &[provider.clone(), provider]).is_err(),
    "ambiguous connector installation fails closed"
  );
}

#[test]
fn deployment_tool_risk_profiles_resolve_an_exact_separate_server_connector() {
  let profile = tool_risk_profile_fixture();
  let binding = &profile.binding;
  let document = serde_json::json!({
    "version":1,"projects":[{
      "project_id":binding.project_id,"references":[],"build_configurations":[{
        "alias":"observer-42","reference":profile.build_configuration,
      }],
    }],"codex":[],"tool_risk":[{
      "project_id":binding.project_id,"plan":binding.plan,"mapping":binding.mapping,
      "max_calls":binding.max_calls,"credential_profile":binding.credential_profile,
      "network_host":binding.network_host.as_str(),"project_permissions":binding.project_permissions,
      "local_permissions":{"grants":binding.local_permissions.grants(),"enforceable":PermissionCategory::ALL},
      "base_task_digest":profile.base_task_digest,"build_configuration":profile.build_configuration,
    }],
  });
  let bytes = serde_json::to_vec(&document).unwrap();
  let providers = [FactoryRuntimeSignalConnector::new(
    binding.credential_profile.clone(),
    profile.provider,
  )];
  let installed = FactoryRuntimeProfileCatalog::parse_json_with_signal_providers(&bytes, &providers).unwrap();
  assert!(installed.signals.is_empty(), "tool-risk is separate from routing");
  assert_eq!(installed.tool_risk.len(), 1);
  assert_eq!(
    installed.tool_risk[0].binding.policy_digest().unwrap(),
    binding.policy_digest().unwrap()
  );
  assert_eq!(installed.tool_risk[0].base_task_digest, profile.base_task_digest);
  assert_eq!(installed.tool_risk[0].build_configuration, profile.build_configuration);
  assert_eq!(installed.tool_risk[0].provider.capability(), &binding.plan.capability);
  assert!(
    FactoryRuntimeProfileCatalog::parse_json(&bytes).is_err(),
    "metadata never creates a connector"
  );
  for (field, value) in [
    ("max_calls", serde_json::json!(0)),
    ("credential_profile", serde_json::json!("another-tool-risk-key")),
    ("project_id", serde_json::json!(ProjectId::generate())),
    ("private_provider_key", serde_json::json!("secret-canary")),
  ] {
    let mut invalid = document.clone();
    invalid["tool_risk"][0][field] = value;
    let error = FactoryRuntimeProfileCatalog::parse_json_with_signal_providers(
      &serde_json::to_vec(&invalid).unwrap(),
      &providers,
    )
    .err()
    .unwrap();
    assert_eq!(error.to_string(), "Factory installation is invalid");
    assert!(!format!("{error:?}").contains("secret-canary"));
  }
}

#[tokio::test]
async fn installed_signal_connector_reports_an_observed_failure_without_an_extra_request() {
  use std::sync::atomic::{AtomicUsize, Ordering};
  struct UnavailableProvider {
    capability: DecisionSignalProviderCapability,
    calls: Arc<AtomicUsize>,
  }
  struct Dependencies(Arc<AtomicUsize>);
  #[async_trait::async_trait]
  impl FactoryRuntimeReadiness for Dependencies {
    async fn available(&self) -> bool {
      self.0.fetch_add(1, Ordering::SeqCst);
      true
    }
  }
  impl DecisionSignalProvider for UnavailableProvider {
    fn capability(&self) -> &DecisionSignalProviderCapability {
      &self.capability
    }
    fn evaluate<'a>(&'a self, request: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
      Box::pin(async move {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(
          DecisionSignalProviderFailure::unobserved(
            request.id(),
            request.provider().clone(),
            request.adapter().clone(),
            request.model().clone(),
            DecisionSignalState::Unavailable,
          )
          .unwrap(),
        )
      })
    }
  }
  let binding = signal_binding_fixture();
  let calls = Arc::new(AtomicUsize::new(0));
  let connector = FactoryRuntimeSignalConnector::new(
    binding.credential_profile.clone(),
    Arc::new(UnavailableProvider {
      capability: binding.plan.capability.clone(),
      calls: calls.clone(),
    }),
  );
  let retained = connector.clone();
  let dependency_checks = Arc::new(AtomicUsize::new(0));
  let readiness = FactoryRuntimeSignalReadiness::new(
    Arc::new(Dependencies(dependency_checks.clone())),
    vec![retained.clone()],
  );
  assert!(readiness.available().await);
  assert!(!connector.has_observed_failure());
  assert_eq!(calls.load(Ordering::SeqCst), 0);
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let request = binding.plan.prepare(&fixture.input, &fixture.attempt).unwrap();
  assert!(connector.provider().evaluate(&request).await.is_err());
  assert!(connector.has_observed_failure());
  assert!(
    retained.has_observed_failure(),
    "runtime readiness observes the same installed transport"
  );
  assert!(
    !readiness.available().await,
    "a failed installed capability must degrade Factory readiness"
  );
  assert_eq!(dependency_checks.load(Ordering::SeqCst), 1);
  assert_eq!(
    calls.load(Ordering::SeqCst),
    1,
    "reading availability must not consume provider quota"
  );
}

#[tokio::test(start_paused = true)]
async fn interrupted_signal_connector_exchange_remains_degraded_after_timeout() {
  struct PendingProvider(DecisionSignalProviderCapability);
  impl DecisionSignalProvider for PendingProvider {
    fn capability(&self) -> &DecisionSignalProviderCapability {
      &self.0
    }
    fn evaluate<'a>(&'a self, _: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
      Box::pin(std::future::pending())
    }
  }
  let binding = signal_binding_fixture();
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let request = binding.plan.prepare(&fixture.input, &fixture.attempt).unwrap();
  let connector = FactoryRuntimeSignalConnector::new(
    binding.credential_profile,
    Arc::new(PendingProvider(binding.plan.capability)),
  );
  assert!(
    tokio::time::timeout(Duration::from_millis(10), connector.provider().evaluate(&request))
      .await
      .is_err()
  );
  assert!(
    connector.has_observed_failure(),
    "an interrupted exchange cannot report known healthy transport"
  );
}

#[tokio::test]
async fn only_an_exact_valid_signal_response_restores_connector_availability() {
  use std::sync::atomic::{AtomicBool, Ordering};
  struct Provider {
    capability: DecisionSignalProviderCapability,
    substitute: Arc<AtomicBool>,
  }
  impl DecisionSignalProvider for Provider {
    fn capability(&self) -> &DecisionSignalProviderCapability {
      &self.capability
    }
    fn evaluate<'a>(&'a self, request: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
      Box::pin(async move {
        Ok(signal_response_fixture(
          request,
          if self.substitute.load(Ordering::SeqCst) {
            DecisionSignalRequestId::generate()
          } else {
            request.id()
          },
        ))
      })
    }
  }
  let binding = signal_binding_fixture();
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let request = binding.plan.prepare(&fixture.input, &fixture.attempt).unwrap();
  let substitute = Arc::new(AtomicBool::new(true));
  let connector = FactoryRuntimeSignalConnector::new(
    binding.credential_profile,
    Arc::new(Provider {
      capability: binding.plan.capability,
      substitute: substitute.clone(),
    }),
  );
  let response = connector.provider().evaluate(&request).await.unwrap();
  assert!(!DecisionSignalProviderObservation::Result(response).is_valid_for(&request));
  assert!(
    connector.has_observed_failure(),
    "a foreign result is not proof of a healthy installed capability"
  );
  substitute.store(false, Ordering::SeqCst);
  let response = connector.provider().evaluate(&request).await.unwrap();
  assert!(DecisionSignalProviderObservation::Result(response).is_valid_for(&request));
  assert!(!connector.has_observed_failure());
}

fn signal_response_fixture(
  request: &DecisionSignalProviderRequest,
  request_id: DecisionSignalRequestId,
) -> DecisionSignalProviderResult {
  let question = &request.input().questions()[0];
  let DecisionSignalQuestionDomain::FiniteChoice(choices) = question.domain() else {
    panic!("fixture requires finite choices")
  };
  DecisionSignalProviderResult::new(
    request_id,
    request.provider().clone(),
    request.adapter().clone(),
    request.model().clone(),
    request.probability_semantics().clone(),
    vec![DecisionSignalAnswer::new(
      question.reference().clone(),
      DecisionSignalAnswerValue::FiniteChoice {
        selected: choices.as_slice()[0].clone(),
        selected_probability: DecisionSignalProbability::new(800_000).unwrap(),
        runner_up_probability: DecisionSignalProbability::new(200_000).unwrap(),
      },
    )],
    BudgetUsage {
      attempts: 1,
      elapsed_millis: 1,
      ..BudgetUsage::default()
    },
  )
  .unwrap()
}

#[tokio::test(start_paused = true)]
async fn signal_connector_readiness_tracks_recovery_without_reevaluating_the_request() {
  use std::{
    collections::VecDeque,
    sync::{
      Mutex,
      atomic::{AtomicUsize, Ordering},
    },
  };
  struct Ready;
  #[async_trait::async_trait]
  impl FactoryRuntimeReadiness for Ready {
    async fn available(&self) -> bool {
      true
    }
  }
  struct Observer {
    capability: DecisionSignalProviderCapability,
    evaluations: AtomicUsize,
    observations: Mutex<VecDeque<Option<DecisionSignalProviderObservation>>>,
  }
  impl DecisionSignalProvider for Observer {
    fn capability(&self) -> &DecisionSignalProviderCapability {
      &self.capability
    }
    fn evaluate<'a>(&'a self, _: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
      Box::pin(async move {
        self.evaluations.fetch_add(1, Ordering::SeqCst);
        std::future::pending().await
      })
    }
    fn observe_evaluation<'a>(
      &'a self,
      _: &'a DecisionSignalProviderRequest,
    ) -> DecisionSignalProviderObservationFuture<'a> {
      Box::pin(async move {
        let next = self.observations.lock().unwrap().pop_front();
        match next {
          Some(observation) => observation,
          None => std::future::pending().await,
        }
      })
    }
  }
  let binding = signal_binding_fixture();
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let request = binding.plan.prepare(&fixture.input, &fixture.attempt).unwrap();
  let failure = DecisionSignalProviderFailure::new(
    request.id(),
    request.provider().clone(),
    request.adapter().clone(),
    request.model().clone(),
    DecisionSignalState::Unavailable,
    BudgetUsage::default(),
  )
  .unwrap();
  let provider = Arc::new(Observer {
    capability: binding.plan.capability,
    evaluations: AtomicUsize::new(0),
    observations: Mutex::new(VecDeque::from([
      None,
      None,
      Some(DecisionSignalProviderObservation::Result(signal_response_fixture(
        &request,
        DecisionSignalRequestId::generate(),
      ))),
      Some(DecisionSignalProviderObservation::Failure(failure)),
      Some(DecisionSignalProviderObservation::Result(signal_response_fixture(
        &request,
        request.id(),
      ))),
      None,
    ])),
  });
  let connector = FactoryRuntimeSignalConnector::new(binding.credential_profile, provider.clone());
  let readiness = FactoryRuntimeSignalReadiness::new(Arc::new(Ready), vec![connector.clone()]);
  assert!(connector.provider().observe_evaluation(&request).await.is_none());
  assert!(readiness.available().await, "no observation preserves the prior state");
  assert!(
    tokio::time::timeout(Duration::from_millis(10), connector.provider().evaluate(&request))
      .await
      .is_err()
  );
  assert!(!readiness.available().await);
  for expected_available in [false, false, false, true, true] {
    let _ = connector.provider().observe_evaluation(&request).await;
    assert_eq!(
      readiness.available().await,
      expected_available,
      "only an exact recovered result establishes availability"
    );
  }
  assert!(
    tokio::time::timeout(
      Duration::from_millis(10),
      connector.provider().observe_evaluation(&request)
    )
    .await
    .is_err()
  );
  assert!(
    !readiness.available().await,
    "interrupted recovery cannot establish health"
  );
  assert_eq!(
    provider.evaluations.load(Ordering::SeqCst),
    1,
    "recovery never redispatches evaluation"
  );
}

#[tokio::test(start_paused = true)]
async fn signal_connector_timeout_preserves_unknown_usage_and_original_deadline() {
  use octacity_server_domain::Timestamp;
  use std::sync::atomic::{AtomicUsize, Ordering};
  struct Clock(Timestamp);
  impl FactoryExecutionClock for Clock {
    fn now(&self) -> Result<Timestamp, ApplicationError> {
      Ok(self.0)
    }
  }
  struct PendingProvider {
    capability: DecisionSignalProviderCapability,
    calls: Arc<AtomicUsize>,
  }
  impl DecisionSignalProvider for PendingProvider {
    fn capability(&self) -> &DecisionSignalProviderCapability {
      &self.capability
    }
    fn evaluate<'a>(&'a self, _: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
      Box::pin(async move {
        self.calls.fetch_add(1, Ordering::SeqCst);
        std::future::pending().await
      })
    }
  }
  let binding = signal_binding_fixture();
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let request = binding.plan.prepare(&fixture.input, &fixture.attempt).unwrap();
  for remaining in [100, 10, 0] {
    let calls = Arc::new(AtomicUsize::new(0));
    let now = Timestamp::from_unix_millis(request.deadline().unix_millis() - remaining).unwrap();
    let provider = super::factory_signal_provider::BoundedFactorySignalProvider::new(
      Arc::new(PendingProvider {
        capability: binding.plan.capability.clone(),
        calls: calls.clone(),
      }),
      Arc::new(Clock(now)),
    );
    let start = tokio::time::Instant::now();
    let failure = provider
      .evaluate(&request)
      .await
      .expect_err("unfinished connector must not complete");
    assert_eq!(failure.state(), DecisionSignalState::TimedOut);
    assert_eq!(failure.request_id(), request.id());
    assert_eq!(
      tokio::time::Instant::now() - start,
      Duration::from_millis(remaining as u64)
    );
    assert_eq!(calls.load(Ordering::SeqCst), usize::from(remaining != 0));
    assert_eq!(
      failure.usage(),
      if remaining == 0 {
        Some(BudgetUsage::default())
      } else {
        None
      }
    );
  }
}

#[tokio::test(start_paused = true)]
async fn signal_connector_recovers_after_the_original_deadline_with_bounded_readonly_io() {
  use octacity_server_domain::Timestamp;
  use std::sync::atomic::{AtomicUsize, Ordering};
  struct Clock(Timestamp);
  impl FactoryExecutionClock for Clock {
    fn now(&self) -> Result<Timestamp, ApplicationError> {
      Ok(self.0)
    }
  }
  struct Observer {
    capability: DecisionSignalProviderCapability,
    reads: Arc<AtomicUsize>,
    terminal: bool,
  }
  impl DecisionSignalProvider for Observer {
    fn capability(&self) -> &DecisionSignalProviderCapability {
      &self.capability
    }
    fn evaluate<'a>(&'a self, _: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
      panic!("recovery must not evaluate a request");
    }
    fn observe_evaluation<'a>(
      &'a self,
      request: &'a DecisionSignalProviderRequest,
    ) -> DecisionSignalProviderObservationFuture<'a> {
      Box::pin(async move {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if !self.terminal {
          return std::future::pending().await;
        }
        Some(DecisionSignalProviderObservation::Failure(
          DecisionSignalProviderFailure::new(
            request.id(),
            request.provider().clone(),
            request.adapter().clone(),
            request.model().clone(),
            DecisionSignalState::Unavailable,
            BudgetUsage {
              attempts: 1,
              elapsed_millis: 5,
              tokens: 1,
              cost_micro_units: 1,
              output_bytes: 1,
            },
          )
          .unwrap(),
        ))
      })
    }
  }
  let binding = signal_binding_fixture();
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let request = binding.plan.prepare(&fixture.input, &fixture.attempt).unwrap();
  for terminal in [true, false] {
    let reads = Arc::new(AtomicUsize::new(0));
    let provider = super::factory_signal_provider::BoundedFactorySignalProvider::new(
      Arc::new(Observer {
        capability: binding.plan.capability.clone(),
        reads: reads.clone(),
        terminal,
      }),
      Arc::new(Clock(
        Timestamp::from_unix_millis(request.deadline().unix_millis() + 10).unwrap(),
      )),
    );
    let start = tokio::time::Instant::now();
    let observed = DecisionSignalProvider::observe_evaluation(&provider, &request).await;
    assert_eq!(
      reads.load(Ordering::SeqCst),
      1,
      "expired dispatch authority does not prohibit readonly recovery"
    );
    assert_eq!(
      tokio::time::Instant::now() - start,
      Duration::from_millis(if terminal { 0 } else { 100 })
    );
    match observed {
      Some(DecisionSignalProviderObservation::Failure(failure)) if terminal => {
        assert_eq!(failure.request_id(), request.id());
        assert_eq!(failure.usage().unwrap().tokens, 1);
      }
      None if !terminal => (),
      _ => panic!("readonly timeout must retain uncertainty; measured recovery must be forwarded"),
    }
  }
}

fn qualified_codex_profile() -> FactoryCodexBuildProfile {
  let key = |text| FactoryKey::new(text).unwrap();
  let mut grants = serde_json::to_value(managed::permissions(false)).unwrap();
  grants["secret_profiles"] = serde_json::json!(["review-credential"]);
  for mount in grants["mounts"].as_array_mut().unwrap() {
    if mount["root"] == "/workspace/source" {
      mount["mode"] = serde_json::json!("read_only");
    }
  }
  grants["outputs"]["kinds"] = serde_json::json!(["codex-run-trace", "codex-run-provenance", "codex-run-result"]);

  let grants: FactoryPermissionSet = serde_json::from_value(grants).unwrap();
  FactoryCodexBuildProfile {
    mcp_configurations: vec![],
    toolchain: FactoryTaskToolchain {
      plugin: managed::reference("codex", "0.5.0"),
      executable: managed::reference("codex-cli", "0.116.0"),
      model: managed::reference("immutable-model", "v1"),
    },
    build_configuration: BuildConfigurationRef::new(
      BuildConfigurationId::generate(),
      BuildConfigurationVersion::INITIAL,
      ProjectId::generate(),
      FactoryDigest::from_bytes([3; 32]),
    ),
    instructions: FactoryText::new("Observe the declared schema.").unwrap(),
    credentials: FactoryCredentialProfiles::new(
      key("author-credential"),
      key("review-credential"),
      key("source-credential"),
      key("delivery-credential"),
    )
    .unwrap(),
    consumer: FactoryCredentialConsumer::ReadOnlyReview,
    usage_policy: FactoryCodexUsagePolicy::ReportedCost,
    tool_risk: None,
    tool_response_limits: None,
    reviewer: None,
    candidate_capture_tool: None,
    capture: None,
    pipeline_node: PipelineNodeId::new("operator-selected-job-42").unwrap(),
    parameters: BTreeMap::new(),
    project_permissions: grants.clone(),
    local_permissions: LocalPermissionCeiling::fully_enforced(grants),
  }
}

fn codex_catalog_document(profile: &FactoryCodexBuildProfile) -> serde_json::Value {
  let project = profile.build_configuration.project_id();
  serde_json::json!({
    "version": 1,
    "projects": [{"project_id": project, "references": [], "build_configurations": [
      {"alias": "observer-42", "reference": profile.build_configuration}
    ]}],
    "codex": [{
      "toolchain": {"plugin": profile.toolchain.plugin, "executable": profile.toolchain.executable,
        "model": profile.toolchain.model},
      "build_configuration": profile.build_configuration,
      "instructions": profile.instructions,
      "credentials": profile.credentials,
      "credential_purpose": "evaluator",
      "usage_policy": profile.usage_policy,
      "pipeline_node": profile.pipeline_node,
      "parameters": profile.parameters,
      "project_permissions": profile.project_permissions,
      "local_permissions": {"grants": profile.local_permissions.grants(),
        "enforceable": PermissionCategory::ALL}
    }]
  })
}

#[test]
fn deployment_codex_mcp_configuration_keeps_its_exact_content_pin() {
  use octacity_server_application::{FactoryCodexMcpConfiguration, FactoryCodexMcpServer};
  let key = |value: &str| FactoryKey::new(value).unwrap();
  let installed = FactoryCodexMcpConfiguration::new(
    key("installed-reader"),
    key("v1"),
    FactoryCodexMcpServer::Stdio {
      command: "/usr/bin/true".into(),
      args: vec![],
      env_vars: vec![],
      enabled_tools: None,
    },
    None,
  )
  .unwrap();
  let profile = qualified_codex_profile();
  let mut document = codex_catalog_document(&profile);
  document["codex"][0]["mcp_configurations"] = serde_json::json!([installed]);
  let parsed = FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&document).unwrap()).unwrap();
  assert_eq!(parsed.codex[0].mcp_configurations[0].reference(), installed.reference());
  document["codex"][0]["mcp_configurations"][0]["server"]["command"] = serde_json::json!("/usr/bin/false");
  assert!(FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&document).unwrap()).is_err());
}

#[tokio::test]
async fn deployment_profile_catalog_parses_exact_project_choices_and_private_codex_policy() {
  let profile = qualified_codex_profile();
  let project = profile.build_configuration.project_id();
  let source = codex_catalog_document(&profile);
  let encoded = serde_json::to_vec(&source).unwrap();
  let parsed = FactoryRuntimeProfileCatalog::parse_json(&encoded).unwrap();
  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join("private-profile-canary.json");
  std::fs::write(&path, &encoded).unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
  }
  let loaded = FactoryRuntimeProfileCatalog::load(&path).await.unwrap();
  assert_eq!(loaded.codex[0].task_digest(), parsed.codex[0].task_digest());
  #[cfg(unix)]
  {
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    let link = directory.path().join("profile-link.json");
    symlink(&path, &link).unwrap();
    assert!(FactoryRuntimeProfileCatalog::load(&link).await.is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let error = FactoryRuntimeProfileCatalog::load(&path).await.err().unwrap();
    assert_eq!(error.to_string(), "Factory installation is invalid");
    assert!(!format!("{error:?}").contains("private-profile-canary"));
  }
  assert_eq!(parsed.projects.len(), 1);
  assert_eq!(parsed.projects[0].0, project);
  assert_eq!(
    parsed.projects[0].1.entries().build_configurations,
    vec![(
      FactoryKey::new("observer-42").unwrap(),
      profile.build_configuration.clone()
    )]
  );
  assert_eq!(parsed.codex.len(), 1);
  assert_eq!(parsed.codex[0].task_digest(), profile.task_digest());
  assert_eq!(parsed.codex[0].pipeline_node.as_str(), "operator-selected-job-42");
  assert!(parsed.codex[0].candidate_capture_tool.is_none());
  let capture_tool = managed::reference("operator-capture", "3.4.1");
  let mut candidate_source = source.clone();
  candidate_source["codex"][0]["candidate_capture_tool"] = serde_json::json!(capture_tool);
  let candidate_profile = FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&candidate_source).unwrap())
    .expect("private profile can pin the installed capture tool for readonly candidate access");
  assert_eq!(
    candidate_profile.codex[0].candidate_capture_tool.as_ref(),
    Some(&capture_tool)
  );
  assert_eq!(candidate_profile.codex[0].task_digest(), profile.task_digest());
  let mut author_source = source.clone();
  let author = &mut author_source["codex"][0];
  author["credential_purpose"] = serde_json::json!("model");
  let capture_instruction = serde_json::json!({
    "author_name": "Configured author", "author_email": "author@example.invalid",
    "committed_at": 1, "patch_max_bytes": null,
    "policy": {"allowed_path_prefixes": ["public/approved"], "forbidden_control_paths": [],
      "max_changed_paths": 1, "max_file_bytes": 4096, "max_total_bytes": 4096,
      "allow_empty": false, "binary_policy": "reject",
      "forbidden_secret_patterns": octacity_protocol::ChangeSetSecretPatternV3::ALL}
  });
  author["capture"] = serde_json::json!({"tool": capture_tool, "instruction": capture_instruction});
  author["project_permissions"]["secret_profiles"] = serde_json::json!(["author-credential"]);
  for mount in author["project_permissions"]["mounts"].as_array_mut().unwrap() {
    if mount["root"] == "/workspace/source" {
      mount["mode"] = serde_json::json!("read_write");
    }
  }
  author["project_permissions"]["outputs"]["kinds"] = serde_json::json!([
    "codex-run-trace",
    "codex-run-provenance",
    "codex-run-result",
    "change-set.bundle",
    "change-set-manifest.json"
  ]);
  author["local_permissions"]["grants"] = author["project_permissions"].clone();
  let installed_author = FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&author_source).unwrap())
    .expect("bounded installed author has a separate coding credential and capture authority");
  assert_eq!(
    installed_author.codex[0].consumer,
    FactoryCredentialConsumer::CodingNode
  );
  assert_eq!(
    installed_author.codex[0].capture.as_ref().unwrap().tool(),
    &capture_tool
  );
  assert_eq!(
    installed_author.codex[0]
      .capture
      .as_ref()
      .unwrap()
      .instruction()
      .committed_at,
    1
  );
  let mut approved_author_toolchain = resources().toolchain;
  approved_author_toolchain.octa.plugin_digests.insert(
    "codex".to_owned(),
    installed_author.codex[0].toolchain.plugin.digest().to_string(),
  );
  assert_eq!(
    installed_author.codex[0].validate_installation(&approved_author_toolchain),
    Ok(())
  );
  let mut observed_source = author_source.clone();
  let observed = &mut observed_source["codex"][0];
  observed["capture"]["instruction"]["observation"] = serde_json::json!({
    "name":"operator.capture-facts", "schema":{"identity":"operator.closed-capture-facts","version":"v1","sha256":"2a".repeat(32)}
  });
  observed["project_permissions"]["outputs"]["kinds"]
    .as_array_mut()
    .unwrap()
    .push(serde_json::json!("operator.capture-facts"));
  observed["project_permissions"]["outputs"]["max_report_count"] = serde_json::json!(2);
  observed["local_permissions"]["grants"] = observed["project_permissions"].clone();
  let observed = FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&observed_source).unwrap())
    .expect("a private catalog can explicitly install an independently pinned capture Report");
  assert_ne!(observed.codex[0].task_digest(), installed_author.codex[0].task_digest());
  assert_eq!(
    observed.codex[0].validate_installation(&approved_author_toolchain),
    Ok(()),
    "the exact additional Report is part of installed capture authority"
  );
  let mut invalid_instruction = capture_instruction.clone();
  invalid_instruction["committed_at"] = serde_json::json!(0);
  for invalid_capture in [
    None,
    Some(serde_json::json!({
      "tool": capture_tool, "instruction": invalid_instruction,
    })),
  ] {
    let mut invalid_author = author_source.clone();
    invalid_author["codex"][0]["capture"] = invalid_capture.unwrap_or(serde_json::Value::Null);
    assert!(FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&invalid_author).unwrap()).is_err());
  }
  let mut wrong_purpose = author_source.clone();
  wrong_purpose["codex"][0]["credential_purpose"] = serde_json::json!("evaluator");
  assert!(FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&wrong_purpose).unwrap()).is_err());
  let mut wrong_consumer = installed_author.codex[0].clone();
  wrong_consumer.consumer = FactoryCredentialConsumer::ReadOnlyReview;
  assert_eq!(
    wrong_consumer.validate_installation(&approved_author_toolchain),
    Err(CodexCompilationError::CredentialProfile)
  );
  let mut missing_capture = installed_author.codex[0].clone();
  missing_capture.capture = None;
  assert_eq!(
    missing_capture.validate_installation(&approved_author_toolchain),
    Err(CodexCompilationError::CredentialProfile)
  );

  assert!(parsed.codex[0].tool_response_limits.is_none());
  let mut bounded = source.clone();
  bounded["codex"][0]["tool_response_limits"] = serde_json::json!({
    "per_call_bytes": 32,
    "total_bytes": 64
  });
  let bounded_profile = FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&bounded).unwrap())
    .expect("private operator policy may explicitly freeze independent reader response limits");
  let limits = bounded_profile.codex[0].tool_response_limits.unwrap();
  assert_eq!(limits.per_call_bytes, 32);
  assert_eq!(limits.total_bytes, 64);
  assert_ne!(bounded_profile.codex[0].task_digest(), profile.task_digest());
  for (per_call, total) in [(0, 64), (32, 0), (65, 64), (4_194_305, 4_194_305)] {
    bounded["codex"][0]["tool_response_limits"] = serde_json::json!({
      "per_call_bytes": per_call,
      "total_bytes": total
    });
    assert!(FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&bounded).unwrap()).is_err());
  }
  for invalid in [
    {
      let mut value = source.clone();
      value["version"] = serde_json::json!(2);
      value
    },
    {
      let mut value = source.clone();
      value["codex"][0]["credential_purpose"] = serde_json::json!("source");
      value
    },
    {
      let mut value = source.clone();
      value["codex"][0]["instructions_canary"] = serde_json::json!("secret-canary");
      value
    },
    {
      let mut value = source.clone();
      value["projects"][0]["project_id"] = serde_json::json!(ProjectId::generate());
      value
    },
  ] {
    let error = FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&invalid).unwrap())
      .err()
      .unwrap();
    assert_eq!(error.to_string(), "Factory installation is invalid");
    assert!(!format!("{error:?}").contains("secret-canary"));
  }
}

#[tokio::test]
async fn native_profile_installation_exposes_only_its_project_catalog_without_dependency_io() {
  use octacity_server_store::FactoryConfigurationAvailability;
  struct Ready;
  #[async_trait::async_trait]
  impl FactoryRuntimeReadiness for Ready {
    async fn available(&self) -> bool {
      true
    }
  }
  let profile = qualified_codex_profile();
  let project = profile.build_configuration.project_id();
  let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: vec![],
    build_configurations: vec![(
      FactoryKey::new("selected-observer").unwrap(),
      profile.build_configuration.clone(),
    )],
  })
  .unwrap();
  let catalog = FactoryRuntimeProfileCatalog {
    projects: vec![(project, choices.clone())],
    codex: vec![profile.clone()],
    signals: vec![],
    profile_selectors: vec![],
    tool_risk: vec![],
    review_planning: vec![],
    executors: vec![],
  };
  let installer = FactoryProfileRuntimeInstaller::new(catalog.clone(), Arc::new(Ready), vec![]).unwrap();
  let installed = installer.install(resources()).await.unwrap();
  assert!(
    installed.work_sources.is_none(),
    "manual Factory installations opt out without provider requirements"
  );
  let root = tempfile::tempdir().unwrap();
  let source = resources()
    .source_worker(
      FactorySourceInstallation {
        projects: vec![project],
        profiles: vec![],
        settings: FactorySourceWorkerSettings {
          worker: toml::from_str(
            "poll_interval_milliseconds=10\nclaim_lifetime_milliseconds=1000\nbatch_size=4\nconcurrency=2",
          )
          .unwrap(),
          max_attempts: 3,
          operation_timeout: Duration::from_millis(100),
          cancellation_grace: Duration::from_millis(50),
        },
      },
      Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(root.path()).unwrap()),
      Arc::new(TrustedNetworkManagementPolicy),
    )
    .unwrap();
  let opted_in = FactoryProfileRuntimeInstaller::new(catalog.clone(), Arc::new(Ready), vec![])
    .unwrap()
    .with_work_sources(source)
    .install(resources())
    .await
    .unwrap();
  assert!(opted_in.work_sources.is_some());
  assert_eq!(installed.executors.len(), 1);
  assert_eq!(installed.executors[0].0, FlowNodeKind::Reasoning);
  assert_eq!(
    installed.executors[0].1,
    NodeExecutionIdentity::External(profile.toolchain.executable)
  );
  assert_eq!(
    installed.capabilities.availability(project).await.unwrap(),
    FactoryConfigurationAvailability::Available(choices)
  );
  assert_eq!(
    installed
      .capabilities
      .availability(ProjectId::generate())
      .await
      .unwrap(),
    FactoryConfigurationAvailability::Disabled
  );
  let mut duplicate = catalog.clone();
  duplicate.projects.push(duplicate.projects[0].clone());
  assert!(FactoryProfileRuntimeInstaller::new(duplicate, Arc::new(Ready), vec![]).is_err());
  let mut foreign = catalog;
  foreign.projects[0].0 = ProjectId::generate();
  assert!(
    FactoryProfileRuntimeInstaller::new(foreign, Arc::new(Ready), vec![]).is_err(),
    "a Project cannot publish another Project's Build Configuration choice"
  );
}

#[tokio::test]
async fn codex_runtime_binding_rejects_unqualified_packages_before_dependency_io() {
  let resources = resources();
  let profile = qualified_codex_profile();
  let mut unsupported_tool_risk = profile.clone();
  unsupported_tool_risk.tool_risk = Some(
    serde_json::from_value(serde_json::json!({
      "policy_digest": FactoryDigest::from_bytes([41; 32]),
      "signal_allowance": BudgetLimit::new(1, 100, 10, 100, 100).unwrap(),
    }))
    .unwrap(),
  );
  assert!(
    matches!(
      resources.codex_executor(vec![unsupported_tool_risk]),
      Err(FactoryRuntimeInstallationError::Invalid)
    ),
    "a read-only evaluator cannot advertise a writable tool-risk broker capability"
  );
  let key = |text| FactoryKey::new(text).unwrap();
  let mut substituted = profile.clone();
  substituted.toolchain.plugin =
    ImmutableReference::new(key("codex"), key("0.5.0"), FactoryDigest::from_bytes([9; 32]));
  assert!(matches!(
    resources.codex_executor(vec![substituted]),
    Err(FactoryRuntimeInstallationError::Invalid)
  ));
  let mut crossed_price = profile.clone();
  crossed_price.usage_policy = FactoryCodexUsagePolicy::TokenRates {
    model: managed::reference("another-model", "v1"),
    input_micro_units_per_million: 1,
    cached_input_micro_units_per_million: 1,
    output_micro_units_per_million: 1,
  };
  assert!(
    matches!(
      resources.codex_executor(vec![crossed_price]),
      Err(FactoryRuntimeInstallationError::Invalid)
    ),
    "a price policy for another model must fail before protected input publication or Build dispatch"
  );
  let (kind, identity, _) = resources.codex_executor(vec![profile.clone()]).unwrap();
  assert_eq!(kind, FlowNodeKind::Reasoning);
  assert_eq!(identity, NodeExecutionIdentity::External(profile.toolchain.executable));
}

#[tokio::test]
async fn native_promotion_binding_uses_ordinary_build_ports_without_dependency_io() {
  struct Compiler;
  #[async_trait::async_trait]
  impl FactoryBuildMaterializer for Compiler {
    async fn materialize(
      &self,
      _: &FlowBuildIntent,
      _: &AdmittedFlow,
      _: FactoryClaimOwnership,
      _: octacity_server_domain::Timestamp,
    ) -> Result<octacity_server_store::AcceptFactoryBuild, OrdinaryBuildApplicationError> {
      panic!("installation must not create a deployment");
    }
  }
  struct Outputs;
  #[async_trait::async_trait]
  impl FactoryNodeBuildOutputSource for Outputs {
    async fn terminal_usage(
      &self,
      _: octacity_server_domain::BuildId,
      _: octacity_server_domain::AttemptId,
    ) -> Result<BudgetUsage, OrdinaryBuildApplicationError> {
      panic!("installation must not observe a deployment");
    }
    async fn published_outputs(
      &self,
      _: octacity_server_domain::BuildId,
      _: octacity_server_domain::AttemptId,
      _: u64,
    ) -> Result<FactoryNodeBuildOutputObservation, OrdinaryBuildApplicationError> {
      panic!("installation must not read deployment outputs");
    }
  }
  let identity = managed::reference("installed-promotion", "v1");
  let (kind, execution, _) = resources().promotion_executor(identity.clone(), Arc::new(Compiler), Arc::new(Outputs));
  assert_eq!(kind, FlowNodeKind::TrustedAction);
  assert_eq!(execution, NodeExecutionIdentity::External(identity));
}

#[tokio::test]
async fn native_validation_evidence_binds_original_retained_inputs_without_dependency_io() {
  use managed::key;
  let reference = |value| managed::reference(value, "v1");
  let report = EvidenceRequirement::new(
    key("selected-check-report"),
    EvidenceOutputKind::Report,
    reference("check-report-schema"),
    reference("trusted-check-tool"),
    reference("trusted-check-plugin"),
  );
  let facts = EvidenceRequirement::new(
    key("selected-check-facts"),
    EvidenceOutputKind::Report,
    reference("check-facts-schema"),
    reference("trusted-check-tool"),
    reference("trusted-check-plugin"),
  );
  let profile = FactoryProtectedValidationEvidenceProfile {
    action: key("publish-selected-evidence"),
    ready: FlowOutcomeDefinition::new(
      key("verified"),
      FlowOutcomeKind::Success,
      reference("complete-manifest-schema"),
    ),
    report,
    facts,
  };
  let identity = ImmutableReference::new(key("installed-evidence-owner"), key("v1"), profile.digest().unwrap());
  let policy = FactoryProtectedTestPolicy::new(
    ProtectedTestCoverage::new(vec![key("operator.custom")], 4, 9000, 4).unwrap(),
    900,
  )
  .unwrap();
  let inventory = EvidenceRequirement::new(
    key("selected-private-inventory"),
    EvidenceOutputKind::Artifact,
    reference("private-inventory-schema"),
    reference("private-capture-tool"),
    reference("private-capture-plugin"),
  );
  let runtime = resources();
  let binding = runtime
    .protected_validation_evidence_executor(
      identity.clone(),
      profile.clone(),
      reference("installed-author-policy"),
      policy.clone(),
      inventory.clone(),
      reference("installed-source-capture"),
    )
    .unwrap();
  assert_eq!(binding.0, FlowNodeKind::TrustedAction);
  assert_eq!(binding.1, NodeExecutionIdentity::External(identity.clone()));
  let substituted_identity = ImmutableReference::new(
    identity.identity().clone(),
    identity.version().clone(),
    FactoryDigest::from_bytes([99; 32]),
  );
  assert!(
    runtime
      .protected_validation_evidence_executor(
        substituted_identity,
        profile.clone(),
        reference("installed-author-policy"),
        policy.clone(),
        inventory.clone(),
        reference("installed-source-capture")
      )
      .is_err()
  );
  let mistaken_inventory = EvidenceRequirement::new(
    inventory.kind().clone(),
    EvidenceOutputKind::Report,
    inventory.schema().clone(),
    inventory.tool().clone(),
    inventory.plugin().clone(),
  );
  assert!(
    runtime
      .protected_validation_evidence_executor(
        identity.clone(),
        profile.clone(),
        reference("installed-author-policy"),
        policy.clone(),
        mistaken_inventory,
        reference("installed-source-capture")
      )
      .is_err()
  );
  let mut changed = profile.clone();
  changed.facts = changed.report.clone();
  let changed_identity = ImmutableReference::new(
    identity.identity().clone(),
    identity.version().clone(),
    changed.digest().unwrap(),
  );
  assert!(
    runtime
      .protected_validation_evidence_executor(
        changed_identity,
        changed,
        reference("installed-author-policy"),
        policy,
        inventory,
        reference("installed-source-capture")
      )
      .is_err()
  );
}

#[cfg(unix)]
#[tokio::test]
async fn native_delivery_materializer_binds_artifact_ports_without_dependency_io() {
  use std::os::unix::fs::PermissionsExt as _;
  let private = tempfile::tempdir().unwrap();
  std::fs::set_permissions(private.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
  let runtime = resources();
  let material: Arc<dyn FactoryDeliveryMaterializer> = runtime
    .delivery_materializer(BTreeMap::from([(
      managed::reference("installed-delivery-policy", "v1"),
      private.path().to_owned(),
    )]))
    .unwrap();
  drop(material);
  assert!(matches!(
    runtime.delivery_materializer(BTreeMap::new()),
    Err(FactoryRuntimeInstallationError::Invalid)
  ));
}

fn qualified_tool_reasoning_profile() -> FactoryCodexBuildProfile {
  let mut profile = qualified_codex_profile();
  profile.consumer = FactoryCredentialConsumer::ToolReasoning;
  profile.tool_risk = Some(FactoryCodexToolRiskPolicy::from_binding(&tool_risk_profile_fixture().binding).unwrap());
  profile.tool_response_limits = Some(octacity_protocol::FactoryToolResponseLimitsV3 {
    per_call_bytes: 32,
    total_bytes: 64,
  });
  let mut grants = serde_json::to_value(&profile.project_permissions).unwrap();
  grants["secret_profiles"] = serde_json::json!([profile.credentials.profile(FactoryCredentialPurpose::Model)]);
  grants["tools"] = serde_json::json!([managed::reference("installed-read-capability", "v1")]);
  let grants: FactoryPermissionSet = serde_json::from_value(grants).unwrap();
  profile.project_permissions = grants.clone();
  profile.local_permissions = LocalPermissionCeiling::fully_enforced(grants);
  profile
}

#[tokio::test]
async fn protected_tool_profile_qualifies_only_readonly_source_and_exact_model_authority() {
  let resources = resources();
  let profile = qualified_tool_reasoning_profile();
  profile
    .validate_installation(&resources.toolchain)
    .expect("explicit protected tools are distinct from authoring and evaluator profiles");
  {
    let mut bad = profile.clone();
    let mut grants = serde_json::to_value(&bad.project_permissions).unwrap();
    for mount in grants["mounts"].as_array_mut().unwrap() {
      if mount["root"] == "/workspace/source" {
        mount["mode"] = serde_json::json!("read_write");
      }
    }
    bad.project_permissions = serde_json::from_value(grants).unwrap();
    bad.local_permissions = LocalPermissionCeiling::fully_enforced(bad.project_permissions.clone());
    assert!(bad.validate_installation(&resources.toolchain).is_err());
  }
  let mut candidate = profile.clone();
  candidate.candidate_capture_tool = Some(managed::reference("source-reconstruction", "v1"));
  assert!(candidate.validate_installation(&resources.toolchain).is_err());
  let mut no_tools = profile.clone();
  let mut grants = serde_json::to_value(&no_tools.project_permissions).unwrap();
  grants["tools"] = serde_json::json!([]);
  no_tools.project_permissions = serde_json::from_value(grants).unwrap();
  no_tools.local_permissions = LocalPermissionCeiling::fully_enforced(no_tools.project_permissions.clone());
  assert!(no_tools.validate_installation(&resources.toolchain).is_err());
  let mut deterministic = profile.clone();
  deterministic.tool_risk = None;
  deterministic
    .validate_installation(&resources.toolchain)
    .expect("explicit deterministic tools retain signed authority and quotas");
  let mut no_limits = profile.clone();
  no_limits.tool_response_limits = None;
  assert!(no_limits.validate_installation(&resources.toolchain).is_err());
  let mut wrong_credential = profile.clone();
  wrong_credential.consumer = FactoryCredentialConsumer::ReadOnlyReview;
  assert!(wrong_credential.validate_installation(&resources.toolchain).is_err());
}

#[tokio::test]
async fn protected_tool_native_composition_does_not_require_candidate_reconstruction_authority() {
  let resources = resources();
  resources
    .codex_executor(vec![qualified_tool_reasoning_profile()])
    .expect("tool-only native composition must not install a candidate reconstruction source");
}

#[tokio::test]
async fn deployment_protected_tools_selects_one_exact_installed_signal_policy() {
  let profile = qualified_tool_reasoning_profile();
  let mut signal = tool_risk_profile_fixture();
  signal.binding.project_id = profile.build_configuration.project_id();
  signal.build_configuration = profile.build_configuration.clone();
  signal.base_task_digest = profile.base_task_digest();
  let binding = &signal.binding;
  let expected = FactoryCodexToolRiskPolicy::from_binding(binding).unwrap();
  let mut document = codex_catalog_document(&profile);
  document["codex"][0]["credential_purpose"] = serde_json::json!("model");
  document["codex"][0]["execution_mode"] = serde_json::json!("protected_tools");
  document["codex"][0]["tool_response_limits"] = serde_json::json!(profile.tool_response_limits);
  document["tool_risk"] = serde_json::json!([{
    "project_id":binding.project_id,"plan":binding.plan,"mapping":binding.mapping,
    "max_calls":binding.max_calls,"credential_profile":binding.credential_profile,
    "network_host":binding.network_host.as_str(),"project_permissions":binding.project_permissions,
    "local_permissions":{"grants":binding.local_permissions.grants(),"enforceable":PermissionCategory::ALL},
    "base_task_digest":signal.base_task_digest,"build_configuration":signal.build_configuration,
  }]);
  let providers = [FactoryRuntimeSignalConnector::new(
    binding.credential_profile.clone(),
    signal.provider.clone(),
  )];
  let parsed =
    FactoryRuntimeProfileCatalog::parse_json_with_signal_providers(&serde_json::to_vec(&document).unwrap(), &providers)
      .expect("private protected-tools mode resolves only an exact installed connector policy");
  resources()
    .codex_executor(parsed.codex.clone())
    .expect("parsed protected-tools policy is installable without dependency I/O");
  assert_eq!(parsed.codex[0].consumer, FactoryCredentialConsumer::ToolReasoning);
  assert_eq!(parsed.codex[0].tool_risk, Some(expected));
  assert_eq!(parsed.codex[0].base_task_digest(), signal.base_task_digest);
  assert_eq!(
    parsed.codex[0].task_digest(),
    expected.task_digest(signal.base_task_digest)
  );
  let mut missing = document.clone();
  missing["tool_risk"] = serde_json::json!([]);
  assert!(
    FactoryRuntimeProfileCatalog::parse_json_with_signal_providers(&serde_json::to_vec(&missing).unwrap(), &providers)
      .is_err()
  );
  let mut duplicate = document.clone();
  duplicate["tool_risk"]
    .as_array_mut()
    .unwrap()
    .push(document["tool_risk"][0].clone());
  assert!(
    FactoryRuntimeProfileCatalog::parse_json_with_signal_providers(
      &serde_json::to_vec(&duplicate).unwrap(),
      &providers
    )
    .is_err()
  );
  let mut changed = document.clone();
  changed["tool_risk"][0]["base_task_digest"] = serde_json::json!(FactoryDigest::from_bytes([91; 32]));
  assert!(
    FactoryRuntimeProfileCatalog::parse_json_with_signal_providers(&serde_json::to_vec(&changed).unwrap(), &providers)
      .is_err()
  );
  let mut evaluator = document.clone();
  evaluator["codex"][0]["credential_purpose"] = serde_json::json!("evaluator");
  assert!(
    FactoryRuntimeProfileCatalog::parse_json_with_signal_providers(
      &serde_json::to_vec(&evaluator).unwrap(),
      &providers
    )
    .is_err()
  );
  let mut candidate = document;
  candidate["codex"][0]["candidate_capture_tool"] =
    serde_json::json!(managed::reference("not-a-tool-mode-capability", "v1"));
  assert!(
    FactoryRuntimeProfileCatalog::parse_json_with_signal_providers(
      &serde_json::to_vec(&candidate).unwrap(),
      &providers
    )
    .is_err()
  );
}

#[allow(dead_code)]
#[path = "../../../core/octacity-server-factory/src/flow_test_support.rs"]
mod selection_fixtures;
use octacity_server_factory as factory;

#[test]
fn native_codex_selection_keeps_readonly_and_capture_profiles_separate_under_one_cli() {
  let work = selection_fixtures::work();
  let mut readonly = qualified_codex_profile();
  readonly.build_configuration = BuildConfigurationRef::new(
    readonly.build_configuration.id(),
    readonly.build_configuration.version(),
    work.subject().project_id(),
    readonly.build_configuration.definition_digest(),
  );
  let mut coding = readonly.clone();
  coding.consumer = FactoryCredentialConsumer::CodingNode;
  coding.instructions = FactoryText::new("Change only the declared candidate path.").unwrap();
  coding.capture = Some(
    FactoryCodexCapturePolicy::new(
      managed::reference("installed-git-a", "v1"),
      octacity_protocol::ChangeSetCaptureV3 {
        author_name: "Fixture".into(),
        author_email: "fixture@example.test".into(),
        committed_at: 1,
        patch_max_bytes: None,
        observation: None,
        policy: octacity_protocol::ChangeSetCapturePolicyV3 {
          allowed_path_prefixes: vec!["public".into()],
          forbidden_control_paths: vec!["Octafile.yml".into()],
          max_changed_paths: 1,
          max_file_bytes: 1024,
          max_total_bytes: 1024,
          allow_empty: false,
          binary_policy: octacity_protocol::ChangeSetBinaryPolicyV3::Reject,
          forbidden_secret_patterns: octacity_protocol::ChangeSetSecretPatternV3::ALL.to_vec(),
        },
      },
    )
    .unwrap(),
  );
  let mut other = coding.clone();
  other.capture = Some(
    FactoryCodexCapturePolicy::new(
      managed::reference("installed-git-b", "v1"),
      coding.capture.as_ref().unwrap().instruction().clone(),
    )
    .unwrap(),
  );
  let intent = |profile: &FactoryCodexBuildProfile| {
    let mut binding = selection_fixtures::binding(work.subject().project_id());
    binding.tool = profile.toolchain.executable.clone();
    binding.plugin = profile.toolchain.plugin.clone();
    binding.model_or_tool = profile.toolchain.model.clone();
    binding.build_configuration = profile.build_configuration.clone();
    binding.task_digest = profile.task_digest();
    let (_, admitted, _, input, node) = selection_fixtures::execution_fixture_for_work_with_binding(
      FlowNodeKind::Reasoning,
      work.clone(),
      binding,
      None,
      None,
    );
    (
      FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap(),
      admitted,
    )
  };
  let executor = NativeCodexNodeExecutor {
    builds: Arc::new(()),
    outputs: Arc::new(()),
    profiles: vec![coding.clone(), readonly.clone(), other.clone()],
    review_invocations: Arc::new(()),
  };
  let capture_tool = |executor: &NativeCodexNodeExecutor<(), (), ()>,
                      (intent, admitted): &(FlowBuildIntent, AdmittedFlow)| {
    executor
      .profile(intent, admitted)
      .map(|profile| profile.capture.as_ref().map(|capture| capture.tool().clone()))
  };
  assert_eq!(capture_tool(&executor, &intent(&readonly)).unwrap(), None);
  assert_eq!(
    capture_tool(&executor, &intent(&coding)).unwrap().as_ref(),
    coding.capture.as_ref().map(|c| c.tool())
  );
  assert_eq!(
    capture_tool(&executor, &intent(&other)).unwrap().as_ref(),
    other.capture.as_ref().map(|c| c.tool())
  );
  let mut foreign = readonly.clone();
  foreign.instructions = FactoryText::new("Uninstalled instructions.").unwrap();
  assert!(capture_tool(&executor, &intent(&foreign)).is_err());
  let duplicate = NativeCodexNodeExecutor {
    builds: Arc::new(()),
    outputs: Arc::new(()),
    profiles: vec![readonly.clone(), readonly.clone()],
    review_invocations: Arc::new(()),
  };
  assert!(capture_tool(&duplicate, &intent(&readonly)).is_err());
}

#[test]
fn native_codex_dispatch_uses_the_frozen_selected_profile() {
  let work = selection_fixtures::work();
  let mut installed = qualified_codex_profile();
  let mut binding = selection_fixtures::binding(work.subject().project_id());
  installed.build_configuration = binding.build_configuration.clone();
  binding.tool = installed.toolchain.executable.clone();
  binding.plugin = installed.toolchain.plugin.clone();
  binding.model_or_tool = installed.toolchain.model.clone();
  let role = FactoryRole::publish(
    FactoryRoleId::generate(),
    work.subject().project_id(),
    serde_json::from_value(serde_json::json!({"name":"Reviewer","instructions":["Check the contract."]})).unwrap(),
  )
  .unwrap();
  let mut definition = octacity_server_store::testing::role_profile_contract_definition(work.subject().project_id());
  definition.toolchain = installed.toolchain.clone();
  definition.build_configuration = binding.build_configuration.clone();
  definition.permissions = installed.project_permissions.clone();
  definition.credential_purpose = installed.consumer.purpose();
  definition.skills.clear();
  definition.context_requirements.clear();
  definition.budget = BudgetLimit::new(1, 1000, 10, 10, 10000).unwrap();
  let profile = FactoryRoleProfile::publish(
    FactoryRoleProfileId::generate(),
    work.subject().project_id(),
    &role,
    definition,
  )
  .unwrap();
  binding.task_digest = factory_role_task_digest(installed.task_digest(), &profile);
  let (_, admitted, _, input, node) = selection_fixtures::execution_fixture_with_permissions(
    FlowNodeKind::Reasoning,
    work.clone(),
    binding.clone(),
    None,
    installed.project_permissions.clone(),
  );
  let (admitted, input) = selection_fixtures::with_selected_profile(
    &work,
    &admitted,
    &input,
    FlowProfileCandidate {
      profile,
      build: binding,
    },
  );
  let mut node_json = serde_json::to_value(&node).unwrap();
  node_json["input_digest"] = serde_json::json!(input.digest().unwrap());
  node_json["deadline"] = serde_json::json!(1000);
  let node: NodeAttempt = serde_json::from_value(node_json).unwrap();
  let choice = FlowProfileSelectionRecord::select(
    &input,
    &admitted,
    octacity_server_domain::Timestamp::from_unix_millis(0).unwrap(),
  )
  .unwrap();
  let intent = FlowBuildIntent::selected(input, &admitted, node, Default::default(), 0, &choice).unwrap();
  let executor = NativeCodexNodeExecutor {
    builds: Arc::new(()),
    outputs: Arc::new(()),
    profiles: vec![installed.clone()],
    review_invocations: Arc::new(()),
  };
  assert!(
    executor.profile(&intent, &admitted).is_ok(),
    "a frozen role selection must reach its installed compiler"
  );
  let mut changed = installed;
  changed.instructions = FactoryText::new("Substituted installed instructions.").unwrap();
  let substituted = NativeCodexNodeExecutor {
    profiles: vec![changed],
    ..executor
  };
  assert!(substituted.profile(&intent, &admitted).is_err());
}

#[tokio::test]
async fn private_catalog_preserves_the_exact_optional_installed_reviewer_policy() {
  let mut profile = qualified_codex_profile();
  let legacy_digest = profile.task_digest();
  let evaluator = managed::reference("operator-independent-reviewer", "v1");
  profile.reviewer = Some(FactoryCodexReviewerPolicy {
    evaluator: evaluator.clone(),
  });
  profile.candidate_capture_tool = Some(managed::reference("operator-candidate-capture", "v1"));
  let mut document = codex_catalog_document(&profile);
  document["codex"][0]["reviewer"] = serde_json::json!(profile.reviewer);
  document["codex"][0]["candidate_capture_tool"] = serde_json::json!(profile.candidate_capture_tool);
  let parsed = FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&document).unwrap())
    .expect("the private installer must preserve the independently pinned reviewer role");
  assert_eq!(parsed.codex[0].reviewer.as_ref().unwrap().evaluator, evaluator);
  assert_eq!(parsed.codex[0].task_digest(), profile.task_digest());
  assert_ne!(parsed.codex[0].task_digest(), legacy_digest);
  resources().codex_executor(parsed.codex.clone()).unwrap();
  let mut changed = document.clone();
  changed["codex"][0]["reviewer"]["evaluator"] = serde_json::json!(managed::reference("another-reviewer", "v1"));
  let changed = FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&changed).unwrap()).unwrap();
  assert_ne!(changed.codex[0].task_digest(), profile.task_digest());
  for field in ["candidate_capture_tool", "credential_purpose"] {
    let mut invalid = document.clone();
    invalid["codex"][0][field] = if field == "credential_purpose" {
      serde_json::json!("model")
    } else {
      serde_json::Value::Null
    };
    if let Ok(parsed) = FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&invalid).unwrap()) {
      assert!(
        resources().codex_executor(parsed.codex).is_err(),
        "invalid reviewer must fail before dependency I/O"
      );
    }
  }
  let legacy = FactoryRuntimeProfileCatalog::parse_json(
    &serde_json::to_vec(&codex_catalog_document(&qualified_codex_profile())).unwrap(),
  )
  .unwrap();
  assert!(legacy.codex[0].reviewer.is_none());
}
