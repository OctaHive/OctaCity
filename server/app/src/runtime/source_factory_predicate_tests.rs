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
      json!({"state":"native-state","all_of":[],"any_of":[],"none_of":[],"label_presence":"none"}),
      "native-state",
    ),
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
    let diagnostics = wait_for_selection(client, &format!("{path}/1/diagnostics")).await;
    assert_eq!(diagnostics["scan"]["selection"]["matching_inputs"], json!([]));
    assert_eq!(diagnostics["admissions"], json!([]));
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

pub(super) async fn assert_exclusive_groups_do_not_choose_an_ambiguous_member(
  client: &reqwest::Client,
  project: &str,
  published: &Value,
  provider: &std::path::Path,
) {
  let base = format!("http://localhost/api/v1/projects/{project}");
  let mode_path = provider.join("mode.json");
  let original = std::fs::read(&mode_path).unwrap();
  let mut mode: Value = serde_json::from_slice(&original).unwrap();
  mode["candidates"][0]["body"] = json!("Valid acceptance cannot break an exclusive selection tie");
  std::fs::write(&mode_path, serde_json::to_vec(&mode).unwrap()).unwrap();
  let before: Value = client
    .get(format!("{base}/factory-runs"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  for (index, (expected_matches, strict)) in [(0, false), (0, true), (2, true)].into_iter().enumerate() {
    mode["candidates"][0]["labels"] = if expected_matches == 0 {
      json!(["arbitrary"])
    } else {
      json!([])
    };
    std::fs::write(&mode_path, serde_json::to_vec(&mode).unwrap()).unwrap();
    let mut intent = published["intent"].clone();
    let mut first = intent["inputs"][0].clone();
    let mut second = first.clone();
    first["id"] = json!(uuid::Uuid::new_v4().to_string());
    second["id"] = json!(uuid::Uuid::new_v4().to_string());
    for input in [&mut first, &mut second] {
      input["definition"]["enabled"] = json!(true);
      input["definition"]["predicate"] = json!({"state":"native-state","all_of":[],"any_of":[],
        "none_of":if strict {vec![]} else {vec!["arbitrary"]},
        "label_presence":if strict {"none"} else {"any"}});
    }
    let group = uuid::Uuid::new_v4().to_string();
    intent["inputs"] = json!([first, second]);
    intent["group"] = json!({"id":group,"expected_version":null,"members":[
      {"id":first["id"],"version":1},{"id":second["id"],"version":1}]});
    let path = format!("{base}/factory-source-groups/{group}/versions");
    let response = client
      .post(&path)
      .header("idempotency-key", format!("qualify-exclusive-group.{index}"))
      .header("if-none-match", "*")
      .json(&json!({"intent":intent}))
      .send()
      .await
      .unwrap();
    assert_eq!(
      response.status(),
      reqwest::StatusCode::OK,
      "{}",
      response.text().await.unwrap()
    );
    let diagnostics = wait_for_selection(client, &format!("{path}/1/diagnostics")).await;
    let matching = diagnostics["scan"]["selection"]["matching_inputs"].as_array().unwrap();
    assert_eq!(
      matching.len(),
      expected_matches,
      "zero is filtered; two is ambiguous: {diagnostics}"
    );
    if expected_matches == 2 {
      assert!(matching.contains(&first["id"]) && matching.contains(&second["id"]));
    }
    assert_eq!(
      diagnostics["admissions"],
      json!([]),
      "exclusive groups cannot pick an arbitrary member"
    );
    for input in intent["inputs"].as_array_mut().unwrap() {
      input["expected_version"] = json!(1);
      input["definition"]["enabled"] = json!(false);
    }
    intent["group"]["expected_version"] = json!(1);
    for member in intent["group"]["members"].as_array_mut().unwrap() {
      member["version"] = json!(2);
    }
    let response = client
      .post(&path)
      .header("idempotency-key", format!("stop-exclusive-group.{index}"))
      .header("if-match", "\"1\"")
      .json(&json!({"intent":intent}))
      .send()
      .await
      .unwrap();
    assert_eq!(
      response.status(),
      reqwest::StatusCode::OK,
      "{}",
      response.text().await.unwrap()
    );
  }
  let after: Value = client
    .get(format!("{base}/factory-runs"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(after, before);
  assert!(!provider.join("reporter-requests.jsonl").exists());
  std::fs::write(mode_path, original).unwrap();
}

pub(super) async fn wait_for_selection(client: &reqwest::Client, path: &str) -> Value {
  let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
  loop {
    let response = client.get(path).send().await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let diagnostics: Value = response.json().await.unwrap();
    // The initial page checkpoint precedes candidate settlement; wait for the completed pass.
    if !diagnostics["scan"]["selection"].is_null() && !diagnostics["scan"]["schedule"].is_null() {
      assert!(!diagnostics["scan"]["window"].is_null());
      assert!(
        diagnostics["scan"]["schedule"]["failure"].is_null(),
        "selection is successful observation: {diagnostics}"
      );
      return diagnostics;
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "enabled target must settle its observation: {diagnostics}"
    );
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
  }
}
