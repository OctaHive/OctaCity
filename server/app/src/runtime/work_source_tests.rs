//! Source worker contracts through real stores and the verified provider process boundary.
use super::work_source::*;
use octacity_server_application::*;
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_store::{testing::InMemoryFactoryConfigurationStore, *};
use std::{
  sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
  },
  time::Duration,
};
use tokio_util::sync::CancellationToken;
// Watchdogs bound test failures; process scheduling speed is not an admission contract.
const TEST_WAIT: Duration = Duration::from_secs(30);
#[path = "work_source_configuration_tests.rs"]
mod configuration;
struct Clock(AtomicI64);
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(Timestamp::from_unix_millis(self.0.load(Ordering::SeqCst)).unwrap())
  }
}
#[derive(Default)]
struct Bytes(std::sync::Mutex<Vec<Vec<u8>>>);
#[async_trait::async_trait]
impl octacity_artifact_store::ArtifactBytePublisher for Bytes {
  async fn publish_verified(
    &self,
    _: &octacity_artifact_store::ArtifactObject,
    bytes: Vec<u8>,
  ) -> Result<(), octacity_artifact_store::ArtifactStoreError> {
    self.0.lock().unwrap().push(bytes);
    Ok(())
  }
}
fn settings() -> FactorySourceWorkerSettings {
  FactorySourceWorkerSettings {
    worker: toml::from_str("poll_interval_milliseconds=10\nclaim_lifetime_milliseconds=60000\nbatch_size=4\nconcurrency=2\n[retry]\nmax_backoff_milliseconds=40\nconsecutive_failure_limit=3\ncircuit_open_milliseconds=200").unwrap(),
    max_attempts: 3,
    operation_timeout: Duration::from_secs(10),
    cancellation_grace: Duration::from_secs(1),
  }
}
#[tokio::test]
async fn absent_inputs_and_precancelled_workers_never_require_a_provider_or_credentials() {
  let store = Arc::new(InMemoryFactoryConfigurationStore::new());
  let project = ProjectId::generate();
  store.seed_project(project).unwrap();
  let bytes = Arc::new(Bytes::default());
  let root = tempfile::tempdir().unwrap();
  let worker = FactorySourceWorker::new(
    FactorySourceWorkerDependencies {
      store,
      bytes: bytes.clone(),
      revisions: Arc::new(ExactRevisionResolver),
      authorization: Arc::new(TrustedNetworkManagementPolicy),
      registry: Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(root.path()).unwrap()),
      clock: Arc::new(Clock(AtomicI64::new(100))),
    },
    FactorySourceInstallation {
      projects: vec![project],
      profiles: vec![],
      settings: settings(),
    },
  )
  .unwrap();
  assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().claimed, 0);
  let cancellation = CancellationToken::new();
  cancellation.cancel();
  assert_eq!(worker.run_once(cancellation).await.unwrap().claimed, 0);
  assert!(bytes.0.lock().unwrap().is_empty());
}

use std::collections::BTreeSet;
include!("../../../application/src/factory_admission_test_fixture.rs");
fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
struct Fixture<S = InMemoryFactoryConfigurationStore> {
  root: tempfile::TempDir,
  store: Arc<S>,
  bytes: Arc<Bytes>,
  clock: Arc<Clock>,
  input: WorkSourceInput,
  profile: FactorySourceProfile,
}
impl Fixture {
  async fn wait_for_provider_file(&self, name: &str) {
    let path = self
      .root
      .path()
      .join(self.profile.plugin.identity().as_str())
      .join(name);
    tokio::time::timeout(TEST_WAIT, async {
      while !path.exists() {
        tokio::time::sleep(Duration::from_millis(5)).await;
      }
    })
    .await
    .expect("the controlled provider must reach its requested phase");
  }

  fn release_provider(&self) {
    std::fs::write(
      self
        .root
        .path()
        .join(self.profile.plugin.identity().as_str())
        .join("release"),
      b"release",
    )
    .unwrap();
  }

  async fn enabled(provider: &str) -> Self {
    Self::enabled_with_flow(provider, None).await
  }
  async fn enabled_with_flow(provider: &str, flow: Option<FactoryFlowConfiguration>) -> Self {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(provider);
    std::fs::create_dir(&dir).unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime/fixtures/source-provider.py");
    std::fs::hard_link(&script, dir.join("plugin")).unwrap();
    let plugin_digest = FactoryDigest::content_sha256(&std::fs::read(script).unwrap());
    std::fs::write(dir.join("adapter.toml"),format!("manifest_version=1\nadapter_id={provider:?}\nexecutable=\"plugin\"\nexecutable_sha256=\"{plugin_digest}\"\n[protocol.source]\nmin=1\nmax=1\n[capabilities.source]\nauthorize=true\ncancel=true\ncredential_class=\"work_source_read\"\nconfiguration_digest=\"{}\"\n","e".repeat(64))).unwrap();
    std::fs::write(dir.join("mode.json"),br#"{"candidates":[{"container_id":"native-container","item_id":"TASK-native","display_id":"TASK-7","update_identity":"same-time","title":"Original task","body":"Explicit acceptance","fields":{},"labels":["arbitrary"],"state":"native-state"}]}"#).unwrap();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let project = ProjectId::generate();
    store.seed_project(project).unwrap();
    let config = configuration_with_flow(project, FactoryConfigurationId::generate(), flow);
    store
      .seed_factory_configuration_version(PublishedFactoryConfiguration {
        configuration: config.clone(),
        published_at: time(1),
      })
      .unwrap();
    let repository = repository(project, RepositoryId::generate(), None);
    store.seed_repository_version(repository.clone()).unwrap();
    let binding = WorkSourceBinding {
      installation: exact(provider, 30),
      capability: exact("source", 31),
      configuration: exact("source-input", 32),
      resource: exact("native-space", 33),
    };
    let capability = WorkSourceCapability {
      project_id: project,
      role: WorkSourceCapabilityRole::Source,
      binding: binding.clone(),
    };
    let repo = WorkSourceRepositoryRef {
      project_id: project,
      id: repository.id,
      version: repository.version,
      definition_digest: FactoryDigest::content_sha256(&serde_json::to_vec(&repository.definition).unwrap()),
    };
    let input = WorkSourceInput::publish(
      WorkSourceInputId::generate(),
      project,
      WorkSourceInputDraft {
        predicate: None,
        connection: None,
        target: config.reference().clone(),
        repository: repo.clone(),
        source: binding.clone(),
        reporter: None,
        enabled: true,
      },
      WorkSourceInputContext {
        target: &config,
        repository: &repo,
        source: Some(&capability),
        reporter: None,
      },
    )
    .unwrap();
    store
      .publish_work_sources(PublishWorkSources {
        project_id: project,
        inputs: vec![WorkSourceInputChange {
          expected_version: None,
          input: input.clone(),
        }],
        group: None,
      })
      .await
      .unwrap();
    let resource = octacity_work_source_protocol::ResourceAccess {
      resource_id: "native-container".into(),
      resource_locator: "private-locator".into(),
      credential: octacity_work_source_protocol::CredentialHandle {
        class: octacity_work_source_protocol::CredentialClass::WorkSourceRead,
        handle: "private-source-grant".into(),
      },
    };
    let profile = FactorySourceProfile {
      managed_access: None,
      project_id: project,
      binding: binding.clone(),
      plugin: ImmutableReference::new(key(provider), key("v1"), plugin_digest),
      provider_configuration: FactoryDigest::from_bytes([0xee; 32]),
      resource,
      selection: octacity_work_source_protocol::ConfigurationReference {
        identity: "selected-input".into(),
        sha256: "b".repeat(64),
      },
      network_hosts: vec!["tracker.invalid".into()],
      context: ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap()),
      preparation: SourcePreparationSettings {
        mapping: WorkSourceMapping {
          source: binding,
          acceptance: SourceAcceptanceMapping::Body,
          require_body: true,
          max_title_bytes: 4096,
          max_body_bytes: 262144,
          max_artifact_bytes: 1048576,
          retention_millis: 86400000,
        },
        source: RepositorySourceSelection::ExactRevision(ImmutableRevision::new("0123456789abcdef").unwrap()),
        classification: WorkClassification::new(
          WorkPriority::new(10).unwrap(),
          RiskClass::Medium,
          FactoryMetadata::default(),
        ),
      },
      polling: SourcePollingPolicy {
        page_items: 2,
        max_pages: 1,
        max_items: 2,
        max_bytes: 65536,
        max_page_bytes: 65536,
        overlap_millis: 20,
        reconciliation_interval_millis: 1000,
        retention_millis: 86400000,
      },
    };
    Self {
      root,
      store,
      bytes: Arc::new(Bytes::default()),
      clock: Arc::new(Clock(AtomicI64::new(100))),
      input,
      profile,
    }
  }
}
impl<S: WorkSourceArtifactStore + FactoryDiscoveryStore + 'static> Fixture<S> {
  fn worker(&self) -> FactorySourceWorker<S, Bytes> {
    self.worker_with_settings(settings())
  }
  fn worker_with_settings(&self, policy: FactorySourceWorkerSettings) -> FactorySourceWorker<S, Bytes> {
    FactorySourceWorker::new(
      FactorySourceWorkerDependencies {
        store: self.store.clone(),
        bytes: self.bytes.clone(),
        revisions: Arc::new(ExactRevisionResolver),
        authorization: Arc::new(TrustedNetworkManagementPolicy),
        registry: Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(self.root.path()).unwrap()),
        clock: self.clock.clone(),
      },
      FactorySourceInstallation {
        projects: vec![self.input.project_id()],
        profiles: vec![self.profile.clone()],
        settings: policy,
      },
    )
    .unwrap()
  }
  async fn runs(&self) -> Vec<FactoryRunSummary> {
    self
      .store
      .list_factory_runs(
        ListFactoryRuns::new(
          Some(self.input.project_id()),
          FactoryRunFilter::default(),
          None,
          64,
          FactoryRunListVisibility::all(),
        )
        .unwrap(),
      )
      .await
      .unwrap()
      .items
  }
}

#[path = "work_reporter_journey_tests.rs"]
mod reporting;

#[tokio::test]
async fn admission_contract_fixtures_tolerate_slow_valid_provider_responses() {
  let fixture = Fixture::enabled("jira").await;
  let path = fixture.root.path().join("jira/mode.json");
  let mut mode: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
  mode["delay"] = serde_json::json!(0.75);
  std::fs::write(path, serde_json::to_vec(&mode).unwrap()).unwrap();
  let outcome = fixture.worker().run_once(CancellationToken::new()).await.unwrap();
  let schedule = fixture
    .store
    .source_scan_schedule(
      fixture.input.project_id(),
      WorkSourceScanTarget::Input {
        id: fixture.input.id(),
        version: fixture.input.version(),
      },
    )
    .await
    .unwrap()
    .unwrap();
  assert_eq!(
    (outcome.completed, outcome.deferred),
    (1, 0),
    "valid provider was deferred: {:?}",
    schedule.failure
  );
  assert_eq!(fixture.runs().await.len(), 1);
}

#[tokio::test]
async fn enabled_sources_use_verified_processes_and_ordinary_admission_for_every_provider() {
  for provider in ["github", "gitlab", "jira", "custom-tracker"] {
    let fixture = Fixture::enabled(provider).await;
    let result = fixture.worker().run_once(CancellationToken::new()).await.unwrap();
    assert_eq!((result.claimed, result.completed, result.deferred), (1, 1, 0));
    let runs = fixture.runs().await;
    assert_eq!(runs.len(), 1);
    assert_eq!(fixture.bytes.0.lock().unwrap().len(), 2);
    let captured = std::fs::read_to_string(fixture.root.path().join(provider).join("requests.jsonl")).unwrap();
    let request: serde_json::Value = serde_json::from_str(captured.lines().next().unwrap()).unwrap();
    assert_eq!(
      request["message"]["command"]["payload"]["resource"]["credential"]["class"],
      "work_source_read"
    );
    assert!(
      request["message"]["command"]["payload"].get("target").is_none(),
      "a provider receives no Factory target authority"
    );
  }
}

#[tokio::test]
async fn provider_retry_hints_survive_restart_exceed_local_backoff_and_never_become_empty_success() {
  let fixture = Fixture::enabled("jira").await;
  let path = fixture.root.path().join("jira").join("mode.json");
  let original = std::fs::read(&path).unwrap();
  std::fs::write(&path, br#"{"failure":{"class":"transient","retry_after_millis":2000}}"#).unwrap();
  let result = fixture.worker().run_once(CancellationToken::new()).await.unwrap();
  assert_eq!((result.claimed, result.completed, result.deferred), (1, 0, 1));
  let target = WorkSourceScanTarget::Input {
    id: fixture.input.id(),
    version: fixture.input.version(),
  };
  let schedule = fixture
    .store
    .source_scan_schedule(fixture.input.project_id(), target)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(schedule.retry_at, time(2100));
  assert_eq!(schedule.failure, Some(WorkSourceFailure::RateLimited));
  assert_eq!(schedule.attempts, 1);
  assert!(fixture.runs().await.is_empty());
  assert!(fixture.bytes.0.lock().unwrap().is_empty());
  std::fs::write(path, original).unwrap();
  fixture.clock.0.store(2099, Ordering::SeqCst);
  assert_eq!(
    fixture
      .worker()
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .claimed,
    0
  );
  fixture.clock.0.store(2100, Ordering::SeqCst);
  assert_eq!(
    fixture
      .worker()
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  assert_eq!(fixture.runs().await.len(), 1);
  let recovered = fixture
    .store
    .source_scan_schedule(fixture.input.project_id(), target)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(recovered.attempts, 0);
  assert!(recovered.failure.is_none());
}

#[tokio::test]
async fn source_attempt_budget_is_independent_of_the_supervisor_circuit_threshold() {
  for threshold in [1, 6] {
    let fixture = Fixture::enabled("jira").await;
    std::fs::write(
      fixture.root.path().join("jira/mode.json"),
      br#"{"failure":{"class":"transient","retry_after_millis":null}}"#,
    )
    .unwrap();
    let mut policy = settings();
    policy.max_attempts = 4;
    policy.worker = toml::from_str(&format!(
      "poll_interval_milliseconds=10\nclaim_lifetime_milliseconds=60000\nbatch_size=4\nconcurrency=2\n[retry]\nmax_backoff_milliseconds=40\nconsecutive_failure_limit={threshold}\ncircuit_open_milliseconds=200"
    )).unwrap();
    let target = WorkSourceScanTarget::Input {
      id: fixture.input.id(),
      version: fixture.input.version(),
    };
    for (at, attempts) in [(100, 1), (110, 2), (130, 3), (170, 4), (1000, 4)] {
      fixture.clock.0.store(at, Ordering::SeqCst);
      let outcome = fixture
        .worker_with_settings(policy)
        .run_once(CancellationToken::new())
        .await
        .unwrap();
      assert_eq!(outcome.claimed, u16::from(at != 1000));
      let schedule = fixture
        .store
        .source_scan_schedule(fixture.input.project_id(), target)
        .await
        .unwrap()
        .unwrap();
      assert_eq!(schedule.attempts, attempts);
      assert_eq!(
        schedule.exhausted,
        attempts == 4,
        "only the explicit Source attempt budget stops this input"
      );
    }
    assert!(fixture.runs().await.is_empty());
    assert!(fixture.bytes.0.lock().unwrap().is_empty());
  }
}

#[tokio::test]
async fn source_installations_reject_unbounded_workers_and_ambiguous_or_foreign_profiles_before_activity() {
  for defect in [
    "attempts",
    "timeout",
    "claim-window",
    "duplicate",
    "foreign-project",
    "mapping",
  ] {
    let fixture = Fixture::enabled("jira").await;
    let mut installation = FactorySourceInstallation {
      projects: vec![fixture.input.project_id()],
      profiles: vec![fixture.profile.clone()],
      settings: settings(),
    };
    match defect {
      "attempts" => installation.settings.max_attempts = 0,
      "timeout" => installation.settings.operation_timeout = Duration::ZERO,
      "claim-window" => installation.settings.operation_timeout = installation.settings.worker.claim_lifetime(),
      "duplicate" => installation.profiles.push(fixture.profile.clone()),
      "foreign-project" => installation.profiles[0].project_id = ProjectId::generate(),
      "mapping" => installation.profiles[0].preparation.mapping.source.configuration = exact("foreign-settings", 90),
      _ => unreachable!(),
    }
    assert!(
      FactorySourceWorker::new(
        FactorySourceWorkerDependencies {
          store: fixture.store.clone(),
          bytes: fixture.bytes.clone(),
          revisions: Arc::new(ExactRevisionResolver),
          authorization: Arc::new(TrustedNetworkManagementPolicy),
          registry: Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
          clock: fixture.clock.clone()
        },
        installation
      )
      .is_err(),
      "{defect}"
    );
    assert!(!fixture.root.path().join("jira/requests.jsonl").exists());
  }
}

#[tokio::test]
async fn exclusive_group_ambiguity_is_durable_and_later_one_match_uses_its_exact_target() {
  let fixture = Fixture::enabled("jira").await;
  let config = configuration(fixture.input.project_id(), FactoryConfigurationId::generate());
  fixture
    .store
    .seed_factory_configuration_version(PublishedFactoryConfiguration {
      configuration: config.clone(),
      published_at: time(1),
    })
    .unwrap();
  let mut profile = fixture.profile.clone();
  profile.binding.configuration = exact("other-input", 45);
  profile.preparation.mapping.source = profile.binding.clone();
  profile.selection.identity = "other-selector".into();
  profile.selection.sha256 = "d".repeat(64);
  profile.polling.page_items = 1;
  let capability = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: profile.binding.clone(),
  };
  let mut draft = fixture.input.definition().clone();
  draft.target = config.reference().clone();
  draft.source = profile.binding.clone();
  let second = WorkSourceInput::publish(
    WorkSourceInputId::generate(),
    fixture.input.project_id(),
    draft,
    WorkSourceInputContext {
      target: &config,
      repository: &fixture.input.definition().repository,
      source: Some(&capability),
      reporter: None,
    },
  )
  .unwrap();
  let group = WorkSourceGroup::publish(
    WorkSourceGroupId::generate(),
    fixture.input.project_id(),
    vec![fixture.input.clone(), second.clone()],
  )
  .unwrap();
  fixture
    .store
    .publish_work_sources(PublishWorkSources {
      project_id: fixture.input.project_id(),
      inputs: vec![WorkSourceInputChange {
        expected_version: None,
        input: second.clone(),
      }],
      group: Some(WorkSourceGroupChange {
        expected_version: None,
        group: group.clone(),
      }),
    })
    .await
    .unwrap();
  let path = fixture.root.path().join("jira/mode.json");
  let mut mode: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
  mode["candidates"][0]["matches"] = serde_json::json!([0, 1]);
  std::fs::write(&path, serde_json::to_vec(&mode).unwrap()).unwrap();
  let worker = FactorySourceWorker::new(
    FactorySourceWorkerDependencies {
      store: fixture.store.clone(),
      bytes: fixture.bytes.clone(),
      revisions: Arc::new(ExactRevisionResolver),
      authorization: Arc::new(TrustedNetworkManagementPolicy),
      registry: Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
      clock: fixture.clock.clone(),
    },
    FactorySourceInstallation {
      projects: vec![fixture.input.project_id()],
      profiles: vec![fixture.profile.clone(), profile],
      settings: settings(),
    },
  )
  .unwrap();
  assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().completed, 1);
  assert!(fixture.runs().await.is_empty());
  assert!(fixture.bytes.0.lock().unwrap().is_empty());
  let target = WorkSourceScanTarget::Group {
    id: group.id(),
    version: group.version(),
  };
  let retained = fixture
    .store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: fixture.input.project_id(),
      target,
      owner: key("inspect"),
      now: time(200),
      until: time(300),
    })
    .await
    .unwrap()
    .unwrap();
  assert_eq!(retained.last_selection.unwrap().matching_inputs.len(), 2);
  fixture
    .store
    .finish_source_scan(
      retained.claim,
      time(200),
      WorkSourceSchedule {
        retry_at: time(210),
        attempts: 0,
        failure: None,
        exhausted: false,
      },
    )
    .await
    .unwrap();
  mode["candidates"][0]["matches"] = serde_json::json!([1]);
  std::fs::write(path, serde_json::to_vec(&mode).unwrap()).unwrap();
  fixture.clock.0.store(210, Ordering::SeqCst);
  assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().completed, 1);
  let runs = fixture.runs().await;
  assert_eq!(runs.len(), 1);
  assert_eq!(runs[0].configuration_id, config.reference().id());
  let calls = std::fs::read_to_string(fixture.root.path().join("jira/requests.jsonl")).unwrap();
  for call in calls.lines() {
    let request: serde_json::Value = serde_json::from_str(call).unwrap();
    if request["message"]["command"]["operation"] == "scan" {
      assert_eq!(request["message"]["command"]["payload"]["max_items"], 1);
    }
  }
}

#[tokio::test]
async fn source_batches_are_bounded_concurrent_and_advance_past_the_first_page() {
  let fixture = Fixture::enabled("jira").await;
  let mut inputs = vec![fixture.input.clone()];
  let config = fixture
    .store
    .factory_configuration_version(
      fixture.input.definition().target.id(),
      fixture.input.definition().target.version(),
    )
    .await
    .unwrap();
  let capability = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: fixture.profile.binding.clone(),
  };
  for _ in 0..2 {
    let input = WorkSourceInput::publish(
      WorkSourceInputId::generate(),
      fixture.input.project_id(),
      fixture.input.definition().clone(),
      WorkSourceInputContext {
        target: &config.configuration,
        repository: &fixture.input.definition().repository,
        source: Some(&capability),
        reporter: None,
      },
    )
    .unwrap();
    fixture
      .store
      .publish_work_sources(PublishWorkSources {
        project_id: fixture.input.project_id(),
        inputs: vec![WorkSourceInputChange {
          expected_version: None,
          input: input.clone(),
        }],
        group: None,
      })
      .await
      .unwrap();
    inputs.push(input);
  }
  std::fs::write(
    fixture.root.path().join("jira/mode.json"),
    br#"{"pause_scan":true,"candidates":[]}"#,
  )
  .unwrap();
  let mut policy = settings();
  policy.worker=toml::from_str("poll_interval_milliseconds=10\nclaim_lifetime_milliseconds=60000\nbatch_size=2\nconcurrency=2\n[retry]\nmax_backoff_milliseconds=40\nconsecutive_failure_limit=3\ncircuit_open_milliseconds=200").unwrap();
  let worker = FactorySourceWorker::new(
    FactorySourceWorkerDependencies {
      store: fixture.store.clone(),
      bytes: fixture.bytes.clone(),
      revisions: Arc::new(ExactRevisionResolver),
      authorization: Arc::new(TrustedNetworkManagementPolicy),
      registry: Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
      clock: fixture.clock.clone(),
    },
    FactorySourceInstallation {
      projects: vec![fixture.input.project_id()],
      profiles: vec![fixture.profile.clone()],
      settings: policy,
    },
  )
  .unwrap();
  let worker = Arc::new(worker);
  let first = worker.clone();
  let pass = tokio::spawn(async move { first.run_once(CancellationToken::new()).await });
  // Both provider processes must start before either can return, independent of CPU scheduling.
  tokio::time::timeout(TEST_WAIT, async {
    loop {
      let events = std::fs::read_to_string(fixture.root.path().join("jira/events.jsonl")).unwrap_or_default();
      let started = events
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["event"] == "start")
        .count();
      if started == 2 {
        break;
      }
      tokio::time::sleep(Duration::from_millis(5)).await;
    }
  })
  .await
  .expect("the configured concurrency must reach both provider processes");
  fixture.release_provider();
  assert_eq!(
    tokio::time::timeout(TEST_WAIT, pass)
      .await
      .unwrap()
      .unwrap()
      .unwrap()
      .completed,
    2
  );
  let mut active = 0;
  let mut peak = 0;
  for line in std::fs::read_to_string(fixture.root.path().join("jira/events.jsonl"))
    .unwrap()
    .lines()
  {
    let event: serde_json::Value = serde_json::from_str(line).unwrap();
    if event["event"] == "start" {
      active += 1;
      peak = peak.max(active);
    } else {
      active -= 1;
    }
  }
  assert_eq!(
    peak, 2,
    "two independent inputs should run concurrently within the configured ceiling"
  );
  assert_eq!(active, 0);
  fixture.clock.0.store(110, Ordering::SeqCst);
  assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().completed, 1);
  for input in inputs {
    assert!(
      fixture
        .store
        .source_scan_schedule(
          input.project_id(),
          WorkSourceScanTarget::Input {
            id: input.id(),
            version: input.version()
          }
        )
        .await
        .unwrap()
        .is_some()
    );
  }
}

#[tokio::test]
async fn source_shutdown_drains_the_verified_process_without_spending_a_retry_or_admitting_work() {
  let fixture = Fixture::enabled("jira").await;
  let mode = fixture.root.path().join("jira/mode.json");
  let original = std::fs::read(&mode).unwrap();
  std::fs::write(&mode, br#"{"hold":true}"#).unwrap();
  let cancellation = CancellationToken::new();
  let cancel = cancellation.clone();
  let worker = fixture.worker();
  let pass = tokio::spawn(async move { worker.run_once(cancel).await });
  fixture.wait_for_provider_file("ready").await;
  cancellation.cancel();
  let result = tokio::time::timeout(TEST_WAIT, pass).await.unwrap().unwrap().unwrap();
  assert_eq!(result.deferred, 1);
  assert!(fixture.runs().await.is_empty());
  assert!(fixture.bytes.0.lock().unwrap().is_empty());
  let target = WorkSourceScanTarget::Input {
    id: fixture.input.id(),
    version: fixture.input.version(),
  };
  let schedule = fixture
    .store
    .source_scan_schedule(fixture.input.project_id(), target)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(schedule.failure, Some(WorkSourceFailure::Cancelled));
  assert_eq!(
    schedule.attempts, 0,
    "operator shutdown does not consume the finite provider retry budget"
  );
  let cancelled = std::fs::read_to_string(fixture.root.path().join("jira/cancelled")).unwrap();
  assert!(cancelled.contains("cancel"));
  std::fs::write(mode, original).unwrap();
  fixture.clock.0.store(110, Ordering::SeqCst);
  assert_eq!(
    fixture
      .worker()
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  assert_eq!(fixture.runs().await.len(), 1);
  let calls = std::fs::read_to_string(fixture.root.path().join("jira/requests.jsonl")).unwrap();
  let ids = calls
    .lines()
    .map(|line| {
      serde_json::from_str::<serde_json::Value>(line).unwrap()["message"]["command"]["payload"]["scan_id"].clone()
    })
    .collect::<Vec<_>>();
  assert_eq!(ids[0], ids[1], "restart resumes the original logical scan");
}

#[tokio::test]
async fn a_source_pass_deadline_bounds_revision_resolution_and_leaves_original_intent_recoverable() {
  struct Stalled(Arc<tokio::sync::Notify>);
  #[async_trait::async_trait]
  impl RevisionResolver for Stalled {
    async fn resolve(&self, _: RevisionResolutionRequest) -> Result<ImmutableRevision, RevisionResolutionError> {
      self.0.notify_one();
      std::future::pending().await
    }
  }
  let fixture = Fixture::enabled("jira").await;
  let policy = settings();
  let resolving = Arc::new(tokio::sync::Notify::new());
  let worker = FactorySourceWorker::new(
    FactorySourceWorkerDependencies {
      store: fixture.store.clone(),
      bytes: fixture.bytes.clone(),
      revisions: Arc::new(Stalled(resolving.clone())),
      authorization: Arc::new(TrustedNetworkManagementPolicy),
      registry: Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
      clock: fixture.clock.clone(),
    },
    FactorySourceInstallation {
      projects: vec![fixture.input.project_id()],
      profiles: vec![fixture.profile.clone()],
      settings: policy,
    },
  )
  .unwrap();
  let pass = tokio::spawn(async move { worker.run_once(CancellationToken::new()).await });
  tokio::time::timeout(TEST_WAIT, resolving.notified())
    .await
    .expect("the original intent must reach VCS before exercising the pass deadline");
  // Start virtual time only after real provider processes have completed preparation.
  tokio::time::pause();
  tokio::time::advance(policy.worker.claim_lifetime()).await;
  let result = tokio::time::timeout(TEST_WAIT, pass)
    .await
    .expect("the complete source pass must have a finite deadline")
    .unwrap()
    .unwrap();
  tokio::time::resume();
  assert_eq!(result.deferred, 1);
  assert!(fixture.runs().await.is_empty());
  let target = WorkSourceScanTarget::Input {
    id: fixture.input.id(),
    version: fixture.input.version(),
  };
  let schedule = fixture
    .store
    .source_scan_schedule(fixture.input.project_id(), target)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(schedule.failure, Some(WorkSourceFailure::Timeout));
  let pending = fixture
    .store
    .pending_source_preparations(fixture.input.project_id(), target, None, 2)
    .await
    .unwrap();
  assert_eq!(pending.len(), 1);
  assert!(pending[0].intent.is_some());
  let config = fixture
    .store
    .factory_configuration_version(
      fixture.input.definition().target.id(),
      fixture.input.definition().target.version(),
    )
    .await
    .unwrap();
  let capability = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: fixture.profile.binding.clone(),
  };
  let changed = fixture
    .input
    .replace(
      WorkSourceInputVersion::new(2).unwrap(),
      fixture.input.definition().clone(),
      WorkSourceInputContext {
        target: &config.configuration,
        repository: &fixture.input.definition().repository,
        source: Some(&capability),
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
        input: changed,
      }],
      group: None,
    })
    .await
    .unwrap();
  std::fs::write(
    fixture.root.path().join("jira/mode.json"),
    br#"{"failure":{"class":"unsupported"}}"#,
  )
  .unwrap();
  let manifest = fixture.root.path().join("jira/adapter.toml");
  let installed = std::fs::read_to_string(&manifest).unwrap();
  std::fs::write(&manifest, installed.replace("min=1\nmax=1", "min=2\nmax=2")).unwrap();
  fixture.clock.0.store(110, Ordering::SeqCst);
  fixture.worker().run_once(CancellationToken::new()).await.unwrap();
  assert!(
    fixture.runs().await.is_empty(),
    "retained data cannot bypass current Source version negotiation"
  );
  std::fs::write(&manifest, installed).unwrap();
  fixture.clock.0.store(120, Ordering::SeqCst);
  fixture.worker().run_once(CancellationToken::new()).await.unwrap();
  assert_eq!(
    fixture.runs().await.len(),
    1,
    "retained original intent settles before fetching an unavailable provider after definition replacement"
  );
  assert_eq!(
    fixture.bytes.0.lock().unwrap().len(),
    2,
    "frozen prepared inputs are not republished"
  );
}

#[tokio::test]
async fn installed_source_polling_is_supervised_and_stops_with_the_runtime() {
  let fixture = Fixture::enabled("jira").await;
  let cancellation = CancellationToken::new();
  let task = super::workers::spawn_factory_source_worker(Arc::new(fixture.worker()), cancellation.clone());
  tokio::time::timeout(TEST_WAIT, async {
    while fixture.runs().await.is_empty() {
      tokio::time::sleep(Duration::from_millis(5)).await;
    }
  })
  .await
  .expect("the runtime must actually poll the explicitly installed Source");
  cancellation.cancel();
  tokio::time::timeout(TEST_WAIT, task).await.unwrap().unwrap();
  assert_eq!(fixture.runs().await.len(), 1);
}

impl Fixture {
  async fn replace_enabled(&self, enabled: bool) -> WorkSourceInput {
    let config = self
      .store
      .factory_configuration_version(
        self.input.definition().target.id(),
        self.input.definition().target.version(),
      )
      .await
      .unwrap();
    let capability = WorkSourceCapability {
      project_id: self.input.project_id(),
      role: WorkSourceCapabilityRole::Source,
      binding: self.profile.binding.clone(),
    };
    let mut draft = self.input.definition().clone();
    draft.enabled = enabled;
    let input = self
      .input
      .replace(
        WorkSourceInputVersion::new(2).unwrap(),
        draft,
        WorkSourceInputContext {
          target: &config.configuration,
          repository: &self.input.definition().repository,
          source: Some(&capability),
          reporter: None,
        },
      )
      .unwrap();
    self
      .store
      .publish_work_sources(PublishWorkSources {
        project_id: input.project_id(),
        inputs: vec![WorkSourceInputChange {
          expected_version: Some(self.input.version()),
          input: input.clone(),
        }],
        group: None,
      })
      .await
      .unwrap();
    input
  }
}
#[tokio::test]
async fn unavailable_inputs_have_durable_finite_diagnostics_and_disabled_inputs_do_no_provider_work() {
  let fixture = Fixture::enabled("jira").await;
  let root = tempfile::tempdir().unwrap();
  for (at, attempts) in [(100, 1), (110, 2), (130, 3), (1000, 3)] {
    fixture.clock.0.store(at, Ordering::SeqCst);
    let worker = FactorySourceWorker::new(
      FactorySourceWorkerDependencies {
        store: fixture.store.clone(),
        bytes: fixture.bytes.clone(),
        revisions: Arc::new(ExactRevisionResolver),
        authorization: Arc::new(TrustedNetworkManagementPolicy),
        registry: Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(root.path()).unwrap()),
        clock: fixture.clock.clone(),
      },
      FactorySourceInstallation {
        projects: vec![fixture.input.project_id()],
        profiles: vec![fixture.profile.clone()],
        settings: settings(),
      },
    )
    .unwrap();
    let result = worker.run_once(CancellationToken::new()).await.unwrap();
    assert_eq!(result.deferred, if at == 1000 { 0 } else { 1 });
    let schedule = fixture
      .store
      .source_scan_schedule(
        fixture.input.project_id(),
        WorkSourceScanTarget::Input {
          id: fixture.input.id(),
          version: fixture.input.version(),
        },
      )
      .await
      .unwrap()
      .unwrap();
    assert_eq!(schedule.failure, Some(WorkSourceFailure::Unavailable));
    assert_eq!(schedule.attempts, attempts);
    assert_eq!(schedule.exhausted, attempts == 3);
  }
  let replacement = fixture.replace_enabled(true).await;
  assert_eq!(
    fixture
      .worker()
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  assert!(
    fixture
      .store
      .source_scan_schedule(
        replacement.project_id(),
        WorkSourceScanTarget::Input {
          id: replacement.id(),
          version: replacement.version()
        }
      )
      .await
      .unwrap()
      .unwrap()
      .failure
      .is_none()
  );
  let disabled = Fixture::enabled("gitlab").await;
  disabled.replace_enabled(false).await;
  assert_eq!(
    disabled
      .worker()
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .claimed,
    0
  );
  assert!(!disabled.root.path().join("gitlab/requests.jsonl").exists());
  assert!(disabled.runs().await.is_empty());
}

#[tokio::test]
async fn disabling_an_input_in_flight_revokes_preparation_and_admission() {
  let fixture = Fixture::enabled("jira").await;
  let path = fixture.root.path().join("jira/mode.json");
  let mut mode: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
  mode["pause_scan"] = serde_json::json!(true);
  std::fs::write(path, serde_json::to_vec(&mode).unwrap()).unwrap();
  let worker = fixture.worker();
  let mut pass = tokio::spawn(async move { worker.run_once(CancellationToken::new()).await });
  tokio::select! {
    () = fixture.wait_for_provider_file("ready") => {},
    result = &mut pass => panic!("the provider must wait for revocation before replying: {result:?}"),
  }
  fixture.replace_enabled(false).await;
  fixture.release_provider();
  assert_eq!(pass.await.unwrap().unwrap().deferred, 1);
  assert!(fixture.runs().await.is_empty());
  assert!(fixture.bytes.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_stalled_provider_is_timed_out_and_is_not_reported_as_an_empty_page() {
  let fixture = Fixture::enabled("jira").await;
  std::fs::write(fixture.root.path().join("jira/mode.json"), br#"{"hold":true}"#).unwrap();
  let worker = fixture.worker();
  let pass = tokio::spawn(async move { worker.run_once(CancellationToken::new()).await });
  fixture.wait_for_provider_file("ready").await;
  tokio::time::pause();
  tokio::time::advance(settings().operation_timeout).await;
  // Let real child exit/reaping run before applying a watchdog to OS process cleanup.
  tokio::time::resume();
  let outcome = tokio::time::timeout(TEST_WAIT, pass).await.unwrap().unwrap().unwrap();
  assert_eq!(outcome.deferred, 1);
  let schedule = fixture
    .store
    .source_scan_schedule(
      fixture.input.project_id(),
      WorkSourceScanTarget::Input {
        id: fixture.input.id(),
        version: fixture.input.version(),
      },
    )
    .await
    .unwrap()
    .unwrap();
  assert_eq!(schedule.failure, Some(WorkSourceFailure::Timeout));
  assert_eq!(schedule.attempts, 1);
  assert!(fixture.runs().await.is_empty());
}

#[tokio::test]
async fn runtime_retention_clears_orphaned_source_snapshots_after_the_input_is_disabled() {
  let fixture = Fixture::enabled("jira").await;
  let target = WorkSourceScanTarget::Input {
    id: fixture.input.id(),
    version: fixture.input.version(),
  };
  let scan = fixture
    .store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: fixture.input.project_id(),
      target,
      owner: key("orphan"),
      now: time(100),
      until: time(200),
    })
    .await
    .unwrap()
    .unwrap();
  let retained = fixture
    .store
    .reserve_source_preparation(ReserveWorkSourcePreparation {
      claim: scan.claim,
      input_id: fixture.input.id(),
      item: WorkSourceItem {
        resource: ExternalWorkIdentity::new("native-container").unwrap(),
        item: ExternalWorkIdentity::new("orphaned").unwrap(),
      },
      snapshot: b"private orphaned snapshot".to_vec(),
      now: time(100),
    })
    .await
    .unwrap();
  fixture.replace_enabled(false).await;
  let worker = FactorySourceInputRetentionWorker::new(
    fixture.store.clone(),
    super::factory_tests::resources().objects,
    Duration::from_secs(1),
  )
  .unwrap();
  let cancellation = CancellationToken::new();
  let task = super::workers::spawn_source_input_retention_worker(
    worker,
    Duration::from_millis(10),
    Duration::from_secs(1),
    4,
    cancellation.clone(),
  );
  tokio::time::timeout(Duration::from_secs(2), async {
    while !fixture
      .store
      .source_preparation(fixture.input.project_id(), retained.id)
      .await
      .unwrap()
      .snapshot
      .is_empty()
    {
      tokio::time::sleep(Duration::from_millis(5)).await;
    }
  })
  .await
  .expect("private source retention must run even without active Source polling");
  cancellation.cancel();
  task.await.unwrap();
  assert!(!fixture.root.path().join("jira/requests.jsonl").exists());
}

#[tokio::test]
async fn retained_preparations_are_drained_in_finite_pages_before_fresh_discovery() {
  let mut fixture = Fixture::enabled("jira").await;
  fixture.profile.polling.max_items = 100;
  let target = WorkSourceScanTarget::Input {
    id: fixture.input.id(),
    version: fixture.input.version(),
  };
  let scan = fixture
    .store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: fixture.input.project_id(),
      target,
      owner: key("interrupted"),
      now: time(10),
      until: time(200),
    })
    .await
    .unwrap()
    .unwrap();
  for index in 0..65 {
    let snapshot = WorkSourceSnapshot {
      selection_matches: vec![],
      item: WorkSourceItem {
        resource: ExternalWorkIdentity::new("native-container").unwrap(),
        item: ExternalWorkIdentity::new(format!("pending-{index}")).unwrap(),
      },
      display_id: format!("TASK-{index}"),
      update_identity: "original".into(),
      title: "Missing required acceptance".into(),
      body: None,
      fields: Default::default(),
      labels: vec![],
      state: "open".into(),
    };
    fixture
      .store
      .reserve_source_preparation(ReserveWorkSourcePreparation {
        claim: scan.claim.clone(),
        input_id: fixture.input.id(),
        item: snapshot.item.clone(),
        snapshot: serde_json::to_vec(&snapshot).unwrap(),
        now: time(11),
      })
      .await
      .unwrap();
  }
  fixture
    .store
    .finish_source_scan(
      scan.claim,
      time(20),
      WorkSourceSchedule {
        retry_at: time(100),
        attempts: 0,
        failure: None,
        exhausted: false,
      },
    )
    .await
    .unwrap();
  let mut policy = settings();
  policy.worker =
    toml::from_str("poll_interval_milliseconds=10\nclaim_lifetime_milliseconds=20000\nbatch_size=4\nconcurrency=2")
      .unwrap();
  assert_eq!(
    fixture
      .worker_with_settings(policy)
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1,
    "a full retained page is progress, not a checkpoint conflict"
  );
  assert_eq!(
    fixture
      .store
      .pending_source_preparations(fixture.input.project_id(), target, None, 64)
      .await
      .unwrap()
      .len(),
    1
  );
  let calls = std::fs::read_to_string(fixture.root.path().join("jira/requests.jsonl")).unwrap();
  assert!(
    calls.lines().all(
      |line| serde_json::from_str::<serde_json::Value>(line).unwrap()["message"]["command"]["operation"] == "authorize"
    ),
    "do not discover while the retained queue may contain another page"
  );
  std::fs::write(fixture.root.path().join("jira/mode.json"), br#"{"candidates":[]}"#).unwrap();
  fixture.clock.0.store(110, Ordering::SeqCst);
  assert_eq!(
    fixture
      .worker()
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  assert!(
    fixture
      .store
      .pending_source_preparations(fixture.input.project_id(), target, None, 64)
      .await
      .unwrap()
      .is_empty()
  );
  assert!(fixture.runs().await.is_empty());
}

#[tokio::test]
async fn retained_replay_rechecks_the_current_read_grant_in_a_verified_process() {
  let fixture = Fixture::enabled("jira").await;
  assert_eq!(
    fixture
      .worker()
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  let path = fixture.root.path().join("jira/mode.json");
  let mut mode: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
  mode["deny_access"] = serde_json::json!(true);
  std::fs::write(path, serde_json::to_vec(&mode).unwrap()).unwrap();
  fixture.clock.0.store(110, Ordering::SeqCst);
  assert_eq!(
    fixture
      .worker()
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .deferred,
    1,
    "a retained association cannot bypass a revoked current read grant"
  );
  assert_eq!(fixture.runs().await.len(), 1);
  assert_eq!(fixture.bytes.0.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn pending_unmapped_snapshots_require_current_read_authority_before_artifact_publication() {
  let fixture = Fixture::enabled("jira").await;
  let target = WorkSourceScanTarget::Input {
    id: fixture.input.id(),
    version: fixture.input.version(),
  };
  let scan = fixture
    .store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: fixture.input.project_id(),
      target,
      owner: key("interrupted"),
      now: time(10),
      until: time(200),
    })
    .await
    .unwrap()
    .unwrap();
  let snapshot = WorkSourceSnapshot {
    selection_matches: vec![],
    item: WorkSourceItem {
      resource: ExternalWorkIdentity::new("native-container").unwrap(),
      item: ExternalWorkIdentity::new("unmapped").unwrap(),
    },
    display_id: "TASK-1".into(),
    update_identity: "original".into(),
    title: "Retained task".into(),
    body: Some("Explicit acceptance".into()),
    fields: Default::default(),
    labels: vec![],
    state: "open".into(),
  };
  fixture
    .store
    .reserve_source_preparation(ReserveWorkSourcePreparation {
      claim: scan.claim.clone(),
      input_id: fixture.input.id(),
      item: snapshot.item.clone(),
      snapshot: serde_json::to_vec(&snapshot).unwrap(),
      now: time(11),
    })
    .await
    .unwrap();
  fixture
    .store
    .finish_source_scan(
      scan.claim,
      time(20),
      WorkSourceSchedule {
        retry_at: time(100),
        attempts: 0,
        failure: None,
        exhausted: false,
      },
    )
    .await
    .unwrap();
  std::fs::write(
    fixture.root.path().join("jira/mode.json"),
    br#"{"deny_access":true,"candidates":[]}"#,
  )
  .unwrap();
  assert_eq!(
    fixture
      .worker()
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .deferred,
    1
  );
  assert!(
    fixture.bytes.0.lock().unwrap().is_empty(),
    "a retained snapshot cannot publish artifacts under revoked read authority"
  );
  assert!(fixture.runs().await.is_empty());
}

#[tokio::test]
async fn installing_an_exclusive_group_revokes_an_in_flight_standalone_scan() {
  let fixture = Fixture::enabled("jira").await;
  let path = fixture.root.path().join("jira/mode.json");
  let mut mode: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
  mode["pause_scan"] = serde_json::json!(true);
  std::fs::write(path, serde_json::to_vec(&mode).unwrap()).unwrap();
  let worker = fixture.worker();
  let mut pass = tokio::spawn(async move { worker.run_once(CancellationToken::new()).await });
  tokio::select! {
    () = fixture.wait_for_provider_file("ready") => {},
    result = &mut pass => panic!("the provider must wait for group publication before replying: {result:?}"),
  }
  let group = WorkSourceGroup::publish(
    WorkSourceGroupId::generate(),
    fixture.input.project_id(),
    vec![fixture.input.clone()],
  )
  .unwrap();
  fixture
    .store
    .publish_work_sources(PublishWorkSources {
      project_id: fixture.input.project_id(),
      inputs: vec![],
      group: Some(WorkSourceGroupChange {
        expected_version: None,
        group,
      }),
    })
    .await
    .unwrap();
  fixture.release_provider();
  assert_eq!(
    pass.await.unwrap().unwrap().deferred,
    1,
    "a newly exclusive member must not use an old standalone claim"
  );
  assert!(fixture.runs().await.is_empty());
  assert!(fixture.bytes.0.lock().unwrap().is_empty());
  fixture.clock.0.store(120, Ordering::SeqCst);
  assert_eq!(
    fixture
      .worker()
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  assert_eq!(fixture.runs().await.len(), 1);
}

impl Fixture {
  async fn publish_group(&self, members: Vec<WorkSourceInput>, previous: Option<&WorkSourceGroup>) -> WorkSourceGroup {
    let group = match previous {
      Some(group) => group.replace(WorkSourceGroupVersion::new(group.version().get() + 1).unwrap(), members),
      None => WorkSourceGroup::publish(WorkSourceGroupId::generate(), self.input.project_id(), members),
    }
    .unwrap();
    self
      .store
      .publish_work_sources(PublishWorkSources {
        project_id: self.input.project_id(),
        inputs: vec![],
        group: Some(WorkSourceGroupChange {
          expected_version: previous.map(WorkSourceGroup::version),
          group: group.clone(),
        }),
      })
      .await
      .unwrap();
    group
  }

  async fn reserve_pending(&self, target: WorkSourceScanTarget) -> WorkSourcePreparation {
    let scan = self
      .store
      .claim_source_scan(ClaimWorkSourceScan {
        project_id: self.input.project_id(),
        target,
        owner: key("interrupted"),
        now: time(10),
        until: time(50),
      })
      .await
      .unwrap()
      .unwrap();
    let snapshot = WorkSourceSnapshot {
      selection_matches: vec![],
      item: WorkSourceItem {
        resource: ExternalWorkIdentity::new("native-container").unwrap(),
        item: ExternalWorkIdentity::new("original-pending-item").unwrap(),
      },
      display_id: "TASK-1".into(),
      update_identity: "original".into(),
      title: "Original pending task".into(),
      body: Some("Original acceptance".into()),
      fields: Default::default(),
      labels: vec![],
      state: "open".into(),
    };
    self
      .store
      .reserve_source_preparation(ReserveWorkSourcePreparation {
        claim: scan.claim,
        input_id: self.input.id(),
        item: snapshot.item.clone(),
        snapshot: serde_json::to_vec(&snapshot).unwrap(),
        now: time(11),
      })
      .await
      .unwrap()
  }
}

#[tokio::test]
async fn disabled_members_pending_work_does_not_block_the_current_group() {
  let fixture = Fixture::enabled("jira").await;
  let config = fixture
    .store
    .factory_configuration_version(
      fixture.input.definition().target.id(),
      fixture.input.definition().target.version(),
    )
    .await
    .unwrap();
  let capability = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: fixture.profile.binding.clone(),
  };
  let second = WorkSourceInput::publish(
    WorkSourceInputId::generate(),
    fixture.input.project_id(),
    fixture.input.definition().clone(),
    WorkSourceInputContext {
      target: &config.configuration,
      repository: &fixture.input.definition().repository,
      source: Some(&capability),
      reporter: None,
    },
  )
  .unwrap();
  fixture
    .store
    .publish_work_sources(PublishWorkSources {
      project_id: fixture.input.project_id(),
      inputs: vec![WorkSourceInputChange {
        expected_version: None,
        input: second.clone(),
      }],
      group: None,
    })
    .await
    .unwrap();
  let group = fixture
    .publish_group(vec![fixture.input.clone(), second.clone()], None)
    .await;
  let retained = fixture
    .reserve_pending(WorkSourceScanTarget::Group {
      id: group.id(),
      version: group.version(),
    })
    .await;
  let disabled = fixture.replace_enabled(false).await;
  fixture
    .publish_group(vec![disabled, second.clone()], Some(&group))
    .await;

  let outcome = fixture.worker().run_once(CancellationToken::new()).await.unwrap();
  assert_eq!(
    (outcome.completed, outcome.deferred),
    (1, 0),
    "a disabled member must not stop enabled members"
  );
  assert_eq!(fixture.runs().await.len(), 1);
  {
    let published = fixture.bytes.0.lock().unwrap();
    assert!(!published.is_empty());
    assert!(
      published
        .iter()
        .all(|bytes| !String::from_utf8_lossy(bytes).contains("Original pending task")),
      "disabled input snapshots must not be materialized"
    );
  }
  assert_eq!(
    fixture
      .store
      .source_preparation(fixture.input.project_id(), retained.id)
      .await
      .unwrap(),
    retained,
    "retired work remains private and unchanged for ordinary retention"
  );
}

#[tokio::test]
async fn pending_work_follows_its_stable_input_into_an_exclusive_group() {
  for mapped in [false, true] {
    let fixture = Fixture::enabled("gitlab").await;
    let target = WorkSourceScanTarget::Input {
      id: fixture.input.id(),
      version: fixture.input.version(),
    };
    let retained = fixture.reserve_pending(target).await;
    if mapped {
      fixture.clock.0.store(60, Ordering::SeqCst);
      let scan = fixture
        .store
        .claim_source_scan(ClaimWorkSourceScan {
          project_id: fixture.input.project_id(),
          target,
          owner: key("mapping-before-group"),
          now: time(60),
          until: time(80),
        })
        .await
        .unwrap()
        .unwrap();
      FactoryWorkSourcePreparer::new(fixture.store.clone(), fixture.bytes.clone(), fixture.clock.clone())
        .prepare(scan.claim, retained.id, fixture.profile.preparation.clone())
        .await
        .unwrap();
    }
    let original = fixture
      .store
      .source_preparation(fixture.input.project_id(), retained.id)
      .await
      .unwrap();
    fixture.publish_group(vec![fixture.input.clone()], None).await;
    // Recovery must find the original item even if it is absent from a new provider page.
    std::fs::write(fixture.root.path().join("gitlab/mode.json"), br#"{"candidates":[]}"#).unwrap();
    fixture.clock.0.store(100, Ordering::SeqCst);
    let outcome = fixture.worker().run_once(CancellationToken::new()).await.unwrap();
    assert_eq!((outcome.completed, outcome.deferred), (1, 0));
    let runs = fixture.runs().await;
    assert_eq!(
      runs.len(),
      1,
      "the current group must resume the original input without rediscovery"
    );
    let admitted = fixture
      .store
      .source_preparation(fixture.input.project_id(), retained.id)
      .await
      .unwrap();
    assert_eq!(admitted.snapshot, original.snapshot);
    assert_eq!(admitted.input, original.input);
    assert_eq!(
      admitted.target, original.target,
      "the original observation's provenance is immutable"
    );
    if let Some(intent) = original.intent {
      assert_eq!(admitted.intent, Some(intent));
    }
    assert_eq!(admitted.admission.unwrap().run_id, runs[0].id);
    assert_eq!(
      fixture.bytes.0.lock().unwrap().len(),
      2,
      "recovery never republishes original artifacts"
    );
  }
}

#[tokio::test]
async fn factory_wip_backpressure_does_not_exhaust_the_provider_retry_budget_or_republish_inputs() {
  let mut fixture = Fixture::enabled("jira").await;
  fixture.profile.polling.page_items = 3;
  fixture.profile.polling.max_items = 3;
  let path = fixture.root.path().join("jira/mode.json");
  let mut mode: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
  let mut second = mode["candidates"][0].clone();
  second["item_id"] = serde_json::json!("second-native-ticket");
  second["display_id"] = serde_json::json!("TASK-8");
  mode["candidates"].as_array_mut().unwrap().push(second);
  let mut third = mode["candidates"][0].clone();
  third["item_id"] = serde_json::json!("third-native-ticket");
  third["display_id"] = serde_json::json!("TASK-9");
  mode["candidates"].as_array_mut().unwrap().push(third);
  std::fs::write(path, serde_json::to_vec(&mode).unwrap()).unwrap();
  let target = WorkSourceScanTarget::Input {
    id: fixture.input.id(),
    version: fixture.input.version(),
  };
  for at in [100, 110, 120, 130, 140] {
    fixture.clock.0.store(at, Ordering::SeqCst);
    assert_eq!(
      fixture
        .worker()
        .run_once(CancellationToken::new())
        .await
        .unwrap()
        .deferred,
      1
    );
    let schedule = fixture
      .store
      .source_scan_schedule(fixture.input.project_id(), target)
      .await
      .unwrap()
      .unwrap();
    assert!(
      !schedule.exhausted,
      "ordinary Factory WIP must defer the retained item without disabling its Source"
    );
    assert_eq!(schedule.attempts, 0);
    assert_eq!(fixture.runs().await.len(), 2);
    assert_eq!(
      fixture.bytes.0.lock().unwrap().len(),
      6,
      "original prepared bytes must be reused while capacity is unavailable"
    );
  }
  assert_eq!(
    fixture
      .store
      .pending_source_preparations(fixture.input.project_id(), target, None, 2)
      .await
      .unwrap()
      .len(),
    1
  );
}

#[tokio::test]
async fn expired_pending_snapshots_wait_for_retention_without_exhausting_or_resurrecting_work() {
  let fixture = Fixture::enabled("jira").await;
  let target = WorkSourceScanTarget::Input {
    id: fixture.input.id(),
    version: fixture.input.version(),
  };
  let scan = fixture
    .store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: fixture.input.project_id(),
      target,
      owner: key("orphaned"),
      now: time(10),
      until: time(200),
    })
    .await
    .unwrap()
    .unwrap();
  let snapshot = WorkSourceSnapshot {
    selection_matches: vec![],
    item: WorkSourceItem {
      resource: ExternalWorkIdentity::new("native-container").unwrap(),
      item: ExternalWorkIdentity::new("expired").unwrap(),
    },
    display_id: "TASK-1".into(),
    update_identity: "original".into(),
    title: "Retained task".into(),
    body: Some("Explicit acceptance".into()),
    fields: Default::default(),
    labels: vec![],
    state: "open".into(),
  };
  let retained = fixture
    .store
    .reserve_source_preparation(ReserveWorkSourcePreparation {
      claim: scan.claim.clone(),
      input_id: fixture.input.id(),
      item: snapshot.item.clone(),
      snapshot: serde_json::to_vec(&snapshot).unwrap(),
      now: time(11),
    })
    .await
    .unwrap();
  let expired = retained.snapshot_expires_at.unix_millis();
  fixture.clock.0.store(expired, Ordering::SeqCst);
  for at in [expired, expired + 10, expired + 20, expired + 30] {
    fixture.clock.0.store(at, Ordering::SeqCst);
    assert_eq!(
      fixture
        .worker()
        .run_once(CancellationToken::new())
        .await
        .unwrap()
        .deferred,
      1
    );
    let schedule = fixture
      .store
      .source_scan_schedule(fixture.input.project_id(), target)
      .await
      .unwrap()
      .unwrap();
    assert!(
      !schedule.exhausted,
      "waiting for bounded private cleanup must not permanently disable a Source"
    );
    assert_eq!(schedule.attempts, 0);
  }
  let cleanup = FactorySourceInputRetentionWorker::new(
    fixture.store.clone(),
    super::factory_tests::resources().objects,
    Duration::from_secs(1),
  )
  .unwrap();
  assert_eq!(
    cleanup.run_once(time(expired + 30), None, 4).await.unwrap().completed,
    1
  );
  let path = fixture.root.path().join("jira/mode.json");
  let mut observed: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
  observed["candidates"][0]["item_id"] = serde_json::json!("expired");
  std::fs::write(path, serde_json::to_vec(&observed).unwrap()).unwrap();
  fixture.clock.0.store(expired + 40, Ordering::SeqCst);
  assert_eq!(
    fixture
      .worker()
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  assert!(fixture.runs().await.is_empty());
  assert!(fixture.bytes.0.lock().unwrap().is_empty());
  assert!(
    fixture
      .store
      .source_preparation(fixture.input.project_id(), retained.id)
      .await
      .unwrap()
      .snapshot
      .is_empty()
  );
}

// Trusted installed fixture; private provider material is simulated only at the store boundary.
async fn managed_access(
  fixture: &Fixture,
  binding: &WorkSourceBinding,
  purpose: IntegrationPurpose,
) -> (ManagedIntegrationAccess, IntegrationId) {
  let id = IntegrationId::generate();
  let credential = octacity_server_store::testing::integration_connection_fixture("custom-tracker")
    .0
    .resources[0]
    .capabilities[0]
    .credential
    .clone();
  let installed = InstalledIntegration {
    installation: binding.installation.clone(),
    provider: key("custom-tracker"),
    adapter: fixture.profile.plugin.clone(),
    endpoints: vec!["https://tracker.invalid/".into()],
    authentication: vec![key("token")],
    resources: vec![IntegrationResource {
      reference: binding.resource.clone(),
      native_id: ExternalWorkIdentity::new("native-container").unwrap(),
      locator: ExternalWorkIdentity::new("private-container").unwrap(),
      capabilities: vec![IntegrationCapability {
        purpose,
        capability: binding.capability.clone(),
        configuration: binding.configuration.clone(),
        credential: credential.clone(),
      }],
    }],
  };
  let resources = vec![IntegrationResourceSelection {
    reference: binding.resource.clone(),
    purposes: vec![purpose],
  }];
  fixture
    .store
    .publish_integration_connection(octacity_server_store::testing::management_mutation(
      PublishIntegrationConnection {
        intent: IntegrationConnectionIntent {
          id,
          expected_current_version: None,
          draft: IntegrationConnectionDraft {
            name: IntegrationName::new("Managed tracker").unwrap(),
            installation: binding.installation.clone(),
            endpoint: "https://tracker.invalid/".into(),
            authentication: key("token"),
            resources: resources.clone(),
          },
        },
        installed,
        idempotency_key: IdempotencyKey::new("managed-metadata").unwrap(),
        published_at: time(1),
      },
    ))
    .await
    .unwrap();
  let prepared = fixture
    .store
    .begin_integration_administration(octacity_server_store::testing::management_mutation(
      BeginIntegrationAdministration {
        intent: IntegrationAdministrationIntent {
          id,
          expected_metadata_version: IntegrationVersion::INITIAL,
          expected_revision: 0,
          action: IntegrationAdministrationAction::Credential {
            reference: credential,
            kind: serde_json::from_value(serde_json::json!("adopt")).unwrap(),
            activate_connection: true,
          },
        },
        idempotency_key: IdempotencyKey::new("managed-activate").unwrap(),
        requested_at: time(2),
      },
    ))
    .await
    .unwrap();
  fixture
    .store
    .complete_integration_administration(octacity_server_store::testing::management_mutation(prepared))
    .await
    .unwrap();
  fixture
    .store
    .publish_integration_project_access(octacity_server_store::testing::management_mutation(
      PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id: id,
          connection_version: IntegrationVersion::INITIAL,
          project_id: fixture.input.project_id(),
          expected_current_version: None,
          resources,
        },
        idempotency_key: IdempotencyKey::new("managed-project-grant").unwrap(),
        published_at: time(3),
      },
    ))
    .await
    .unwrap();
  (
    ManagedIntegrationAccess::new(fixture.store.clone(), id, IntegrationVersion::INITIAL),
    id,
  )
}

#[tokio::test]
async fn managed_source_rechecks_project_grants_before_provider_reads_after_revocation() {
  let mut fixture = Fixture::enabled("custom-tracker").await;
  let (access, id) = managed_access(&fixture, &fixture.profile.binding, IntegrationPurpose::TaskRead).await;
  fixture.profile = fixture.profile.clone().with_managed_access(access);
  assert_eq!(
    fixture
      .worker()
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  let original = fixture.runs().await;
  assert_eq!(original.len(), 1);
  let requests = fixture.root.path().join("custom-tracker/requests.jsonl");
  let before = std::fs::read(&requests).unwrap();
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
        idempotency_key: IdempotencyKey::new("revoke-managed-read").unwrap(),
        published_at: time(110),
      },
    ))
    .await
    .unwrap();
  fixture.clock.0.store(200, Ordering::SeqCst);
  let result = fixture.worker().run_once(CancellationToken::new()).await.unwrap();
  assert_eq!(result.deferred, 1);
  assert_eq!(
    std::fs::read(&requests).unwrap(),
    before,
    "revocation precedes any provider or credential use"
  );
  assert_eq!(fixture.runs().await, original);
}
#[path = "work_source_preview_tests.rs"]
mod preview;
