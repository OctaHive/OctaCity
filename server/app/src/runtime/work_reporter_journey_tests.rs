//! Public ordinary admission/action/outbox journey through the content-verified process boundary.
use super::*;
use crate::runtime::*;
use octacity_server_factory::NetworkHost;
use octacity_work_source_protocol::{ConfigurationReference, CredentialClass, CredentialHandle, ResourceAccess};
#[path = "work_reporter_ordering_tests.rs"]
mod ordering;
#[path = "work_reporter_recovery_tests.rs"]
mod recovery;

// Call from a start_paused test. Tokio documents that a blocking task inhibits
// automatic clock advancement; block_on still runs real PostgreSQL/process I/O.
// Each test owns its runtime and blocking task, without a lock or a busy loop.
async fn controlled_io<F>(future: F) -> F::Output
where
  F: std::future::Future + Send + 'static,
  F::Output: Send + 'static,
{
  let runtime = tokio::runtime::Handle::current();
  tokio::task::spawn_blocking(move || runtime.block_on(future))
    .await
    .unwrap()
}
// External process barriers use a real watchdog, independent of the frozen clock.
async fn wait_for_provider(mut ready: impl FnMut() -> bool + Send + 'static) {
  tokio::task::spawn_blocking(move || {
    let started = std::time::Instant::now();
    while !ready() {
      assert!(started.elapsed() < TEST_WAIT, "provider did not reach the test barrier");
      std::thread::sleep(Duration::from_millis(5));
    }
  })
  .await
  .unwrap();
}

fn capability() -> ImmutableReference {
  exact("source-status", 61)
}
fn input_schema() -> FlowDataSchema {
  FlowDataSchema::new(key("report.input"), key("v1"), FlowValueSchema::Boolean).unwrap()
}
fn receipt_schema() -> FlowDataSchema {
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
fn outcome() -> FlowOutcomeDefinition {
  FlowOutcomeDefinition::new(
    key("done"),
    FlowOutcomeKind::Success,
    receipt_schema().reference().clone(),
  )
}
fn flow(provider: &str) -> FactoryFlowConfiguration {
  flow_with_authority(provider, None)
}
fn flow_with_authority(provider: &str, omit: Option<&str>) -> FactoryFlowConfiguration {
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

async fn fixture(provider: &str) -> (Fixture, FactoryReporterProfile) {
  fixture_with_authority(provider, None).await
}
async fn fixture_with_authority(provider: &str, omit: Option<&str>) -> (Fixture, FactoryReporterProfile) {
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
    project_id: fixture.input.project_id(),
    installation: fixture.profile.binding.installation.clone(),
    capability: capability(),
    resource: fixture.profile.binding.resource.clone(),
    plugin: fixture.profile.plugin.clone(),
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
    binding: fixture.profile.binding.clone(),
  };
  let reporter = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Reporter,
    binding: profile.binding().clone(),
  };
  let mut draft = fixture.input.definition().clone();
  draft.reporter = Some(profile.binding().clone());
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
  std::fs::OpenOptions::new().append(true).open(dir.join("adapter.toml")).unwrap().write_all(format!("\n[protocol.reporter]\nmin=1\nmax=2\n[capabilities.reporter]\nobserve=true\nproject=true\ncancel=true\nidempotency=\"observe_before_retry\"\ncredential_class=\"work_reporter_write\"\nconfiguration_digest=\"{}\"\n","d".repeat(64)).as_bytes()).unwrap();
  (fixture, profile)
}
fn hosted<S: FactorySourceStore + FactoryRunStore + 'static>(
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
async fn queue<S: WorkSourceArtifactStore + FactoryDiscoveryStore + FactoryRunStore + 'static>(
  fixture: &Fixture<S>,
  projector: Arc<HostedFactoryWorkStatusProjector<S>>,
) -> FactoryRunId {
  queue_at(fixture, projector, 100).await
}
async fn queue_at<S: WorkSourceArtifactStore + FactoryDiscoveryStore + FactoryRunStore + 'static>(
  fixture: &Fixture<S>,
  projector: Arc<HostedFactoryWorkStatusProjector<S>>,
  base: i64,
) -> FactoryRunId {
  let run = admit_at(fixture, base).await;
  accept_status_at(fixture, projector, run, base).await;
  run
}
// Admission can be prepared before a race; the status intent is accepted later.
async fn admit_at<S: WorkSourceArtifactStore + FactoryDiscoveryStore + FactoryRunStore + 'static>(
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
async fn accept_status_at<S: FactorySourceStore + FactoryRunStore + 'static>(
  fixture: &Fixture<S>,
  projector: Arc<HostedFactoryWorkStatusProjector<S>>,
  run: FactoryRunId,
  base: i64,
) {
  fixture.clock.0.store(base, Ordering::SeqCst);
  let dir = fixture.root.path().join(fixture.profile.plugin.identity().as_str());
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
#[tokio::test]
async fn accepted_source_status_reaches_only_its_original_verified_reporter_for_every_provider() {
  for provider in ["github", "gitlab", "jira", "custom-tracker"] {
    let (fixture, profile) = fixture(provider).await;
    let projector = hosted(&fixture, vec![profile]);
    let run = queue(&fixture, projector.clone()).await;
    let before = fixture.store.factory_run_snapshot(run).await.unwrap();
    let operation = before
      .outbox
      .iter()
      .find(|row| row.kind.as_str() == "work.status.projection")
      .unwrap()
      .operation_id;
    let node = before
      .admitted_flow
      .closure()
      .definition(before.admitted_flow.closure().root())
      .unwrap()
      .node(&key("report-any-stage"))
      .unwrap();
    let pending = FactoryWorkStatusRequest {
      operation_id: operation,
      outbox_claim: None,
      work_id: before.work.id(),
      external_identity: before.work.external_identity().clone(),
      subject: before.work.subject().clone(),
      status: FactorySafeText::new("needs-human-eyes").unwrap(),
      permissions: node.permissions().clone(),
      budget: node.budget(),
      deadline: time(5000),
    };
    assert!(
      projector.set_status(pending).await.is_err(),
      "a pending outbox row alone cannot authorize a write"
    );
    let worker = FactoryWorkStatusOutboxWorker::new(
      fixture.store.clone(),
      std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
      FactoryReporterWorkerSettings::new(key("reporting"), 10000, 4).unwrap(),
      fixture.clock.clone(),
    )
    .unwrap();
    assert_eq!(worker.run_once(time(120)).await.unwrap().delivered, 1);
    let calls = std::fs::read_to_string(fixture.root.path().join(provider).join("reporter-requests.jsonl")).unwrap();
    let calls = calls
      .lines()
      .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
      .collect::<Vec<_>>();
    assert_eq!(calls.len(), 4);
    let operations: Vec<_> = calls
      .iter()
      .map(|call| call["message"]["command"]["operation"].as_str().unwrap())
      .collect();
    assert_eq!(operations, ["inspect", "inspect", "apply", "inspect"]);
    for call in calls {
      let payload = original_intent(&call);
      assert_eq!(payload["projection_id"], operation.to_string());
      assert_eq!(payload["item_id"], "TASK-native");
      assert_eq!(payload["display_id"], "TASK-7");
      assert_eq!(payload["mapping"]["identity"], "original-effects");
      assert_eq!(payload["resource"]["credential"]["class"], "work_reporter_write");
      assert_eq!(payload["status"], "needs-human-eyes");
    }
    let after = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(before.flow, after.flow);
    assert_eq!(before.run, after.run);
    assert_eq!(worker.run_once(time(121)).await.unwrap().claimed, 0);
  }
}

#[tokio::test]
async fn reporter_retry_lower_bounds_and_exhaustion_survive_restart_without_reopening_the_flow() {
  let (fixture, profile) = fixture("jira").await;
  let projector = hosted(&fixture, vec![profile.clone()]);
  let run = queue(&fixture, projector.clone()).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let dir = fixture.root.path().join("jira");
  std::fs::write(
    dir.join("reporter-mode.json"),
    br#"{"failure":{"class":"transient","retry_after_millis":2000}}"#,
  )
  .unwrap();
  let worker = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("first-reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  assert_eq!(worker.run_once(time(120)).await.unwrap().retrying, 1);
  let next = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert!(
    next
      .outbox
      .iter()
      .any(|row| row.state == FactoryOutboxState::Pending && row.attempt == 1 && row.available_at == time(2120)),
    "the provider lower bound must exceed the configured 100ms delay"
  );
  let projector = hosted(&fixture, vec![profile]);
  let restarted = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("restarted-reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  fixture.clock.0.store(2119, Ordering::SeqCst);
  assert_eq!(restarted.run_once(time(2119)).await.unwrap().claimed, 0);
  fixture.clock.0.store(2120, Ordering::SeqCst);
  assert_eq!(restarted.run_once(time(2120)).await.unwrap().failed, 1);
  assert_eq!(restarted.run_once(time(2200)).await.unwrap().claimed, 0);
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(before.flow, after.flow);
  assert_eq!(before.run, after.run);
  let calls = std::fs::read_to_string(dir.join("reporter-requests.jsonl")).unwrap();
  let calls = calls
    .lines()
    .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
    .collect::<Vec<_>>();
  assert_eq!(calls.len(), 2);
  assert_eq!(
    calls[0]["message"]["command"]["payload"]["projection_id"],
    calls[1]["message"]["command"]["payload"]["projection_id"]
  );
  assert!(!dir.join("reporter-applied").exists());
}

#[tokio::test]
async fn missing_action_authority_is_denied_before_exposing_a_write_handle_to_the_process() {
  for omitted in ["grant", "network", "plugin"] {
    let (fixture, profile) = fixture_with_authority("gitlab", Some(omitted)).await;
    let projector = hosted(&fixture, vec![profile]);
    let run = queue(&fixture, projector.clone()).await;
    let before = fixture.store.factory_run_snapshot(run).await.unwrap();
    let worker = FactoryWorkStatusOutboxWorker::new(
      fixture.store.clone(),
      std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
      FactoryReporterWorkerSettings::new(key("reporter"), 10000, 4).unwrap(),
      fixture.clock.clone(),
    )
    .unwrap();
    assert_eq!(worker.run_once(time(120)).await.unwrap().failed, 1, "{omitted}");
    let after = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(before.flow, after.flow);
    assert_eq!(before.run, after.run);
    assert!(!fixture.root.path().join("gitlab/reporter-requests.jsonl").exists());
  }
}

#[tokio::test]
async fn disabling_or_replacing_the_current_input_never_redirects_an_accepted_projection() {
  let (mut fixture, profile) = fixture("custom-tracker").await;
  let projector = hosted(&fixture, vec![profile]);
  let run = queue(&fixture, projector.clone()).await;
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
    binding: fixture.profile.binding.clone(),
  };
  let mut draft = fixture.input.definition().clone();
  draft.enabled = false;
  draft.reporter = None;
  let replacement = fixture
    .input
    .replace(
      WorkSourceInputVersion::new(3).unwrap(),
      draft,
      WorkSourceInputContext {
        target: &target.configuration.configuration,
        repository: &fixture.input.definition().repository,
        source: Some(&source),
        reporter: None,
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
  let worker = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  assert_eq!(worker.run_once(time(120)).await.unwrap().delivered, 1);
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  let retained = fixture
    .store
    .source_preparation_for_work(fixture.input.project_id(), after.work.id())
    .await
    .unwrap()
    .unwrap();
  assert_eq!(retained.input.version(), WorkSourceInputVersion::new(2).unwrap());
  let calls = std::fs::read_to_string(fixture.root.path().join("custom-tracker/reporter-requests.jsonl")).unwrap();
  assert!(calls.lines().all(
    |line| original_intent(&serde_json::from_str::<serde_json::Value>(line).unwrap())["mapping"]["identity"]
      == "original-effects"
  ));
}

#[tokio::test]
async fn unavailable_reporter_is_local_and_manual_factory_intake_requires_no_reporter() {
  let (fixture, profile) = fixture("github").await;
  let initial = hosted(&fixture, vec![profile]);
  let run = queue(&fixture, initial).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let unavailable = hosted(&fixture, vec![]);
  let worker = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), unavailable as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  assert_eq!(worker.run_once(time(120)).await.unwrap().retrying, 1);
  assert!(!fixture.root.path().join("github/reporter-requests.jsonl").exists());
  let source = fixture
    .store
    .source_preparation_for_work(fixture.input.project_id(), before.work.id())
    .await
    .unwrap()
    .unwrap();
  let mut command = source.intent.unwrap();
  command.external_identity = ExternalWorkIdentity::new("manually-entered-task").unwrap();
  command.idempotency_key = IdempotencyKey::new("manual-during-reporter-outage").unwrap();
  command.admitted_at = time(121);
  let manual = FactoryAdmissionHandlers::new(fixture.store.clone(), Arc::new(ExactRevisionResolver))
    .execute_management_command(
      &fixture.profile.context,
      &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
      command,
    )
    .await
    .unwrap();
  assert_ne!(manual.factory_run_id, run);
  assert!(
    fixture
      .store
      .source_preparation_for_work(fixture.input.project_id(), manual.work_envelope_id)
      .await
      .unwrap()
      .is_none()
  );
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(before.flow, after.flow);
  assert_eq!(before.run, after.run);
  assert!(!fixture.root.path().join("github/reporter-requests.jsonl").exists());
}

#[tokio::test]
async fn reporter_timeout_and_original_action_deadline_do_not_authorize_a_fresh_execution_window() {
  let (fixture, profile) = fixture("jira").await;
  let projector = Arc::new(
    HostedFactoryWorkStatusProjector::new(
      fixture.store.clone(),
      Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
      vec![profile],
      fixture.clock.clone(),
      Duration::from_millis(100),
      Duration::from_millis(20),
      CancellationToken::new(),
    )
    .unwrap(),
  );
  let run = queue(&fixture, projector.clone()).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let dir = fixture.root.path().join("jira");
  std::fs::write(dir.join("reporter-mode.json"), br#"{"delay":5}"#).unwrap();
  let worker = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  let result = tokio::time::timeout(Duration::from_secs(2), worker.run_once(time(120)))
    .await
    .unwrap()
    .unwrap();
  assert_eq!(result.retrying, 1);
  // A deadline may expire before a loaded host starts Python; zero requests
  // and one incomplete observation both forbid writes and a later restart.
  let captured = captured_requests(&dir);
  assert!(captured.lines().count() <= 1);
  fixture.clock.0.store(6000, Ordering::SeqCst);
  assert_eq!(worker.run_once(time(6000)).await.unwrap().failed, 1);
  assert_eq!(
    captured_requests(&dir),
    captured,
    "expiry must be checked before another process starts"
  );
  assert!(!dir.join("reporter-applied").exists());
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(before.flow, after.flow);
  assert_eq!(before.run, after.run);
}

fn captured_requests(dir: &std::path::Path) -> String {
  std::fs::read_to_string(dir.join("reporter-requests.jsonl")).unwrap_or_else(|error| {
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    String::new()
  })
}

#[tokio::test]
async fn an_observation_that_exhausts_the_action_output_budget_cannot_start_a_write() {
  let (fixture, profile) = fixture_with_authority("jira", Some("output")).await;
  let projector = hosted(&fixture, vec![profile]);
  let run = queue(&fixture, projector.clone()).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let worker = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  assert_eq!(worker.run_once(time(120)).await.unwrap().failed, 1);
  assert!(
    !fixture.root.path().join("jira/reporter-applied").exists(),
    "an exhausted observation budget cannot authorize projection"
  );
  assert_eq!(before.flow, fixture.store.factory_run_snapshot(run).await.unwrap().flow);
}

fn original_intent(call: &serde_json::Value) -> &serde_json::Value {
  let payload = &call["message"]["command"]["payload"];
  payload.get("intent").unwrap_or(payload)
}

#[tokio::test]
async fn managed_reporter_revocation_blocks_retained_retry_without_rewriting_work_or_flow() {
  let (fixture, profile) = fixture("custom-tracker").await;
  let (access, id) = managed_access(&fixture, profile.binding(), IntegrationPurpose::TaskUpdate).await;
  let profile = profile.with_managed_access(access);
  let projector = hosted(&fixture, vec![profile]);
  let run = queue(&fixture, projector.clone()).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let dir = fixture.root.path().join("custom-tracker");
  std::fs::write(
    dir.join("reporter-mode.json"),
    br#"{"failure":{"class":"transient","retry_after_millis":200}}"#,
  )
  .unwrap();
  let worker = FactoryWorkStatusOutboxWorker::new(
    fixture.store.clone(),
    std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("managed-reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  assert_eq!(worker.run_once(time(120)).await.unwrap().retrying, 1);
  let captured = std::fs::read(dir.join("reporter-requests.jsonl")).unwrap();
  fixture
    .store
    .publish_integration_project_access(octacity_server_store::testing::management_mutation(
      PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id: id,
          connection_version: IntegrationVersion::INITIAL,
          project_id: fixture.input.project_id(),
          expected_current_version: Some(IntegrationVersion::INITIAL),
          resources: vec![],
        },
        idempotency_key: IdempotencyKey::new("revoke-managed-write").unwrap(),
        published_at: time(200),
      },
    ))
    .await
    .unwrap();
  std::fs::write(dir.join("reporter-mode.json"), b"{}").unwrap();
  fixture.clock.0.store(320, Ordering::SeqCst);
  assert_eq!(worker.run_once(time(320)).await.unwrap().claimed, 1);
  assert_eq!(std::fs::read(dir.join("reporter-requests.jsonl")).unwrap(), captured);
  assert!(!dir.join("reporter-applied").exists());
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(after.work, before.work);
  assert_eq!(after.run, before.run);
  assert_eq!(after.flow, before.flow);
}
