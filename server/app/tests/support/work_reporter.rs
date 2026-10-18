//! Shared configured action/Reporter fixtures; all execution uses public contracts.
use super::*;
pub(super) fn capability() -> ImmutableReference {
  exact("source-status", 61)
}
pub(super) fn input_schema() -> FlowDataSchema {
  FlowDataSchema::new(key("report.input"), key("v1"), FlowValueSchema::Boolean).unwrap()
}
pub(super) fn receipt_schema() -> FlowDataSchema {
  FlowDataSchema::new(
    key("report.receipt"),
    key("v1"),
    FlowValueSchema::Object {
      fields: std::collections::BTreeMap::from([(
        key("receipt"),
        FlowFieldSchema::required(FlowValueSchema::String {
          min_bytes: 64,
          max_bytes: 64,
        }),
      )]),
    },
  )
  .unwrap()
}
pub(super) fn outcome() -> FlowOutcomeDefinition {
  FlowOutcomeDefinition::new(
    key("done"),
    FlowOutcomeKind::Success,
    receipt_schema().reference().clone(),
  )
}
pub(super) fn flow(provider: &str) -> FactoryFlowConfiguration {
  flow_with_authority(provider, None)
}
pub(super) fn flow_with_authority(provider: &str, omit: Option<&str>) -> FactoryFlowConfiguration {
  let bytes =
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime/fixtures/source-provider.py"))
      .unwrap();
  let plugin = ImmutableReference::new(key(provider), key("v1"), FactoryDigest::content_sha256(&bytes));
  let mut ceiling = FactoryPermissionDraft {
    plugins: vec![capability(), plugin],
    network_hosts: vec![NetworkHost::new("tracker.invalid").unwrap()],
    secret_profiles: vec![key("writer")],
    ..Default::default()
  };
  match omit {
    Some("grant") => ceiling.secret_profiles.clear(),
    Some("network") => ceiling.network_hosts.clear(),
    Some("plugin") => ceiling.plugins.retain(|p| p == &capability()),
    _ => {}
  }
  let permissions = FactoryPermissionSet::try_new(ceiling).unwrap();
  let budget = BudgetLimit::new(10, 10000, 100, 100, 10000).unwrap();
  let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("report-any-stage"),
    kind: FlowNodeKind::TrustedAction,
    input_schema: Some(input_schema().reference().clone()),
    outcomes: vec![outcome()],
    budget: BudgetLimit::new(1, 5000, 0, 0, if omit == Some("output") { 100 } else { 10000 }).unwrap(),
    permissions: permissions.clone(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_action(
    FlowActionBinding::new(
      capability(),
      key("set_status"),
      serde_json::json!({"status":"needs-human-eyes","outbox":{"max_attempts":2,"retry_delay_millis":100}}),
    )
    .unwrap(),
  )
  .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(input_schema().reference().clone()),
    entry: node.key().clone(),
    nodes: vec![node],
    transitions: vec![FlowTransition::new(
      key("report-any-stage"),
      key("done"),
      FlowTransitionTarget::Terminal(key("done")),
    )],
    terminals: vec![FlowTerminalDefinition::new(
      key("done"),
      receipt_schema().reference().clone(),
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, permissions.clone(), 1).unwrap(),
  })
  .unwrap();
  FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, budget, permissions).unwrap(),
    vec![receipt_schema(), input_schema()],
    Default::default(),
  )
  .unwrap()
}

pub(super) async fn fixture(provider: &str) -> (Fixture, FactoryReporterProfile) {
  fixture_with_authority(provider, None).await
}
pub(super) async fn fixture_with_authority(provider: &str, omit: Option<&str>) -> (Fixture, FactoryReporterProfile) {
  fixture_with_reporting(provider, omit, false).await
}
pub(super) async fn fixture_with_reporting(
  provider: &str,
  omit: Option<&str>,
  custom: bool,
) -> (Fixture, FactoryReporterProfile) {
  fixture_with_reporting_policy(
    provider,
    omit,
    custom.then(|| WorkSourceReportingPolicy {
      labels: vec!["custom-ready".into()],
      states: vec![],
    }),
  )
  .await
}
pub(super) async fn fixture_with_reporting_policy(
  provider: &str,
  omit: Option<&str>,
  policy: Option<WorkSourceReportingPolicy>,
) -> (Fixture, FactoryReporterProfile) {
  let mut fixture = Fixture::enabled_with_flow(
    provider,
    Some(if omit.is_none() {
      flow(provider)
    } else {
      flow_with_authority(provider, omit)
    }),
  )
  .await;
  let draft = FactoryReporterProfileDraft {
    reporting_policy: policy.clone(),
    project_id: fixture.input.project_id(),
    installation: fixture.profile.read.binding.installation.clone(),
    capability: capability(),
    resource: fixture.profile.read.binding.resource.clone(),
    plugin: fixture.profile.read.plugin.clone(),
    provider_configuration: FactoryDigest::from_bytes([0xdd; 32]),
    access: ResourceAccess {
      resource_id: "native-container".into(),
      resource_locator: "private-reporter-container".into(),
      credential: CredentialHandle {
        class: CredentialClass::WorkReporterWrite,
        handle: "private-reporter-grant".into(),
      },
    },
    mapping: ConfigurationReference {
      identity: "original-effects".into(),
      sha256: "c".repeat(64),
    },
    credential_profile: key("writer"),
    network_hosts: vec!["tracker.invalid".into()],
  };
  let profile = FactoryReporterProfile::new(key("reporter-settings"), key("v1"), draft).unwrap();
  let target = fixture
    .store
    .factory_admission_context(ReadFactoryAdmissionContext {
      project_id: fixture.input.project_id(),
      configuration_id: fixture.input.definition().target.id(),
      configuration_version: fixture.input.definition().target.version(),
      repository_id: fixture.input.definition().repository.id,
      repository_version: fixture.input.definition().repository.version,
    })
    .await
    .unwrap();
  let source = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: fixture.profile.read.binding.clone(),
  };
  let reporter = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Reporter,
    binding: profile.binding().clone(),
  };
  let mut draft = fixture.input.definition().clone();
  draft.reporter = Some(profile.binding().clone());
  draft.reporting = policy.map(|policy| {
    serde_json::from_value(
      serde_json::json!({"statuses":[{"status":"needs-human-eyes","add":[policy.labels[0]],"remove":[],"state":null}]}),
    )
    .unwrap()
  });
  let replacement = fixture
    .input
    .replace(
      WorkSourceInputVersion::new(2).unwrap(),
      draft,
      WorkSourceInputContext {
        target: &target.configuration.configuration,
        repository: &fixture.input.definition().repository,
        source: Some(&source),
        reporter: Some(&reporter),
      },
    )
    .unwrap();
  fixture
    .store
    .publish_work_sources(PublishWorkSources {
      project_id: fixture.input.project_id(),
      inputs: vec![WorkSourceInputChange {
        expected_version: Some(fixture.input.version()),
        input: replacement.clone(),
      }],
      group: None,
    })
    .await
    .unwrap();
  fixture.input = replacement;
  let dir = fixture.root.path().join(provider);
  use std::io::Write as _;
  std::fs::OpenOptions::new().append(true).open(dir.join("adapter.toml")).unwrap().write_all(format!("\n[protocol.reporter]\nmin=1\nmax=2\n[capabilities.reporter]\ncustom_effects=true\nobserve=true\nproject=true\ncancel=true\nidempotency=\"observe_before_retry\"\ncredential_class=\"work_reporter_write\"\nconfiguration_digest=\"{}\"\n","d".repeat(64)).as_bytes()).unwrap();
  (fixture, profile)
}
pub(super) fn hosted<S: FactorySourceStore + FactoryRunStore + 'static>(
  fixture: &Fixture<S>,
  profiles: Vec<FactoryReporterProfile>,
) -> Arc<HostedFactoryWorkStatusProjector<S>> {
  Arc::new(
    HostedFactoryWorkStatusProjector::new(
      fixture.store.clone(),
      Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
      profiles,
      fixture.clock.clone(),
      Duration::from_secs(2),
      Duration::from_millis(20),
      CancellationToken::new(),
    )
    .unwrap(),
  )
}
pub(super) async fn queue<S: WorkSourceArtifactStore + FactoryDiscoveryStore + FactoryRunStore + 'static>(
  fixture: &Fixture<S>,
  projector: Arc<HostedFactoryWorkStatusProjector<S>>,
) -> FactoryRunId {
  queue_at(fixture, projector, 100).await
}
pub(super) async fn queue_at<S: WorkSourceArtifactStore + FactoryDiscoveryStore + FactoryRunStore + 'static>(
  fixture: &Fixture<S>,
  projector: Arc<HostedFactoryWorkStatusProjector<S>>,
  base: i64,
) -> FactoryRunId {
  let run = admit_at(fixture, base).await;
  accept_status_at(fixture, projector, run, base).await;
  run
}
// Admission can be prepared before a race; the status intent is accepted later.
pub(super) async fn admit_at<S: WorkSourceArtifactStore + FactoryDiscoveryStore + FactoryRunStore + 'static>(
  fixture: &Fixture<S>,
  base: i64,
) -> FactoryRunId {
  fixture.clock.0.store(base, Ordering::SeqCst);
  let existing: Vec<_> = fixture.runs().await.iter().map(|r| r.id).collect();
  assert_eq!(
    fixture
      .worker()
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  fixture
    .runs()
    .await
    .into_iter()
    .find(|r| !existing.contains(&r.id))
    .unwrap()
    .id
}
pub(super) async fn accept_status_at<S: FactorySourceStore + FactoryRunStore + 'static>(
  fixture: &Fixture<S>,
  projector: Arc<HostedFactoryWorkStatusProjector<S>>,
  run: FactoryRunId,
  base: i64,
) {
  fixture.clock.0.store(base, Ordering::SeqCst);
  let dir = fixture
    .root
    .path()
    .join(fixture.profile.read.plugin.identity().as_str());
  let captured = captured_requests(&dir);
  let snapshot = fixture.store.factory_run_snapshot(run).await.unwrap();
  let claim = FactoryRunClaimRecord::new(
    run,
    key("owner"),
    FactoryClaim::new(FactoryClaimFence::new(digest(91)), time(base + 10), time(base + 9900)).unwrap(),
  );
  fixture
    .store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run,
      expected_version: snapshot.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run,
        AuditActorKind::Worker,
        None,
        key("claimed"),
        digest(92),
        key("accepted"),
        time(base + 10),
      ),
    })
    .await
    .unwrap();
  let target = FactoryNodeTarget {
    flow_run_id: snapshot.admitted_flow.root_run().id(),
    cycle_id: snapshot.admitted_flow.initial_cycle().id(),
    node: key("report-any-stage"),
  };
  let action = FactoryWorkStatusAction::new(capability(), projector, outcome());
  let runner = FactoryNodeRunner::new(
    fixture.store.clone(),
    Arc::new(FactoryFlowActionAdapter::new(Arc::new(action))),
    fixture.clock.clone(),
  );
  let input = FlowNodeInput::new(
    &snapshot.work,
    &snapshot.admitted_flow,
    snapshot.admitted_flow.root_run(),
    snapshot.admitted_flow.initial_cycle(),
    key("report-any-stage"),
    FlowPayload::new(&input_schema(), serde_json::json!(true)).unwrap(),
  )
  .unwrap();
  runner
    .run_once(run, claim.id, target.clone(), Some(input), time(base + 11))
    .await
    .unwrap();
  assert!(matches!(
    runner
      .run_once(run, claim.id, target, None, time(base + 12))
      .await
      .unwrap(),
    FactoryNodeStep::Completed(_)
  ));
  assert_eq!(
    captured_requests(&dir),
    captured,
    "accepting the action only queues the outbox"
  );
  fixture.clock.0.store(base + 20, Ordering::SeqCst);
}

pub(super) fn captured_requests(dir: &std::path::Path) -> String {
  std::fs::read_to_string(dir.join("reporter-requests.jsonl")).unwrap_or_else(|error| {
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    String::new()
  })
}

pub(super) fn original_intent(call: &serde_json::Value) -> &serde_json::Value {
  let payload = &call["message"]["command"]["payload"];
  payload.get("intent").unwrap_or(payload)
}

pub(super) fn reporting_worker<S: FactoryRunStore>(
  fixture: &Fixture<S>,
  projector: Arc<dyn FactoryWorkStatusProjector>,
  owner: &str,
  claim_millis: u64,
) -> FactoryWorkStatusOutboxWorker<S> {
  FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), projector)]),
    FactoryReporterWorkerSettings::new(key(owner), claim_millis, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap()
}
