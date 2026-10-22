//! Actual ordinary Build/Job events are read through the Issue HTTP journal after restart.
use super::*;
use serde_json::{Value, json};
#[allow(dead_code)]
#[path = "../support/console_pilot.rs"]
mod console_pilot;
use super::runtime_fixture as runtime;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and locked browser dependencies"]
async fn original_issue_job_events_resume_after_restart_without_private_payloads() {
  let (database, store, request) = prepared_factory_build(true).await;
  let original_run = request.intent.input().factory_run_id();
  let original = store.factory_run_snapshot(original_run).await.unwrap();
  let compiler = Arc::new(PreparedBuildCompiler {
    prepared: request.clone(),
    calls: std::sync::atomic::AtomicUsize::new(0),
  });
  let builds = Arc::new(octacity_server_store_postgres::PostgresAuthoritativeStore::new(
    database.pool.clone(),
    postgres_support::test_signer(),
  ));
  let ordinary = Arc::new(FactoryOrdinaryBuildService::new(
    builds.clone(),
    store.clone(),
    compiler,
    Arc::new(BuildClock),
  ));
  FactoryNodeRunner::new(
    store.clone(),
    Arc::new(FactoryNodeBuildAdapter::new(
      ordinary,
      Arc::new(factory_node_test_support::Outputs(None)),
    )),
    factory_node_test_support::clock(),
  )
  .run_once(
    original_run,
    original.current_claim.as_ref().unwrap().id,
    FactoryNodeTarget {
      flow_run_id: request.intent.input().flow_run_id(),
      cycle_id: request.intent.input().cycle_id(),
      node: request.intent.input().node().clone(),
    },
    None,
    at(12),
  )
  .await
  .unwrap();
  let snapshot = store.factory_run_snapshot(original_run).await.unwrap();
  let execution = &snapshot.flow.data.build_executions[0];
  let job = &request.jobs[0];
  let lease = retained_job_assignment(&database.pool, job.id, job.allowed_pools[0]).await;
  let event = |sequence| {
    DurableJobEvent::new(EventSequence::new(sequence).unwrap(), JobEventKind::new("progress").unwrap(), at(100 + sequence as i64), json!({"source":"runner","event":{
    "schema_version":4,"sequence":sequence+10,"timestamp":"2026-10-21T00:00:00Z","category":"execution",
    "data":{"type":"progress","run_id":2,"step_id":9,"command_id":"command-1", "progress":{"message":"private transcript", "observation":{
      "operation_id":"ab".repeat(32),"phase":"completed","observed_at_unix_ms":77,"operation":{"kind":"command","exit_code":17,"failed":true}
    }},"arguments":"private arguments","output":"private output"}
  }})).unwrap()
  };
  builds
    .append_job_events(AppendJobEvents::new(lease, vec![event(1)], at(101)).unwrap())
    .await
    .unwrap();
  let project = original.work.subject().project_id();
  let issue = store
    .create_issue(octacity_server_store::testing::management_mutation(CreateIssue {
      id: IssueId::generate(),
      assignees: Default::default(),
      intent: CreateIssueIntent {
        project_id: project,
        content: IssueContent {
          title: "Original Job journal".into(),
          description: "Manual task".into(),
          acceptance: None,
          priority: None,
        },
        planning: Default::default(),
      },
      idempotency_key: "journal-card".parse().unwrap(),
      created_at: at(100),
    }))
    .await
    .unwrap()
    .issue;
  store
    .associate_issue_work(octacity_server_store::testing::management_mutation(
      AssociateIssueWork {
        project_id: project,
        issue_id: issue.id,
        request_id: None,
        admission: PublishedFactoryAdmission {
          work: original.work.clone(),
          run: FactoryRun::admitted(original_run, &original.work),
          flow: original.admitted_flow.clone(),
          admitted_at: at(1),
        },
      },
    ))
    .await
    .unwrap();
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let root = tempfile::tempdir().unwrap();
  let server =
    octacity_server::ServerRuntime::start(runtime::runtime_config(root.path(), url.as_str(), "http://127.0.0.1:9"))
      .await
      .unwrap();
  let path = format!(
    "/api/v1/projects/{project}/issues/{}/work/{}/journal/events",
    issue.id,
    original.work.id()
  );
  let selection = format!(
    "build_execution={}&job_id={}&limit=1",
    execution.digest().unwrap(),
    job.id
  );
  let client = reqwest::Client::new();
  let first = client
    .get(format!("http://{}{path}?{selection}", server.management_addr()))
    .send()
    .await
    .unwrap();
  assert_eq!(first.status(), reqwest::StatusCode::OK);
  let first: Value = first.json().await.unwrap();
  assert_eq!(first["events"][0]["facts"]["observation"]["operation"]["exit_code"], 17);
  assert!(!first.to_string().contains("private"));
  let cursor = first["cursor"].as_str().unwrap();
  server.shutdown().await.unwrap();
  builds
    .append_job_events(AppendJobEvents::new(lease, vec![event(2)], at(102)).unwrap())
    .await
    .unwrap();
  let server =
    octacity_server::ServerRuntime::start(runtime::runtime_config(root.path(), url.as_str(), "http://127.0.0.1:9"))
      .await
      .unwrap();
  let origin = format!("http://{}", server.management_addr());
  let next = client
    .get(format!("{origin}{path}?{selection}&after={cursor}"))
    .send()
    .await
    .unwrap();
  assert_eq!(next.status(), reqwest::StatusCode::OK);
  let next: Value = next.json().await.unwrap();
  assert_eq!(next["events"][0]["sequence"], 2);
  assert!(!next.to_string().contains("private"));
  let detail = client
    .get(format!(
      "{origin}{path}/1?build_execution={}&job_id={}",
      execution.digest().unwrap(),
      job.id
    ))
    .send()
    .await
    .unwrap();
  assert_eq!(detail.status(), reqwest::StatusCode::OK);
  let detail: Value = detail.json().await.unwrap();
  assert!(
    detail.to_string().contains("private transcript"),
    "raw bytes are only available through the explicit authorized log reader"
  );
  assert_eq!(store.issue(project, issue.id, None).await.unwrap(), issue);
  let browser = console_pilot::run_named(
    &origin,
    Some(&format!(
      "/projects/{project}/issues/{}?section=runs&work={}&journal_build={}&journal_job={}",
      issue.id,
      original.work.id(),
      execution.digest().unwrap(),
      job.id
    )),
    "qualified Job journal",
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
