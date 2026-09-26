//! HTTP helpers shared by the released-product acceptance scenarios.

use reqwest::Client;
use serde_json::{Value, json};

pub async fn publish_policy_and_trigger_definition(
  client: &Client,
  origin: &str,
  project: &str,
  repository: &str,
  configuration: &str,
  pool: &str,
  runtime_class: &str,
) -> String {
  post_management(
    client,
    origin,
    &format!("/api/v1/projects/{project}/policy-versions"),
    &format!("policy-{project}"),
    json!({"policy": {
      "pools": {"mode": "replace", "value": [pool]},
      "repositories": {"mode": "replace", "value": [repository]},
      "secret_profiles": {"mode": "replace", "value": []},
      "identity_profiles": {"mode": "replace", "value": []},
      "runtimes": {"mode": "replace", "value": [runtime_class]},
      "cache": {"mode": "replace", "value": {"namespaces": [], "read": false, "write": false, "max_bytes": 0}},
      "artifacts": {"mode": "replace", "value": {
        "artifact_count": 0,
        "artifact_bytes": 0,
        "report_count": 0,
        "report_bytes": 0,
        "single_output_bytes": 0
      }},
      "concurrency": {"mode": "replace", "value": {"active_builds": 1, "active_jobs": 1}},
      "retention": {"mode": "replace", "value": {
        "build_seconds": 86400,
        "log_seconds": 86400,
        "artifact_seconds": 86400,
        "cache_seconds": 86400
      }}
    }}),
  )
  .await;
  let trigger = post_management(
    client,
    origin,
    "/api/v1/trigger-definitions/manual",
    &format!("trigger-definition-{configuration}"),
    json!({
      "configuration_id": configuration,
      "configuration_version": 1,
      "enabled": true,
      "definition": {}
    }),
  )
  .await;
  resource_id(&trigger)
}

pub async fn post_management(client: &Client, origin: &str, path: &str, key: &str, body: Value) -> Value {
  let response = client
    .post(format!("{origin}{path}"))
    .header("idempotency-key", key)
    .json(&body)
    .send()
    .await
    .unwrap();
  let status = response.status();
  let bytes = response.bytes().await.unwrap();
  assert!(
    status.is_success(),
    "{path} returned {status}: {}",
    String::from_utf8_lossy(&bytes)
  );
  serde_json::from_slice(&bytes).unwrap()
}

pub async fn get_json(client: &Client, url: String) -> Value {
  client
    .get(url)
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap()
}

pub fn resource_id(value: &Value) -> String {
  string(&value["resource"], "id")
}

pub fn string(value: &Value, field: &str) -> String {
  value[field].as_str().unwrap().to_owned()
}
