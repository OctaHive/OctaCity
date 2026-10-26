//! Actual managed Factory polling over the same reviewed predicates as Project import.
use serde_json::{Value, json};

pub(super) async fn assert_nonmatching_inputs_checkpoint_without_execution(
  client: &reqwest::Client,
  project: &str,
  published: &Value,
  provider: &std::path::Path,
) {
  let base = format!("http://localhost/api/v1/projects/{project}");
  let before: Value = client
    .get(format!("{base}/factory-runs"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  let mode_path = provider.join("mode.json");
  let original = std::fs::read(&mode_path).unwrap();
  let cases = [
    (
      json!({"state":"native-state","all_of":["absent-required-label"],"any_of":[],"none_of":[]}),
      "native-state",
    ),
    (
      json!({"state":"native-state","all_of":[],"any_of":["absent-alternative"],"none_of":[]}),
      "native-state",
    ),
    (
      json!({"state":"native-state","all_of":[],"any_of":[],"none_of":["arbitrary"]}),
      "native-state",
    ),
    (
      json!({"state":"native-state","all_of":[],"any_of":[],"none_of":[]}),
      "different-native-state",
    ),
  ];
  for (index, (predicate, state)) in cases.into_iter().enumerate() {
    let mut mode: Value = serde_json::from_slice(&original).unwrap();
    mode["candidates"][0]["state"] = json!(state);
    mode["candidates"][0]["body"] = json!("Valid reviewed acceptance for a filtered candidate");
    std::fs::write(&mode_path, serde_json::to_vec(&mode).unwrap()).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let mut intent = published["intent"].clone();
    intent["inputs"][0]["id"] = json!(id);
    intent["inputs"][0]["definition"]["enabled"] = json!(true);
    intent["inputs"][0]["definition"]["predicate"] = predicate;
    let path = format!("{base}/factory-source-inputs/{id}/versions");
    let response = client
      .post(&path)
      .header("idempotency-key", format!("qualify-factory-filter.{index}"))
      .header("if-none-match", "*")
      .json(&json!({"intent":intent}))
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
      let response = client.get(format!("{path}/1/diagnostics")).send().await.unwrap();
      assert_eq!(response.status(), reqwest::StatusCode::OK);
      let diagnostics: Value = response.json().await.unwrap();
      // The initial page checkpoint precedes candidate settlement; wait for the completed pass.
      if !diagnostics["scan"]["selection"].is_null() && !diagnostics["scan"]["schedule"].is_null() {
        assert!(!diagnostics["scan"]["window"].is_null());
        assert!(
          diagnostics["scan"]["schedule"]["failure"].is_null(),
          "filtering is successful observation: {diagnostics}"
        );
        assert_eq!(diagnostics["scan"]["selection"]["matching_inputs"], json!([]));
        assert_eq!(diagnostics["admissions"], json!([]));
        break;
      }
      assert!(
        tokio::time::Instant::now() < deadline,
        "enabled input must checkpoint its filtered observation: {diagnostics}"
      );
      tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    intent["inputs"][0]["expected_version"] = json!(1);
    intent["inputs"][0]["definition"]["enabled"] = json!(false);
    let response = client
      .post(&path)
      .header("idempotency-key", format!("stop-factory-filter.{index}"))
      .header("if-match", "\"1\"")
      .json(&json!({"intent":intent}))
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
  }
  let after: Value = client
    .get(format!("{base}/factory-runs"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(
    after, before,
    "filtered native candidates cannot create or modify execution"
  );
  assert!(!provider.join("reporter-requests.jsonl").exists());
  std::fs::write(mode_path, original).unwrap();
}
