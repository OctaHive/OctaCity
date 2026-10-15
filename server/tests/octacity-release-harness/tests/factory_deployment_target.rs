#![cfg(unix)]
//! A separate durable static-service target, exercised through ordinary HTTP.
#[path = "factory_pilot/deployment_target.rs"]
mod deployment_target;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use deployment_target::DeploymentTarget;
use serde_json::{Value, json};

fn candidate() -> Value {
  json!({"exact":{"project_id":"00000000-0000-0000-0000-000000000001",
    "repository_id":"00000000-0000-0000-0000-000000000002","base_revision":"a".repeat(40)},
    "candidate_revision":"b".repeat(40),"changeset_digest":"6e".repeat(32)})
}
fn request() -> Value {
  json!({"operation_id":"2a".repeat(32),"candidate":candidate(),
    "content_sha256":"0694aacb66d62e742a92e8d5f1e82bd9d2a8ca1be88744201fbe63d0f5007502",
    "content":STANDARD.encode(b"facts")})
}

#[tokio::test]
async fn deployed_candidate_is_actually_served_and_the_same_receipt_survives_target_restart() {
  let directory = tempfile::tempdir().unwrap();
  let client = reqwest::Client::new();
  let target = DeploymentTarget::start(directory.path()).await;
  let response = client.post(target.endpoint()).json(&request()).send().await.unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let receipt: Value = response.json().await.unwrap();
  assert_eq!(receipt["candidate"], candidate());
  assert_eq!(receipt["content_sha256"], request()["content_sha256"]);
  let original_live = client
    .get(target.live_endpoint())
    .send()
    .await
    .unwrap()
    .bytes()
    .await
    .unwrap();
  assert_eq!(original_live.as_ref(), b"facts");
  let identity = receipt["deployment"]["identity"].as_str().unwrap();
  let observed: Value = client
    .get(format!("{}/{identity}", target.endpoint()))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(observed["deployment"], receipt["deployment"]);
  assert_eq!(observed["content"], STANDARD.encode(b"facts"));
  target.stop().await;
  let restarted = DeploymentTarget::start(directory.path()).await;
  let replay: Value = client
    .post(restarted.endpoint())
    .json(&request())
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(replay, receipt, "replay must retain the exact durable target receipt");
  let live = client
    .get(restarted.live_endpoint())
    .send()
    .await
    .unwrap()
    .bytes()
    .await
    .unwrap();
  assert_eq!(live.as_ref(), b"facts");
  restarted.stop().await;
}

#[tokio::test]
async fn target_rejects_substitution_and_keeps_the_deployed_bytes_and_receipt() {
  let directory = tempfile::tempdir().unwrap();
  let target = DeploymentTarget::start(directory.path()).await;
  let client = reqwest::Client::new();
  assert_eq!(
    client.get(target.live_endpoint()).send().await.unwrap().status(),
    reqwest::StatusCode::NOT_FOUND
  );
  let original: Value = client
    .post(target.endpoint())
    .json(&request())
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  for (field, replacement, status) in [
    (
      "content_sha256",
      json!("ff".repeat(32)),
      reqwest::StatusCode::BAD_REQUEST,
    ),
    ("operation_id", json!("3b".repeat(32)), reqwest::StatusCode::CONFLICT),
    (
      "candidate",
      {
        let mut changed = candidate();
        changed["candidate_revision"] = json!("c".repeat(40));
        changed
      },
      reqwest::StatusCode::CONFLICT,
    ),
  ] {
    let mut substituted = request();
    substituted[field] = replacement;
    assert_eq!(
      client
        .post(target.endpoint())
        .json(&substituted)
        .send()
        .await
        .unwrap()
        .status(),
      status
    );
    let live = client
      .get(target.live_endpoint())
      .send()
      .await
      .unwrap()
      .bytes()
      .await
      .unwrap();
    assert_eq!(live.as_ref(), b"facts");
    let retained: Value = client
      .post(target.endpoint())
      .json(&request())
      .send()
      .await
      .unwrap()
      .json()
      .await
      .unwrap();
    assert_eq!(retained, original);
  }
  target.stop().await;
}
