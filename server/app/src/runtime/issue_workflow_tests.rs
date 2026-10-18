//! Released composition exposes provider-free workflow administration over PostgreSQL.
use super::*;
use super::{postgres_test_support as postgres, test_runtime_config as config};
use serde_json::{Value, json};

#[tokio::test]
#[ignore = "requires OCTACITY_POSTGRES_URL pointing to disposable PostgreSQL"]
async fn runtime_issue_workflows_are_available_without_factory_or_integrations() {
  let database = postgres::TestDatabase::migrated().await;
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let root = tempfile::tempdir().unwrap();
  let runtime = ServerRuntime::start(config::runtime_config(root.path(), url.as_str(), "http://127.0.0.1:9"))
    .await
    .unwrap();
  let client = reqwest::Client::new();
  let origin = format!("http://{}", runtime.management_addr());
  let response = client
    .post(format!("{origin}/api/v1/projects"))
    .header("idempotency-key", "workflow-project")
    .json(&json!({"name":"Local tasks"}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::CREATED);
  let project: Value = response.json().await.unwrap();
  let id = project["resource"]["id"].as_str().unwrap();
  let response = client
    .get(format!("{origin}/api/v1/projects/{id}/issue-workflow-default"))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  assert_eq!(response.headers()["etag"], "\"1\"");
  let default: Value = response.json().await.unwrap();
  assert_eq!(default["workflow"]["project_id"], id);
  assert_eq!(
    default["workflow"]["definition"]["initial_status_id"],
    "10000000-0000-4000-8000-000000000001"
  );
  let workflow_id = "20000000-0000-4000-8000-000000000001";
  let mut definition = default["workflow"]["definition"].clone();
  definition["name"] = json!("Team policy");
  let response = client
    .post(format!("{origin}/api/v1/projects/{id}/issue-workflows"))
    .header("idempotency-key", "create-team-policy")
    .json(&json!({"id":workflow_id,"definition":definition}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::CREATED);
  assert_eq!(response.headers()["etag"], "\"1\"");
  let created: Value = response.json().await.unwrap();
  assert_eq!(created["resource"]["id"], workflow_id);
  assert_eq!(created["resource"]["definition"], definition);
  for suffix in [workflow_id.to_owned(), format!("{workflow_id}/versions/1")] {
    let response = client
      .get(format!("{origin}/api/v1/projects/{id}/issue-workflows/{suffix}"))
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(response.headers()["etag"], "\"1\"");
    assert_eq!(response.json::<Value>().await.unwrap(), created["resource"]);
  }
  let mut replacement = definition.clone();
  replacement["name"] = json!("Team policy revised");
  replacement["statuses"][0]["name"] = json!("Queued");
  replacement["statuses"].as_array_mut().unwrap().reverse();
  replacement["terminal_status_ids"] = json!([]);
  let versions = format!("{origin}/api/v1/projects/{id}/issue-workflows/{workflow_id}/versions");
  let response = client
    .post(&versions)
    .header("idempotency-key", "replace-team-policy")
    .header("if-match", "\"1\"")
    .json(&json!({"definition":replacement}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  assert_eq!(response.headers()["etag"], "\"2\"");
  let published: Value = response.json().await.unwrap();
  assert_eq!(published["resource"]["definition"], replacement);
  let current: Value = client
    .get(format!("{origin}/api/v1/projects/{id}/issue-workflows/{workflow_id}"))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(current, published["resource"]);
  let retained: Value = client
    .get(format!("{versions}/1"))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(retained, created["resource"]);
  let default_url = format!("{origin}/api/v1/projects/{id}/issue-workflow-default");
  let unchanged: Value = client.get(&default_url).send().await.unwrap().json().await.unwrap();
  assert_eq!(unchanged, default);
  let selection = json!({"workflow_id":workflow_id,"workflow_version":1});
  let response = client
    .post(&default_url)
    .header("idempotency-key", "select-team-policy")
    .header("if-match", "\"1\"")
    .json(&selection)
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  assert_eq!(response.headers()["etag"], "\"2\"");
  let selected: Value = response.json().await.unwrap();
  assert_eq!(selected["resource"]["version"], 2);
  assert_eq!(selected["resource"]["workflow"], created["resource"]);
  let response = client
    .post(&default_url)
    .header("idempotency-key", "select-latest-policy")
    .header("if-match", "\"2\"")
    .json(&json!({"workflow_id":workflow_id,"workflow_version":2}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  for (url, key, expected, body, original) in [
    (
      &versions,
      "replace-team-policy",
      "\"1\"",
      json!({"definition":replacement}),
      &published,
    ),
    (
      &default_url,
      "select-team-policy",
      "\"1\"",
      selection.clone(),
      &selected,
    ),
  ] {
    let response = client
      .post(url)
      .header("idempotency-key", key)
      .header("if-match", expected)
      .json(&body)
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let replay: Value = response.json().await.unwrap();
    assert_eq!(replay["disposition"], "replayed");
    assert_eq!(replay["resource"], original["resource"]);
  }
  let response = client
    .post(&default_url)
    .header("idempotency-key", "stale-selection")
    .header("if-match", "\"1\"")
    .json(&selection)
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
  let list_url = format!("{origin}/api/v1/projects/{id}/issue-workflows?limit=1");
  let first: Value = client
    .get(&list_url)
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(first["items"].as_array().unwrap().len(), 1);
  let after = first["next_cursor"].as_str().unwrap();
  let second: Value = client
    .get(format!("{list_url}&after={after}"))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(second["items"].as_array().unwrap().len(), 1);
  assert!(second["next_cursor"].is_null());
  let mut actual = [
    first["items"][0]["id"].as_str().unwrap(),
    second["items"][0]["id"].as_str().unwrap(),
  ];
  actual.sort();
  let mut expected = [id, workflow_id];
  expected.sort();
  assert_eq!(actual, expected);
  runtime.shutdown().await.unwrap();
  database.cleanup().await;
}
