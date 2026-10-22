//! Same controlled Source/Reporter journey with production durable association ownership.
use super::*;
#[allow(dead_code)]
#[path = "../../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod database;
#[path = "../support/runtime.rs"]
mod runtime;

#[allow(dead_code)]
#[path = "../support/console_pilot.rs"]
mod console_pilot;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and MinIO"]
async fn imported_card_keeps_independent_work_across_reporter_restart_and_second_vocabulary() {
  for (provider, labels) in [
    ("github", ["approved", "rejected"]),
    ("custom-tracker", ["green", "red"]),
  ] {
    let database = database::TestDatabase::migrated().await;
    let (fixture, profile) = fixture_with_policy(
      provider,
      None,
      Some(WorkSourceReportingPolicy {
        labels: labels.iter().map(|s| (*s).into()).collect(),
        states: vec![],
      }),
      Some(IssueReporterDeliveryPolicy {
        retry: FactoryReporterRetryPolicy {
          max_attempts: 3,
          retry_delay_millis: 100,
        },
        window_millis: 30000,
        budget: BudgetLimit::new(1, 5000, 0, 0, 10000).unwrap(),
      }),
    )
    .await;
    let fixture = fixture.into_postgres(database.pool.clone()).await;
    let Pilot {
      fixture,
      profile,
      connection,
      downstream_id,
      ..
    } = pipeline_with_target(fixture, profile, &labels, Some(controls::flow())).await;
    let directory = fixture.root.path().join(provider);
    std::fs::write(
      directory.join("reporter-mode.json"),
      br#"{"source_effects":true,"lose_project_response":true}"#,
    )
    .unwrap();
    let worker = fixture
      .worker()
      .with_managed_connections(ManagedIntegrationConnections::new(fixture.store.clone()));
    assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().completed, 2);
    let assessment = fixture.runs().await[0].id;
    accept_status_at(&fixture, managed_projector(&fixture, &profile), assessment, 100).await;
    assert_eq!(
      reporting_worker(&fixture, managed_projector(&fixture, &profile), "pilot-first", 10000)
        .run_once(time(120))
        .await
        .unwrap()
        .retrying,
      1
    );
    fixture.clock.0.store(220, Ordering::SeqCst);
    assert_eq!(
      reporting_worker(
        &fixture,
        managed_projector(&fixture, &profile),
        "pilot-restarted",
        10000
      )
      .run_once(time(220))
      .await
      .unwrap()
      .delivered,
      1
    );
    fixture.clock.0.store(2100, Ordering::SeqCst);
    assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().completed, 2);
    let runs = fixture.runs().await;
    assert_eq!(runs.len(), 2);
    assert_ne!(runs[0].work_id, runs[1].work_id);
    let downstream = runs.iter().find(|run| run.id != assessment).unwrap();
    assert_eq!(
      fixture
        .store
        .source_preparation_for_work(fixture.input.project_id(), downstream.work_id)
        .await
        .unwrap()
        .unwrap()
        .input
        .id(),
      downstream_id
    );
    // Only the production association worker creates links. No associate_issue_work seed.
    let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
    url.set_path(&database.name);
    let root = tempfile::tempdir().unwrap();
    let server = ServerRuntime::start(runtime::runtime_config(
      root.path(),
      url.as_str(),
      &std::env::var("OCTACITY_MINIO_ENDPOINT").unwrap(),
    ))
    .await
    .unwrap();
    let project = fixture.input.project_id();
    tokio::time::timeout(TEST_WAIT, async {
      loop {
        let mut complete = true;
        for run in &runs {
          complete &= fixture
            .store
            .issue_work_association(project, run.work_id)
            .await
            .unwrap()
            .is_some();
        }
        if complete {
          break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
      }
    })
    .await
    .unwrap();
    server.shutdown().await.unwrap();
    let issue = issues::card_on(
      &fixture,
      Arc::new(observation_reader(&fixture, &connection)),
      &runs,
      fixture.store.clone(),
      true,
    )
    .await;
    controls::begin(fixture.store.clone(), downstream.id).await;
    controls::qualify(fixture.store.clone(), &fixture.profile.read.context, downstream.id).await;
    assert_eq!(
      fixture.store.issue(project, issue.id, None).await.unwrap(),
      issue,
      "clarification, retry and completion never move business status"
    );
    let issue = conflict::qualify(&fixture, &profile, &connection, issue, &labels).await;
    let server = ServerRuntime::start(runtime::runtime_config(root.path(), url.as_str(), "http://127.0.0.1:9"))
      .await
      .unwrap();
    let client = reqwest::Client::new();
    let path = format!("/api/v1/projects/{project}/issues/{}", issue.id);
    let origin = format!("http://{}", server.management_addr());
    let before = client.get(format!("{origin}{path}/legacy-work")).send().await.unwrap();
    assert_eq!(before.status(), reqwest::StatusCode::OK);
    let before: serde_json::Value = before.json().await.unwrap();
    assert_eq!(before["items"].as_array().unwrap().len(), 2);
    server.shutdown().await.unwrap();
    let reviews = Arc::new(review_reader::Reader {
      checks: true,
      ..Default::default()
    });
    reviews.revoked.store(true, Ordering::SeqCst);
    let server = ServerRuntime::start_with_issue_integrations(
      runtime::runtime_config(root.path(), url.as_str(), "http://127.0.0.1:9"),
      IssueRuntimeIntegrations {
        sources: None,
        code_reviews: Some(reviews),
      },
    )
    .await
    .unwrap();
    let origin = format!("http://{}", server.management_addr());
    let after: serde_json::Value = client
      .get(format!("{origin}{path}/legacy-work"))
      .send()
      .await
      .unwrap()
      .json()
      .await
      .unwrap();
    assert_eq!(after, before);
    for run in &runs {
      let result = client
        .get(format!("{origin}{path}/work/{}", run.work_id))
        .send()
        .await
        .unwrap();
      assert_eq!(result.status(), reqwest::StatusCode::OK);
      let result: serde_json::Value = result.json().await.unwrap();
      assert_eq!(result["association"]["issue_id"], issue.id.to_string());
      assert_eq!(result["association"]["run_id"], run.id.to_string());
    }
    let expected_runs = fixture.runs().await;
    let resume = fixture.clock.0.load(Ordering::SeqCst) + 2000;
    let native_path = directory.join("mode.json");
    let mut native: serde_json::Value = serde_json::from_slice(&std::fs::read(&native_path).unwrap()).unwrap();
    native["failure"] = serde_json::json!({"class":"transient","retry_after_millis":100});
    std::fs::write(&native_path, serde_json::to_vec(&native).unwrap()).unwrap();
    fixture.clock.0.store(resume, Ordering::SeqCst);
    assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().deferred, 2);
    native.as_object_mut().unwrap().remove("failure");
    std::fs::write(&native_path, serde_json::to_vec(&native).unwrap()).unwrap();
    let input = fixture
      .store
      .source_input(project, downstream_id, WorkSourceInputVersion::INITIAL)
      .await
      .unwrap();
    let (group, _) = overlapping_group(&fixture, &profile, &labels, &input).await;
    fixture.clock.0.store(resume + 2000, Ordering::SeqCst);
    assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().completed, 2);
    let diagnostics = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(handlers(&fixture, &profile, &labels)),
    )
    .handle_query(
      &fixture.profile.read.context,
      GetWorkSourceDiagnosticsQuery {
        project_id: project,
        target: WorkSourceScanTarget::Group {
          id: group.id(),
          version: group.version(),
        },
        after: None,
        limit: 2,
      },
    )
    .await
    .unwrap();
    assert_eq!(diagnostics.scan.unwrap().selection.unwrap().matching_inputs.len(), 2);
    assert_eq!(
      diagnostics.admissions.len(),
      1,
      "the original downstream admission stays visible"
    );
    assert_eq!(diagnostics.admissions[0].association.run_id, downstream.id);
    assert_eq!(fixture.runs().await, expected_runs);
    grant(
      &fixture,
      connection.id,
      Some(connection.access_version),
      vec![],
      "durable-pilot-revoke",
    )
    .await;
    let calls = captured_requests(&directory);
    fixture.clock.0.store(resume + 4000, Ordering::SeqCst);
    assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().deferred, 2);
    assert_eq!(captured_requests(&directory), calls);
    assert_eq!(fixture.runs().await, expected_runs);
    assert_eq!(fixture.store.issue(project, issue.id, None).await.unwrap(), issue);
    let manual = client
      .post(format!("{origin}/api/v1/projects/{project}/issues"))
      .header("idempotency-key", "manual-during-outage")
      .json(&serde_json::json!({"title":"Manual during provider outage", "description":"Independent task"}))
      .send()
      .await
      .unwrap();
    assert_eq!(manual.status(), reqwest::StatusCode::CREATED);
    assert_eq!(fixture.runs().await, expected_runs);
    let browser = console_pilot::run_named(
      &origin,
      Some(&format!("/projects/{project}/issues/{}", issue.id)),
      "imported executions",
    )
    .await;
    assert!(
      browser.status.success(),
      "{}\n{}",
      String::from_utf8_lossy(&browser.stdout),
      String::from_utf8_lossy(&browser.stderr)
    );
    server.shutdown().await.unwrap();
    database.cleanup().await;
  }
}
