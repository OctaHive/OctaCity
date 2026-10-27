//! A later read-only Project grant selects a different native task container and code Repository live.
use super::*;

pub(super) async fn run(environment: &source_environment::SourceEnvironment, project: &str, resource: &Value) -> Value {
  let mut candidate = source_provider::issue(9901, 31);
  candidate["title"] = json!("Research triage outside code repository");
  candidate["body"] =
    json!("Research requirements\n\n## Acceptance\nRetain research identity and independent code selection.");
  environment.provider.resources.lock().await.insert(
    "research/backlog".into(),
    source_provider::NativeResource {
      metadata: json!({"id":5678}),
      issues: vec![candidate],
    },
  );
  let connection = source_setup::enroll_resource(
    &environment.client,
    &environment.origin,
    &environment.installation,
    project,
    resource,
    false,
  )
  .await;
  let base = format!("{}/api/v1/projects/{project}", environment.origin);
  let choices = get_json(
    &environment.client,
    format!("{base}/issue-source-choices?connection_id={connection}&connection_version=1"),
  )
  .await;
  assert_eq!(
    choices["items"].as_array().unwrap().len(),
    1,
    "this grant supplies only its own read-only resource"
  );
  let import = Uuid::new_v4();
  let import_path = format!("/api/v1/projects/{project}/issue-source-imports/{import}/versions");
  let mut import_intent = json!({"id":import,"project_id":project,"expected_current_version":null,"definition":{
    "source":choices["items"][0]["binding"],"enabled":true,"connection":{"id":connection,"version":1,
      "access_version":1,"source_configuration":choices["items"][0]["connection_configuration"],"reporter_configuration":null}
  }});
  source_setup::publish(
    &environment.client,
    &environment.origin,
    &import_path,
    &import.to_string(),
    "if-none-match",
    "*",
    json!({"intent":import_intent}),
  )
  .await;
  let deadline = Instant::now() + Duration::from_secs(20);
  let issue = loop {
    let issues = get_json(&environment.client, format!("{base}/issues")).await;
    if issues["items"].as_array().unwrap().len() == 1 {
      break issues["items"][0]["id"].clone();
    }
    assert!(
      Instant::now() < deadline,
      "a later native resource grant must import without Server restart: {issues}"
    );
    sleep(Duration::from_millis(100)).await;
  };
  let (target, repository) = source_factory_configuration::publish(environment, project, None).await;
  let choices = get_json(
    &environment.client,
    format!("{base}/factory-source-choices?connection_id={connection}&connection_version=1"),
  )
  .await;
  assert_eq!(choices["items"].as_array().unwrap().len(), 1);
  let id = Uuid::new_v4();
  let path = format!("/api/v1/projects/{project}/factory-source-inputs/{id}/versions");
  let mut intent = json!({"project_id":project,"group":null,"inputs":[{"id":id,"expected_version":null,"definition":{
    "target":target,"repository":repository,"source":choices["items"][0]["binding"],"reporter":null,"enabled":true,
    "connection":{"id":connection,"version":1,"access_version":1,
      "source_configuration":choices["items"][0]["connection_configuration"],"reporter_configuration":null}
  }}]});
  source_setup::publish(
    &environment.client,
    &environment.origin,
    &path,
    &id.to_string(),
    "if-none-match",
    "*",
    json!({"intent":intent}),
  )
  .await;
  let deadline = Instant::now() + Duration::from_secs(20);
  let admission = loop {
    let progress = get_json(
      &environment.client,
      format!("{}{path}/1/diagnostics", environment.origin),
    )
    .await;
    if progress["admissions"].as_array().unwrap().len() == 1 && progress["admissions"][0]["issue"]["issue_id"] == issue
    {
      break progress["admissions"][0].clone();
    }
    assert!(
      Instant::now() < deadline,
      "a later Project must admit against its own code Repository live: {progress}"
    );
    sleep(Duration::from_millis(100)).await;
  };
  let run = admission["association"]["run_id"].as_str().unwrap();
  let work = get_json(
    &environment.client,
    format!("{}/api/v1/factory-runs/{run}/work", environment.origin),
  )
  .await;
  assert_eq!(work["items"][0]["record"]["subject"]["project_id"], project);
  assert_eq!(work["items"][0]["record"]["subject"]["repository_id"], repository["id"]);
  assert_eq!(
    admission["reporting"],
    json!([]),
    "read-only execution has no writer requirement"
  );
  intent["inputs"][0]["expected_version"] = json!(1);
  intent["inputs"][0]["definition"]["enabled"] = json!(false);
  source_setup::publish(
    &environment.client,
    &environment.origin,
    &path,
    &format!("{id}-disable"),
    "if-match",
    "\"1\"",
    json!({"intent":intent}),
  )
  .await;
  import_intent["expected_current_version"] = json!(1);
  import_intent["definition"]["enabled"] = json!(false);
  source_setup::publish(
    &environment.client,
    &environment.origin,
    &import_path,
    &format!("{import}-disable"),
    "if-match",
    "\"1\"",
    json!({"intent":import_intent}),
  )
  .await;
  json!({"project_id":project,"native_resource":resource["native_id"],"code_repository":repository,"admission":admission})
}
