//! Explicit Project policy initialization after the technical runtime catalog is loaded.
use reqwest::Client;
use serde_json::{Value, json};

pub(super) async fn provision(client: &Client, origin: &str, project: &str, access_alias: &str) -> Value {
  let mut legacy = Vec::new();
  let mut designated = None;
  for (choice_kind, policy_kind) in [("admission_policy", "launch"), ("permission_ceiling", "access")] {
    let mut after: Option<String> = None;
    loop {
      let mut url = reqwest::Url::parse(&format!(
        "{origin}/api/v1/projects/{project}/factory-configuration-choices"
      ))
      .unwrap();
      url.query_pairs_mut().append_pair("kind", choice_kind);
      if let Some(cursor) = &after {
        url.query_pairs_mut().append_pair("after", cursor);
      }
      let page: Value = client
        .get(url)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
      for choice in page["items"].as_array().unwrap() {
        assert_eq!(choice["reference"]["kind"], "immutable");
        let reference = choice["reference"]["reference"].clone();
        if policy_kind == "access" && choice["alias"] == access_alias {
          designated = Some(reference.clone());
        }
        legacy.push(json!({"kind":policy_kind,"reference":reference}));
      }
      after = page["next_cursor"].as_str().map(str::to_owned);
      if after.is_none() {
        break;
      }
    }
  }
  let designated = designated.expect("the manual authoring access ceiling must be installed");
  let response = client
    .post(format!("{origin}/api/v1/projects/{project}/factory-policies/provision"))
    .header("idempotency-key", format!("manual-authoring-policies-{project}"))
    .json(&json!({"designated_access":designated,"legacy_references":legacy}))
    .send()
    .await
    .unwrap();
  let status = response.status();
  let outcome: Value = response.json().await.unwrap();
  assert!(
    status.is_success(),
    "Project policy initialization failed: {status}: {outcome}"
  );
  assert_eq!(
    outcome["unresolved"],
    json!([]),
    "all reviewed installed policy references must resolve"
  );
  outcome
}
