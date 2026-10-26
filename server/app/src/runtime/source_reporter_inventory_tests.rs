//! Shared installed declarations govern authoring and ordinary outbox dispatch through normal startup.
use super::*;
use octacity_server_store_postgres::PostgresStore;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt as _;

async fn installed_reporter(
  fixture: &Fixture<PostgresStore>,
  database_url: &str,
) -> (
  tempfile::TempDir,
  crate::ServerConfig,
  Arc<crate::runtime::source_management::SourceManagement>,
) {
  let deployment = tempfile::tempdir().unwrap();
  let connection = fixture.input.definition().connection.as_ref().unwrap();
  let metadata = fixture.store.integration_connection(connection.id, None).await.unwrap();
  let writer = metadata.definition.resources[0]
    .capabilities
    .iter()
    .find(|c| c.purpose == IntegrationPurpose::TaskUpdate)
    .unwrap();
  let draft = reporter_draft(fixture, None, None);
  let installed = InstalledIntegration {
    installation: metadata.definition.installation.clone(),
    provider: metadata.definition.provider.clone(),
    adapter: metadata.definition.adapter.clone(),
    endpoints: vec![metadata.definition.endpoint.clone()],
    authentication: vec![metadata.definition.authentication.clone()],
    resources: metadata.definition.resources.clone(),
  };
  installed.validate().unwrap();
  let registry = octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap();
  registry
    .resolve(
      installed.adapter.identity().as_str(),
      &installed.adapter.digest().to_string(),
    )
    .unwrap();
  let profiles = deployment.path().join("shared-source-profiles.json");
  std::fs::write(
    &profiles,
    serde_json::to_vec(&json!({"version":1,
      "consumer":{"poll_interval_millis":10,"batch_size":4,"initial_retry_millis":10,"maximum_retry_millis":50},
      "reporters":[{"identity":"reporter-settings","version":"v1", "write":{
        "installation":draft.installation,"resource_reference":draft.resource,"capability":writer,
        "plugin":draft.plugin,"provider_configuration":draft.provider_configuration,"resource":draft.access,
        "network_hosts":draft.network_hosts},"mapping":draft.mapping,"credential_profile":draft.credential_profile}]
    }))
    .unwrap(),
  )
  .unwrap();
  let catalog = deployment.path().join("shared-integrations.json");
  std::fs::write(
    &catalog,
    serde_json::to_vec(&json!({"version":1,"registry":fixture.root.path(),
    "installations":[installed],"source_profiles_file":profiles}))
    .unwrap(),
  )
  .unwrap();
  for path in [&profiles, &catalog] {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
  }
  let config = crate::ServerConfig::parse_toml(&crate::runtime::test_runtime_config::runtime_config_source(
    deployment.path(),
    database_url,
    "http://127.0.0.1:9",
    &format!(
      "[integration_management]\nsocket_path={:?}\npeer_uid={}\nprincipal='inventory-test'\ncatalog_file={:?}\n",
      deployment.path().join("management.sock").to_str().unwrap(),
      rustix::process::geteuid().as_raw(),
      catalog.to_str().unwrap(),
    ),
  ))
  .unwrap();
  let installed = crate::runtime::integration_management::install(
    &config,
    fixture.store.clone(),
    CancellationToken::new(),
    &mut crate::runtime::tests::healthy_checks(),
  )
  .await
  .unwrap()
  .unwrap();
  (deployment, config, installed.source_management.unwrap())
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn conflicting_reporter_owners_preserve_the_original_pending_effect_without_adapter_io() {
  let (fixture, profile) = fixture("opaque-reporter").await;
  let (fixture, database) = ordering::postgres::postgres_fixture(fixture).await;
  let (fixture, profile) = managed_fixture_with_native_parent(fixture, profile, Some(exact("native-writer", 71))).await;
  let (_deployment, _config, sources) = installed_reporter(&fixture, &database.url()).await;
  let run = admit_managed(&fixture).await;
  let projector = Arc::new(
    sources
      .reporter_projector(fixture.input.project_id(), fixture.clock.clone())
      .unwrap(),
  );
  accept_status_at(&fixture, projector, run, 100).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let shared = sources.reporting_dispatcher(&mut vec![]).unwrap().unwrap();
  let wrong_profile = profile
    .with_native_configuration(exact("another-native-writer", 72))
    .unwrap();
  let wrong_owner = Arc::new(reporting_worker(
    &fixture,
    hosted(&fixture, vec![wrong_profile]),
    "different-profile-owner",
    10000,
  ));
  let mut explicit = crate::runtime::factory::install_outbox(
    vec![wrong_owner],
    shared.policy,
    &mut crate::runtime::tests::healthy_checks(),
  );
  assert!(matches!(
    sources.reporting_dispatcher(&mut explicit),
    Err(ApplicationError::CapabilityUnavailable)
  ));
  assert!(explicit.is_empty(), "both ambiguous owners must stop before claims");
  assert_eq!(
    fixture.store.factory_run_snapshot(run).await.unwrap().outbox,
    before.outbox
  );
  assert!(captured_requests(&fixture.root.path().join("opaque-reporter")).is_empty());
  drop(shared);
  drop(sources);
  drop(fixture);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn normal_reporter_dispatch_reuses_the_authoring_inventory_and_original_native_parent() {
  for scenario in ["allowed", "revoked", "replaced-parent"] {
    let allow_write = scenario == "allowed";
    let (fixture, profile) = fixture("opaque-reporter").await;
    let (fixture, database) = ordering::postgres::postgres_fixture(fixture).await;
    let native_parent = exact("trusted-native-writer", 71);
    let (fixture, profile) = managed_fixture_with_native_parent(fixture, profile, Some(native_parent.clone())).await;
    let (_deployment, config, sources) = installed_reporter(&fixture, &database.url()).await;
    let context = crate::runtime::local_management::request_context("inventory-test");
    let connection = fixture.input.definition().connection.as_ref().unwrap();
    let authoring = sources.for_project(fixture.input.project_id(), &context).unwrap();
    let choices = AuthorizedQueryHandler::new(sources.policy.clone(), Arc::new(authoring))
      .handle_query(
        &context,
        ListWorkSourceInputChoicesQuery {
          project_id: fixture.input.project_id(),
          connection_id: connection.id,
          connection_version: connection.version,
        },
      )
      .await
      .unwrap();
    assert_eq!(choices.items.len(), 1);
    assert_eq!(choices.items[0].binding, *profile.binding());
    assert_eq!(choices.items[0].connection_configuration, native_parent);
    assert_ne!(choices.items[0].binding.configuration, native_parent);
    let at = SystemFactoryExecutionClock.now().unwrap().unix_millis();
    fixture.clock.0.store(at, Ordering::SeqCst);
    let run = admit_managed(&fixture).await;
    let projector = Arc::new(
      sources
        .reporter_projector(fixture.input.project_id(), fixture.clock.clone())
        .unwrap(),
    );
    accept_status_at(&fixture, projector, run, at).await;
    if scenario == "revoked" {
      fixture
        .store
        .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
          intent: IntegrationProjectAccessIntent {
            connection_id: connection.id,
            connection_version: connection.version,
            project_id: fixture.input.project_id(),
            expected_current_version: Some(connection.access_version),
            resources: vec![IntegrationResourceSelection {
              reference: fixture.input.definition().source.resource.clone(),
              purposes: vec![IntegrationPurpose::TaskRead],
            }],
          },
          idempotency_key: "revoke-inventory-writer".parse().unwrap(),
          published_at: Timestamp::from_unix_millis(at + 21).unwrap(),
        }))
        .await
        .unwrap();
    }
    if scenario == "replaced-parent" {
      replace_installed_native_parent(_deployment.path());
    }
    let before = fixture.store.factory_run_snapshot(run).await.unwrap();
    let directory = fixture.root.path().join("opaque-reporter");
    assert!(captured_requests(&directory).is_empty());
    let runtime = crate::ServerRuntime::start(config).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    let client = reqwest::Client::builder()
      .unix_socket(_deployment.path().join("management.sock"))
      .build()
      .unwrap();
    if scenario != "replaced-parent" {
      assert_live_writer_project(
        &client,
        connection,
        &fixture.input.definition().source.resource,
        profile.binding(),
      )
      .await;
    }
    loop {
      let observed = fixture.store.factory_run_snapshot(run).await.unwrap();
      let state = if allow_write {
        FactoryOutboxState::Delivered
      } else {
        FactoryOutboxState::Failed
      };
      if observed
        .outbox
        .iter()
        .any(|row| row.kind.as_str() == "work.status.projection" && row.state == state)
      {
        break;
      }
      assert!(
        tokio::time::Instant::now() < deadline,
        "normal shared Reporter must settle accepted delivery under current Project grants"
      );
      tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(directory.join("reporter-applied").exists(), allow_write);
    let after = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(after.work, before.work);
    assert_eq!(after.run, before.run);
    assert_eq!(after.flow, before.flow);
    let calls: Vec<Value> = captured_requests(&directory)
      .lines()
      .map(|line| serde_json::from_str(line).unwrap())
      .collect();
    assert_eq!(
      calls
        .iter()
        .any(|call| call["message"]["command"]["operation"] == "project"
          || call["message"]["command"]["operation"] == "apply"),
      allow_write
    );
    if !allow_write {
      assert!(
        calls.is_empty(),
        "a read grant cannot borrow writer authority from the second Project"
      );
    }
    runtime.shutdown().await.unwrap();
    drop(sources);
    drop(fixture);
    database.cleanup().await;
  }
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn installed_managed_reporter_cannot_borrow_legacy_authority_without_original_connection_pin() {
  let (fixture, profile) = fixture("opaque-reporter").await;
  let (fixture, database) = ordering::postgres::postgres_fixture(fixture).await;
  let (mut fixture, profile) =
    managed_fixture_with_native_parent(fixture, profile, Some(exact("native-writer", 71))).await;
  let (_deployment, _config, sources) = installed_reporter(&fixture, &database.url()).await;
  let mut definition = fixture.input.definition().clone();
  definition.connection = None;
  let selected = fixture
    .store
    .factory_admission_context(ReadFactoryAdmissionContext {
      project_id: fixture.input.project_id(),
      configuration_id: definition.target.id(),
      configuration_version: definition.target.version(),
      repository_id: definition.repository.id,
      repository_version: definition.repository.version,
    })
    .await
    .unwrap();
  let source = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: definition.source.clone(),
  };
  let reporter = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Reporter,
    binding: profile.binding().clone(),
  };
  let repository = definition.repository.clone();
  let legacy = fixture
    .input
    .replace(
      WorkSourceInputVersion::new(fixture.input.version().get() + 1).unwrap(),
      definition,
      WorkSourceInputContext {
        target: &selected.configuration.configuration,
        repository: &repository,
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
        input: legacy.clone(),
      }],
      group: None,
    })
    .await
    .unwrap();
  fixture.input = legacy;
  let projector = Arc::new(
    sources
      .reporter_projector(fixture.input.project_id(), fixture.clock.clone())
      .unwrap(),
  );
  let run = queue(&fixture, projector.clone()).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let outcome = reporting_worker(&fixture, projector, "managed-no-legacy", 10000)
    .run_once(time(120))
    .await
    .unwrap();
  assert_eq!(
    outcome.delivered, 0,
    "a declared managed writer cannot fall back to legacy private-handle authority"
  );
  assert!(captured_requests(&fixture.root.path().join("opaque-reporter")).is_empty());
  assert_eq!(fixture.store.factory_run_snapshot(run).await.unwrap().work, before.work);
  drop(sources);
  drop(fixture);
  database.cleanup().await;
}

async fn assert_live_writer_project(
  client: &reqwest::Client,
  connection: &WorkSourceConnectionRef,
  resource: &ImmutableReference,
  original: &WorkSourceBinding,
) {
  let response = client
    .post("http://localhost/api/v1/projects")
    .header("idempotency-key", "live-writer-project")
    .json(&json!({"name":"Live writer Project"}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::CREATED);
  let created: Value = response.json().await.unwrap();
  let project = created["resource"]["id"].as_str().unwrap();
  let path = format!(
    "http://localhost/api/v1/projects/{project}/factory-source-choices?connection_id={}&connection_version={}",
    connection.id,
    connection.version.get()
  );
  assert_eq!(
    client.get(&path).send().await.unwrap().status(),
    reqwest::StatusCode::UNPROCESSABLE_ENTITY
  );
  let response = client
    .post(format!(
      "http://localhost/api/v1/integrations/{}/projects/{project}/access",
      connection.id
    ))
    .header("idempotency-key", "grant-live-writer-project")
    .header("if-none-match", "*")
    .json(
      &json!({"intent":{"connection_id":connection.id,"connection_version":connection.version,"project_id":project,
      "expected_current_version":null,"resources":[{"reference":resource,"purposes":["task_update"]}]}}),
    )
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  let response = client.get(&path).send().await.unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let choices: Value = response.json().await.unwrap();
  assert_eq!(choices["items"].as_array().unwrap().len(), 1);
  assert_ne!(
    choices["items"][0]["binding"],
    serde_json::to_value(original).unwrap(),
    "a live second Project gets its own immutable writer profile"
  );
  assert_eq!(choices["items"][0]["purpose"], "task_update");
}

fn replace_installed_native_parent(deployment: &std::path::Path) {
  let parent = serde_json::to_value(exact("replacement-native-writer", 72)).unwrap();
  let catalog_path = deployment.join("shared-integrations.json");
  let profiles_path = deployment.join("shared-source-profiles.json");
  let mut catalog: Value = serde_json::from_slice(&std::fs::read(&catalog_path).unwrap()).unwrap();
  let capabilities = catalog["installations"][0]["resources"][0]["capabilities"]
    .as_array_mut()
    .unwrap();
  capabilities.iter_mut().find(|c| c["purpose"] == "task_update").unwrap()["configuration"] = parent.clone();
  let mut profiles: Value = serde_json::from_slice(&std::fs::read(&profiles_path).unwrap()).unwrap();
  profiles["reporters"][0]["write"]["capability"]["configuration"] = parent;
  std::fs::write(catalog_path, serde_json::to_vec(&catalog).unwrap()).unwrap();
  std::fs::write(profiles_path, serde_json::to_vec(&profiles).unwrap()).unwrap();
}
