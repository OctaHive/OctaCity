//! Optional consumer contracts through the installed, verified Source process.
use super::*;

fn reader(fixture: &Fixture) -> HostedWorkSourceObservations {
  HostedWorkSourceObservations::new(
    Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
    vec![fixture.profile.read.clone()],
    fixture.clock.clone(),
    settings().operation_timeout,
    settings().cancellation_grace,
    CancellationToken::new(),
  )
  .unwrap()
}

fn request(fixture: &Fixture) -> ReadWorkSourceObservations {
  ReadWorkSourceObservations {
    project_id: fixture.profile.read.project_id,
    source: fixture.profile.read.binding.clone(),
    scan_id: FactoryDigest::from_bytes([81; 32]),
    window: None,
    continuation: None,
    limit: 2,
  }
}

#[tokio::test]
async fn independent_read_profile_observes_without_factory_preparation() {
  for provider in ["github", "fake-tracker"] {
    let fixture = Fixture::enabled(provider).await;
    let read = WorkSourceReadProfile {
      managed_access: None,
      project_id: fixture.profile.read.project_id,
      binding: fixture.profile.read.binding.clone(),
      plugin: fixture.profile.read.plugin.clone(),
      provider_configuration: fixture.profile.read.provider_configuration,
      resource: fixture.profile.read.resource.clone(),
      selection: fixture.profile.read.selection.clone(),
      network_hosts: fixture.profile.read.network_hosts.clone(),
      context: fixture.profile.read.context.clone(),
      polling: fixture.profile.read.polling.clone(),
    };
    let reader = HostedWorkSourceObservations::new(
      Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
      vec![read],
      fixture.clock.clone(),
      settings().operation_timeout,
      settings().cancellation_grace,
      CancellationToken::new(),
    )
    .unwrap();
    let page = reader
      .read(&fixture.profile.read.context, request(&fixture))
      .await
      .unwrap();
    assert_eq!(page.observations[0].snapshot.title, "Original task");
    assert_eq!(page.observations[0].snapshot.item.item.as_str(), "TASK-native");
    assert!(fixture.runs().await.is_empty());
    assert!(fixture.bytes.0.lock().unwrap().is_empty());
  }
}

#[tokio::test]
async fn optional_observations_carry_trusted_native_scope_without_admitting_work() {
  for provider in ["github", "fake-tracker"] {
    let fixture = Fixture::enabled(provider).await;
    let page = reader(&fixture)
      .read(&fixture.profile.read.context, request(&fixture))
      .await
      .unwrap();
    assert_eq!(page.observations.len(), 1);
    assert!(page.next_cursor.is_none());
    let observed = &page.observations[0];
    assert_eq!(observed.project_id, fixture.profile.read.project_id);
    assert_eq!(observed.security_scope, *fixture.profile.read.context.security_scope());
    assert_eq!(observed.source, fixture.profile.read.binding);
    assert_eq!(observed.plugin, fixture.profile.read.plugin);
    assert_eq!(
      observed.provider_configuration,
      fixture.profile.read.provider_configuration
    );
    assert_eq!(observed.snapshot.item.resource.as_str(), "native-container");
    assert_eq!(observed.snapshot.item.item.as_str(), "TASK-native");
    assert_eq!(observed.snapshot.display_id, "TASK-7");
    assert_eq!(observed.snapshot.body.as_deref(), Some("Explicit acceptance"));
    assert_eq!(observed.observed_at, time(100));
    assert_eq!(observed.expires_at, time(86400100));
    assert_eq!(
      observed.snapshot_digest,
      FactoryDigest::content_sha256(&serde_json::to_vec(&observed.snapshot).unwrap())
    );
    assert_eq!(
      observed.native_correlation,
      source_native_correlation(
        source_native_issue(
          fixture.profile.read.project_id,
          fixture.profile.read.binding.installation.identity(),
          &observed.snapshot.item
        ),
        source_security_partition(&octacity_server_store::ManagementSecurityScope::trusted_network()),
      )
    );
    assert!(fixture.runs().await.is_empty());
    assert!(fixture.bytes.0.lock().unwrap().is_empty());
    assert!(
      fixture
        .store
        .source_scan_schedule(
          fixture.input.project_id(),
          WorkSourceScanTarget::Input {
            id: fixture.input.id(),
            version: fixture.input.version(),
          }
        )
        .await
        .unwrap()
        .is_none()
    );
    let calls = std::fs::read_to_string(fixture.root.path().join(provider).join("requests.jsonl")).unwrap();
    assert_eq!(
      calls.lines().count(),
      1,
      "one bounded existing Source read, no hidden polling"
    );
  }
}

#[tokio::test]
async fn observation_freshness_uses_bounded_content_provenance_not_opaque_update_order() {
  let fixture = Fixture::enabled("fake-tracker").await;
  let reader = reader(&fixture);
  let first = reader
    .read(&fixture.profile.read.context, request(&fixture))
    .await
    .unwrap()
    .observations
    .remove(0);
  assert_eq!(
    first.freshness_at(time(100), None),
    WorkSourceObservationFreshness::Fresh
  );
  fixture.clock.0.store(200, Ordering::SeqCst);
  let unchanged = reader
    .read(&fixture.profile.read.context, request(&fixture))
    .await
    .unwrap()
    .observations
    .remove(0);
  assert_eq!(
    unchanged.freshness_at(time(200), Some(&first)),
    WorkSourceObservationFreshness::Unchanged
  );
  let path = fixture.root.path().join("fake-tracker/mode.json");
  let mut mode: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
  mode["candidates"][0]["body"] = serde_json::json!("Changed body with the same opaque update identity");
  std::fs::write(path, serde_json::to_vec(&mode).unwrap()).unwrap();
  fixture.clock.0.store(300, Ordering::SeqCst);
  let fresh = reader
    .read(&fixture.profile.read.context, request(&fixture))
    .await
    .unwrap()
    .observations
    .remove(0);
  assert_eq!(fresh.snapshot.update_identity, first.snapshot.update_identity);
  assert_ne!(fresh.snapshot_digest, first.snapshot_digest);
  assert_eq!(
    fresh.freshness_at(time(300), Some(&unchanged)),
    WorkSourceObservationFreshness::Fresh
  );
  assert_eq!(
    first.freshness_at(time(300), Some(&fresh)),
    WorkSourceObservationFreshness::Stale
  );
  assert_eq!(
    fresh.freshness_at(time(299), None),
    WorkSourceObservationFreshness::Stale
  );
  assert_eq!(
    first.freshness_at(time(86400100), None),
    WorkSourceObservationFreshness::Stale
  );
  assert_eq!(
    first.expires_at,
    time(86400100),
    "a new read cannot renew old provenance"
  );
}

async fn revoke(fixture: &Fixture, id: IntegrationId) {
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
        idempotency_key: IdempotencyKey::new("revoke-observation-read").unwrap(),
        published_at: time(110),
      },
    ))
    .await
    .unwrap();
}

#[tokio::test]
async fn observation_revocation_before_read_or_during_read_withholds_protected_snapshot() {
  for during in [false, true] {
    let mut fixture = Fixture::enabled("fake-tracker").await;
    let (access, id) = managed_access(&fixture, &fixture.profile.read.binding, IntegrationPurpose::TaskRead).await;
    fixture.profile = fixture.profile.clone().with_managed_access(access);
    let path = fixture.root.path().join("fake-tracker/mode.json");
    let mut mode: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    mode["pause_scan"] = serde_json::json!(true);
    std::fs::write(path, serde_json::to_vec(&mode).unwrap()).unwrap();
    let reader = Arc::new(reader(&fixture));
    if !during {
      revoke(&fixture, id).await;
    }
    let context = fixture.profile.read.context.clone();
    let request = request(&fixture);
    let reading = tokio::spawn(async move { reader.read(&context, request).await });
    if during {
      fixture.wait_for_provider_file("ready").await;
      revoke(&fixture, id).await;
      fixture.release_provider();
    }
    assert!(
      reading.await.unwrap().is_err(),
      "revoked read must not expose a snapshot, including an in-flight response"
    );
    let calls = std::fs::read_to_string(fixture.root.path().join("fake-tracker/requests.jsonl")).unwrap_or_default();
    assert_eq!(
      calls.lines().count(),
      usize::from(during),
      "revocation before dispatch must not reach the provider"
    );
    assert!(fixture.runs().await.is_empty());
    assert!(fixture.bytes.0.lock().unwrap().is_empty());
  }
}

#[tokio::test]
async fn observation_installation_rejects_unbounded_or_ambiguous_read_profiles_before_activity() {
  let fixture = Fixture::enabled("fake-tracker").await;
  let registry = Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap());
  for defect in [
    "expired",
    "retention",
    "page-items",
    "page-bytes",
    "bytes",
    "pages",
    "items",
    "period",
    "duplicate",
  ] {
    let mut profile = fixture.profile.read.clone();
    match defect {
      "expired" => profile.polling.retention_millis = 0,
      "retention" => profile.polling.retention_millis = MAX_SOURCE_RETENTION_MILLIS + 1,
      "page-items" => profile.polling.page_items = 0,
      "page-bytes" => profile.polling.max_page_bytes = 0,
      "bytes" => profile.polling.max_bytes = profile.polling.max_page_bytes - 1,
      "pages" => profile.polling.max_pages = 0,
      "items" => profile.polling.max_items = 0,
      "period" => profile.polling.reconciliation_interval_millis = 0,
      "duplicate" => {}
      _ => unreachable!(),
    }
    let profiles = if defect == "duplicate" {
      vec![profile.clone(), profile]
    } else {
      vec![profile]
    };
    assert!(
      HostedWorkSourceObservations::new(
        registry.clone(),
        profiles,
        fixture.clock.clone(),
        settings().operation_timeout,
        settings().cancellation_grace,
        CancellationToken::new()
      )
      .is_err(),
      "{defect}"
    );
  }
  assert!(!fixture.root.path().join("fake-tracker/requests.jsonl").exists());
}

#[tokio::test]
async fn observations_of_closed_nonmatching_tasks_share_two_original_input_runs_without_rewriting_them() {
  for provider in ["github", "fake-tracker"] {
    let mut fixture = Fixture::enabled(provider).await;
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
      binding: fixture.profile.read.binding.clone(),
    };
    let mut draft = fixture.input.definition().clone();
    draft.predicate = Some(WorkSourcePredicate {
      state: Some(key("native-state")),
      all_of: vec!["arbitrary".into()],
      any_of: vec![],
      none_of: vec![],
    });
    let second = WorkSourceInput::publish(
      WorkSourceInputId::generate(),
      fixture.input.project_id(),
      draft,
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
    let reader = reader(&fixture); // Constructing an unused optional port starts no observer.
    assert!(!fixture.root.path().join(provider).join("requests.jsonl").exists());
    assert_eq!(
      fixture
        .worker()
        .run_once(CancellationToken::new())
        .await
        .unwrap()
        .completed,
      2
    );
    let runs = fixture.runs().await;
    assert_eq!(runs.len(), 2);
    assert_ne!(runs[0].work_id, runs[1].work_id);
    let snapshots = [
      fixture.store.factory_run_snapshot(runs[0].id).await.unwrap(),
      fixture.store.factory_run_snapshot(runs[1].id).await.unwrap(),
    ];
    let artifacts = fixture.bytes.0.lock().unwrap().clone();
    let calls_path = fixture.root.path().join(provider).join("requests.jsonl");
    let calls = std::fs::read_to_string(&calls_path).unwrap();
    let operations: Vec<_> = calls
      .lines()
      .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap()["message"]["command"]["operation"].clone())
      .collect();
    assert_eq!(
      operations.iter().filter(|op| **op == "scan").count(),
      2,
      "no consumer means no additional page reads alongside ordinary intake"
    );
    let first = reader
      .read(&fixture.profile.read.context, request(&fixture))
      .await
      .unwrap()
      .observations
      .remove(0);
    let correlations = fixture
      .store
      .source_diagnostic_correlations(ReadWorkSourceCorrelations {
        project_id: fixture.input.project_id(),
        filter: WorkSourceDiagnosticFilter::Inputs(vec![fixture.input.id(), second.id()]),
        visibility: WorkSourceConfigurationVisibility {
          configurations: FactoryConfigurationListVisibility::all(),
          repositories: RepositoryListVisibility::all(),
          connections: IntegrationConnectionListVisibility::all(),
        },
        runs: FactoryRunListVisibility::all(),
        after: None,
        limit: 64,
      })
      .await
      .unwrap();
    assert_eq!(correlations.items.len(), 2);
    for association in &correlations.items {
      assert_eq!(association.native_correlation, first.native_correlation);
      assert!(
        runs
          .iter()
          .any(|run| run.id == association.run_id && run.work_id == association.work_id)
      );
    }
    let path = fixture.root.path().join(provider).join("mode.json");
    let mut mode: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    mode["candidates"][0]["state"] = serde_json::json!("closed");
    mode["candidates"][0]["labels"] = serde_json::json!([]);
    mode["candidates"][0]["display_id"] = serde_json::json!("renamed-display");
    mode["candidates"][0]["body"] = serde_json::json!("Fresh source body after original admission");
    std::fs::write(path, serde_json::to_vec(&mode).unwrap()).unwrap();
    fixture.clock.0.store(200, Ordering::SeqCst);
    let changed = reader
      .read(&fixture.profile.read.context, request(&fixture))
      .await
      .unwrap()
      .observations
      .remove(0);
    assert_eq!(changed.native_correlation, first.native_correlation);
    assert_ne!(changed.snapshot_digest, first.snapshot_digest);
    assert_eq!(changed.snapshot.state, "closed");
    assert!(
      !second
        .definition()
        .predicate
        .as_ref()
        .unwrap()
        .mismatches(&changed.snapshot.state, &changed.snapshot.labels)
        .is_empty()
    );
    assert_eq!(
      fixture
        .worker()
        .run_once(CancellationToken::new())
        .await
        .unwrap()
        .completed,
      2
    );
    for (run, snapshot) in runs.iter().zip(&snapshots) {
      assert_eq!(fixture.store.factory_run_snapshot(run.id).await.unwrap(), *snapshot);
    }
    assert_eq!(fixture.bytes.0.lock().unwrap().as_slice(), artifacts.as_slice());
    assert_eq!(fixture.runs().await, runs);
    // Disable every execution target. The separately authorized observation read still works.
    fixture.replace_enabled(false).await;
    let mut disabled = second.definition().clone();
    disabled.enabled = false;
    let disabled = second
      .replace(
        WorkSourceInputVersion::new(2).unwrap(),
        disabled,
        WorkSourceInputContext {
          target: &config.configuration,
          repository: &second.definition().repository,
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
          expected_version: Some(second.version()),
          input: disabled,
        }],
        group: None,
      })
      .await
      .unwrap();
    assert!(
      fixture
        .store
        .source_scan_targets(fixture.input.project_id(), None, 64)
        .await
        .unwrap()
        .is_empty()
    );
    let independent = reader
      .read(&fixture.profile.read.context, request(&fixture))
      .await
      .unwrap();
    assert_eq!(independent.observations[0].native_correlation, first.native_correlation);
    fixture.profile.read.binding.configuration = exact("independent-link-read", 99);
    fixture.profile.read.selection.identity = "independently-installed-observation-selector".into();
    let independent = self::reader(&fixture)
      .read(&fixture.profile.read.context, request(&fixture))
      .await
      .unwrap();
    assert_eq!(
      independent.observations[0].native_correlation, first.native_correlation,
      "neither an input identity nor selector configuration belongs to native correlation"
    );
    assert_eq!(independent.observations[0].snapshot_digest, changed.snapshot_digest);
  }
}

#[tokio::test]
async fn observation_scope_bounds_and_failures_use_existing_protected_read_contracts() {
  let fixture = Fixture::enabled("fake-tracker").await;
  let reader = reader(&fixture);
  let other_scope = ManagementRequestContext::new(
    ManagementActor::authenticated("operator").unwrap(),
    octacity_server_application::ManagementSecurityScope::new("other-partition").unwrap(),
    ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
    ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
  )
  .unwrap();
  assert!(reader.read(&other_scope, request(&fixture)).await.is_err());
  let mut foreign = request(&fixture);
  foreign.project_id = ProjectId::generate();
  assert!(reader.read(&fixture.profile.read.context, foreign).await.is_err());
  let mut foreign = request(&fixture);
  foreign.source.installation = exact("other-installation", 90);
  assert!(reader.read(&fixture.profile.read.context, foreign).await.is_err());
  let mut invalid = request(&fixture);
  invalid.limit = 0;
  assert!(reader.read(&fixture.profile.read.context, invalid).await.is_err());
  assert!(!fixture.root.path().join("fake-tracker/requests.jsonl").exists());
  let mut bounded = request(&fixture);
  bounded.limit = u16::MAX;
  bounded.window = Some((time(10), time(20)));
  bounded.continuation = Some("opaque-existing-page-position".into());
  reader.read(&fixture.profile.read.context, bounded).await.unwrap();
  let calls = std::fs::read_to_string(fixture.root.path().join("fake-tracker/requests.jsonl")).unwrap();
  let wire: serde_json::Value = serde_json::from_str(calls.lines().next().unwrap()).unwrap();
  let payload = &wire["message"]["command"]["payload"];
  assert_eq!(payload["max_items"], 2);
  assert_eq!(payload["max_page_bytes"], 65536);
  assert_eq!(
    payload["window"],
    serde_json::json!({"from_millis":10,"until_millis":20})
  );
  assert_eq!(payload["cursor"], "opaque-existing-page-position");
  assert_eq!(payload["scan_id"], FactoryDigest::from_bytes([81; 32]).to_string());
  let path = fixture.root.path().join("fake-tracker/mode.json");
  std::fs::write(&path, br#"{"failure":{"class":"transient","retry_after_millis":2000}}"#).unwrap();
  assert!(matches!(
    reader.read(&fixture.profile.read.context, request(&fixture)).await,
    Err(ApplicationError::SourceDependency {
      class: WorkSourceFailure::RateLimited,
      retry_after_millis: Some(2000)
    })
  ));
  // A substituted native container and oversized snapshot cannot yield a partial observation.
  for defect in ["container", "body"] {
    let mut candidate = serde_json::json!({"container_id":"native-container","item_id":"task",
      "display_id":"TASK-7","update_identity":"opaque","title":"private","body":"body",
      "fields":{},"labels":[],"state":"closed"});
    if defect == "container" {
      candidate["container_id"] = serde_json::json!("substituted");
    } else {
      candidate["body"] = serde_json::json!("x".repeat(262145));
    }
    std::fs::write(
      &path,
      serde_json::to_vec(&serde_json::json!({"candidates":[candidate]})).unwrap(),
    )
    .unwrap();
    assert!(matches!(
      reader.read(&fixture.profile.read.context, request(&fixture)).await,
      Err(ApplicationError::SourceDependency {
        class: WorkSourceFailure::ProtocolFault,
        ..
      })
    ));
  }
  assert!(fixture.runs().await.is_empty());
  assert!(fixture.bytes.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn observation_cancellation_drains_the_existing_process_without_handoff_or_admission() {
  for before in [true, false] {
    let fixture = Fixture::enabled("fake-tracker").await;
    std::fs::write(fixture.root.path().join("fake-tracker/mode.json"), br#"{"hold":true}"#).unwrap();
    let cancellation = CancellationToken::new();
    let reader = HostedWorkSourceObservations::new(
      Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
      vec![fixture.profile.read.clone()],
      fixture.clock.clone(),
      settings().operation_timeout,
      settings().cancellation_grace,
      cancellation.clone(),
    )
    .unwrap();
    if before {
      cancellation.cancel();
    }
    let context = fixture.profile.read.context.clone();
    let request = request(&fixture);
    let reading = tokio::spawn(async move { reader.read(&context, request).await });
    if !before {
      fixture.wait_for_provider_file("ready").await;
      cancellation.cancel();
    }
    assert!(matches!(
      reading.await.unwrap(),
      Err(ApplicationError::SourceDependency {
        class: WorkSourceFailure::Cancelled,
        ..
      })
    ));
    if before {
      assert!(!fixture.root.path().join("fake-tracker/requests.jsonl").exists());
    } else {
      assert!(fixture.root.path().join("fake-tracker/cancelled").exists());
    }
    assert!(fixture.runs().await.is_empty());
    assert!(fixture.bytes.0.lock().unwrap().is_empty());
  }
}

#[tokio::test]
async fn retained_observation_authority_can_be_rechecked_without_listing_another_page() {
  let mut fixture = Fixture::enabled("fake-tracker").await;
  let (access, id) = managed_access(&fixture, &fixture.profile.read.binding, IntegrationPurpose::TaskRead).await;
  fixture.profile = fixture.profile.clone().with_managed_access(access);
  let reader = reader(&fixture);
  reader
    .authorize(
      &fixture.profile.read.context,
      fixture.profile.read.project_id,
      fixture.profile.read.binding.clone(),
    )
    .await
    .unwrap();
  revoke(&fixture, id).await;
  assert!(
    reader
      .authorize(
        &fixture.profile.read.context,
        fixture.profile.read.project_id,
        fixture.profile.read.binding.clone()
      )
      .await
      .is_err()
  );
  let calls = std::fs::read_to_string(fixture.root.path().join("fake-tracker/requests.jsonl")).unwrap_or_default();
  assert!(calls.is_empty(), "authority rechecks do not list tickets");
  assert!(fixture.runs().await.is_empty());
}
