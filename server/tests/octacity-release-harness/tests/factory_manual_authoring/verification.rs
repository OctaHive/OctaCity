//! Public retained results and ordinary Build linkage, also checked after restart.
use super::*;
use std::collections::BTreeSet;

pub(super) async fn completed(
  client: &Client,
  origin: &str,
  run: &str,
  public_results: bool,
  code: Option<&code::CodeInstallation>,
) -> Value {
  let state = get_json(client, format!("{origin}/api/v1/factory-runs/{run}")).await;
  assert_eq!(state["state"], "completed");
  let mut retained = serde_json::Map::new();
  for collection in ["node-completions", "calls", "builds", "work"] {
    let page = get_json(
      client,
      format!("{origin}/api/v1/factory-runs/{run}/{collection}?limit=100"),
    )
    .await;
    assert!(page["next_cursor"].is_null());
    retained.insert(collection.into(), page["items"].clone());
  }
  let completions = retained["node-completions"].as_array().unwrap();
  assert_eq!(completions.len(), 4);
  for (node, outcome, result) in [
    (
      "inspect",
      "assessed",
      json!({"eligible":true,"summary":"eligible-public-summary","duplicate_search":"not_requested"}),
    ),
    ("eligibility-check", "eligible", json!("eligible-public-summary")),
    (
      "route",
      "classified",
      json!({"component":"console","priority":"normal"}),
    ),
    (
      "route-check",
      "ready",
      json!({"component":"console","priority":"normal"}),
    ),
  ] {
    let record = &completions
      .iter()
      .find(|row| row["record"]["node_key"] == node)
      .unwrap()["record"];
    assert_eq!(record["outcome"], outcome);
    if public_results {
      assert_eq!(record["public_result"], result);
    } else {
      assert!(
        record["public_result"].is_null(),
        "unpublished result bodies remain private"
      );
    }
    assert_eq!(record["output_digest"].as_str().unwrap().len(), 64);
  }
  let calls = retained["calls"].as_array().unwrap();
  assert_eq!(calls.len(), 2);
  let first = &calls
    .iter()
    .find(|row| row["record"]["node"]["node_key"] == "inspect")
    .unwrap()["record"];
  let second = &calls
    .iter()
    .find(|row| row["record"]["node"]["node_key"] == "route")
    .unwrap()["record"];
  assert_eq!(first["profile"]["model_or_tool"], second["profile"]["model_or_tool"]);
  for field in ["context_id", "context_digest", "task_digest", "selected_profile"] {
    assert!(!first["profile"][field].is_null());
    assert_ne!(
      first["profile"][field], second["profile"][field],
      "independent step {field}"
    );
  }
  assert_ne!(first["node"]["input_digest"], second["node"]["input_digest"]);
  let builds = retained["builds"].as_array().unwrap();
  assert_eq!(builds.len(), 2, "one ordinary Build per reasoning step");
  let mut build_ids = BTreeSet::new();
  let mut job_ids = BTreeSet::new();
  for row in builds {
    let link = &row["record"];
    assert!(build_ids.insert(string(link, "build_id")));
    let call = calls
      .iter()
      .find(|call| call["record"]["node"]["id"] == link["node_attempt_id"])
      .unwrap();
    let build = get_json(client, format!("{origin}/api/v1/builds/{}", string(link, "build_id"))).await;
    assert_eq!(build["state"], "succeeded");
    assert_eq!(
      build["factory_origin"]["input_digest"],
      call["record"]["node"]["input_digest"]
    );
    if let Some(code) = code.filter(|_| call["record"]["node"]["node_key"] == "route") {
      assert_eq!(build["repository_id"], code.repository);
      assert_eq!(build["immutable_revision"], code.revision);
    } else {
      assert_eq!(build["source"], json!({"kind":"source_free_v1"}));
      for field in ["repository_id", "repository_version", "immutable_revision"] {
        assert!(
          build.get(field).is_none(),
          "source-free Build has no fabricated {field}"
        );
      }
    }
    if !public_results {
      let expected = if call["record"]["node"]["node_key"] == "inspect" {
        json!({"eligible":true,"summary":"eligible-public-summary","duplicate_search":"not_requested"})
      } else {
        json!({"component":"console","priority":"normal"})
      };
      let outputs = get_json(
        client,
        format!(
          "{origin}/api/v1/builds/{}/artifacts?limit=100",
          string(link, "build_id")
        ),
      )
      .await;
      let result = outputs["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|output| output["name"] == "codex-run-result")
        .expect("retained ordinary result report");
      let download: Value = client
        .post(format!("{origin}/api/v1/artifacts/{}/download", string(result, "id")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
      let response = client
        .get(string(&download, "get_url"))
        .send()
        .await
        .unwrap_or_else(|_| panic!("private result transfer failed"));
      assert!(response.status().is_success(), "private result transfer rejected");
      let result: Value = response
        .json()
        .await
        .unwrap_or_else(|_| panic!("private result document is invalid"));
      assert_eq!(result["structured_result"], expected);
    }
    let jobs = link["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 1);
    assert!(job_ids.insert(jobs[0].as_str().unwrap()));
  }
  let work = retained["work"].as_array().unwrap();
  assert_eq!(work.len(), 1);
  let subject = &work[0]["record"]["subject"];
  assert_eq!(subject["kind"], "project_input_v1");
  assert_eq!(subject.as_object().unwrap().len(), 3);
  assert_eq!(subject["input_digest"].as_str().unwrap().len(), 64);
  Value::Object(retained)
}
