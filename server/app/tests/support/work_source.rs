//! Shared installed Source fixtures for component and composed public-boundary tests.
use super::*;
pub(super) struct Clock(pub(super) AtomicI64);
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(Timestamp::from_unix_millis(self.0.load(Ordering::SeqCst)).unwrap())
  }
}
#[derive(Default)]
pub(super) struct Bytes(pub(super) std::sync::Mutex<Vec<Vec<u8>>>);
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
pub(super) fn settings() -> FactorySourceWorkerSettings {
  FactorySourceWorkerSettings {
    worker: toml::from_str("poll_interval_milliseconds=10\nclaim_lifetime_milliseconds=60000\nbatch_size=4\nconcurrency=2\n[retry]\nmax_backoff_milliseconds=40\nconsecutive_failure_limit=3\ncircuit_open_milliseconds=200").unwrap(),
    max_attempts: 3,
    operation_timeout: Duration::from_secs(10),
    cancellation_grace: Duration::from_secs(1),
  }
}

pub(super) fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
pub(super) struct Fixture<S = InMemoryFactoryConfigurationStore> {
  pub(super) root: tempfile::TempDir,
  pub(super) store: Arc<S>,
  pub(super) bytes: Arc<Bytes>,
  pub(super) clock: Arc<Clock>,
  pub(super) input: WorkSourceInput,
  pub(super) profile: FactorySourceProfile,
}
impl Fixture {
  pub(super) async fn wait_for_provider_file(&self, name: &str) {
    let path = self
      .root
      .path()
      .join(self.profile.read.plugin.identity().as_str())
      .join(name);
    tokio::time::timeout(TEST_WAIT, async {
      while !path.exists() {
        tokio::time::sleep(Duration::from_millis(5)).await;
      }
    })
    .await
    .expect("the controlled provider must reach its requested phase");
  }

  pub(super) fn release_provider(&self) {
    std::fs::write(
      self
        .root
        .path()
        .join(self.profile.read.plugin.identity().as_str())
        .join("release"),
      b"release",
    )
    .unwrap();
  }

  pub(super) async fn enabled(provider: &str) -> Self {
    Self::enabled_with_flow(provider, None).await
  }
  pub(super) async fn enabled_with_flow(provider: &str, flow: Option<FactoryFlowConfiguration>) -> Self {
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
        reporting: None,
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
      read: WorkSourceReadProfile {
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
      },
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
  pub(super) fn worker(&self) -> FactorySourceWorker<S, Bytes> {
    self.worker_with_settings(settings())
  }
  pub(super) fn worker_with_settings(&self, policy: FactorySourceWorkerSettings) -> FactorySourceWorker<S, Bytes> {
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
  pub(super) async fn runs(&self) -> Vec<FactoryRunSummary> {
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

#[path = "work_source_postgres.rs"]
mod postgres;
