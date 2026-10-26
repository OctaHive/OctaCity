//! Retained Issue effects recover through ordinary HTTP and a fresh normal server startup.
use super::*;

async fn review_ready(p: &IssueProject) -> (Value, Value) {
  let path = format!("{}/issues/{}", p.base, p.issue["id"].as_str().unwrap());
  let choices = get(&p.client, &format!("{path}/source-choices")).await;
  let workflow = get(
    &p.client,
    &format!(
      "{}/issue-workflows/{}/versions/{}",
      p.base,
      p.issue["workflow_id"].as_str().unwrap(),
      p.issue["workflow_version"]
    ),
  )
  .await;
  let edge = workflow["definition"]["transitions"][0].clone();
  let response = p
    .client
    .post(format!("{path}/source-link/mappings"))
    .header("idempotency-key", "review-recovery")
    .header("if-match", format!("\"{}\"", p.issue["version"].as_u64().unwrap()))
    .json(&json!({"expected_mapping":null,"definition":{
      "source":p.source["binding"],"reporter":choices["items"][0]["writer"]["binding"],
      "push_enabled":true,"fields":{"status":"octacity_owned_push"},"status":{
        "workflow":{"id":p.issue["workflow_id"],"version":p.issue["workflow_version"]},"pull":[],
        "push":{"statuses":[{"status":edge["to"],"add":["custom-ready"],"remove":[],"state":null}]}
      }
    }}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::CREATED,
    "{}",
    response.text().await.unwrap()
  );
  (edge, response.json().await.unwrap())
}

async fn projection(client: &reqwest::Client, path: &str, state: &str, attempt: u64) -> Value {
  let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
  loop {
    let page = get(client, &format!("{path}/synchronization/projections")).await;
    assert!(
      page["items"].as_array().unwrap().len() <= 1,
      "one move retains one logical effect"
    );
    let row = &page["items"][0];
    if row["state"] == state && row["attempt"] == attempt {
      return row.clone();
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "expected {state}/{attempt}, observed {page}"
    );
    tokio::time::sleep(Duration::from_millis(10)).await;
  }
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn normal_issue_reporter_restart_preserves_original_effects_and_current_writer_authority() {
  for revoked in [false, true] {
    let p = start_issue_project_with_retry(3000).await;
    let (edge, mapping) = review_ready(&p).await;
    let IssueProject {
      fixture,
      database,
      deployment: _deployment,
      config,
      runtime,
      client,
      base,
      issue,
      ..
    } = p;
    let directory = fixture.root.path().join("opaque-writer");
    std::fs::write(
      directory.join("reporter-mode.json"),
      br#"{"lose_project_response":true,"source_effects":true}"#,
    )
    .unwrap();
    let path = format!("{base}/issues/{}", issue["id"].as_str().unwrap());
    let accepted = transition(&client, &path, &issue, &edge, "accepted-before-lost-response").await;
    let pending = projection(&client, &path, "pending", 1).await;
    assert_eq!(pending["mapping"], mapping["mapping_reference"]);
    assert!(directory.join("reporter-applied").exists());
    assert_eq!(get(&client, &path).await["status_id"], accepted["status_id"]);
    if revoked {
      let connection = fixture.input.definition().connection.as_ref().unwrap();
      let response = client
        .post(format!(
          "http://localhost/api/v1/integrations/{}/projects/{}/access",
          connection.id,
          issue["project_id"].as_str().unwrap()
        ))
        .header("idempotency-key", "revoke-pending-writer")
        .header("if-match", "\"1\"")
        .json(
          &json!({"intent":{"connection_id":connection.id,"connection_version":connection.version,
          "project_id":issue["project_id"],"expected_current_version":1,"resources":[{
            "reference":fixture.input.definition().source.resource,"purposes":["task_read"]
          }]}}),
        )
        .send()
        .await
        .unwrap();
      assert_eq!(
        response.status(),
        reqwest::StatusCode::OK,
        "{}",
        response.text().await.unwrap()
      );
    } else {
      let mut definition = mapping["resource"]["definition"].clone();
      definition["status"]["push"]["statuses"][0]["add"] = json!([]);
      definition["status"]["push"]["statuses"][0]["remove"] = json!(["custom-ready"]);
      let response = client
        .post(format!("{path}/source-link/mappings"))
        .header("idempotency-key", "review-after-uncertain-write")
        .header("if-match", format!("\"{}\"", accepted["version"].as_u64().unwrap()))
        .json(&json!({"expected_mapping":mapping["mapping_reference"],"definition":definition}))
        .send()
        .await
        .unwrap();
      assert_eq!(
        response.status(),
        reqwest::StatusCode::CREATED,
        "{}",
        response.text().await.unwrap()
      );
    }
    runtime.shutdown().await.unwrap();
    let captured = captured_requests(&directory);
    let calls: Vec<Value> = captured
      .lines()
      .map(|line| serde_json::from_str(line).unwrap())
      .collect();
    let operations: Vec<_> = calls
      .iter()
      .map(|call| call["message"]["command"]["operation"].as_str().unwrap())
      .collect();
    assert_eq!(
      operations,
      ["inspect", "inspect", "apply"],
      "first runtime must stop before recovery"
    );
    let original = fixture
      .store
      .issue_push_projection(pending["operation_id"].as_str().unwrap().parse().unwrap())
      .await
      .unwrap();
    assert_eq!(original.record.state, FactoryOutboxState::Pending);
    assert_eq!(original.record.attempt, 1);
    std::fs::write(directory.join("reporter-mode.json"), br#"{"source_effects":true}"#).unwrap();
    let runtime = crate::ServerRuntime::start(config).await.unwrap();
    let delivered = projection(&client, &path, if revoked { "failed" } else { "delivered" }, 1).await;
    for field in [
      "operation_id",
      "mapping",
      "native_correlation",
      "workflow",
      "effects",
      "accepted_at",
      "status_id",
      "issue_version",
    ] {
      assert_eq!(delivered[field], pending[field], "restart must preserve {field}");
    }
    let after = captured_requests(&directory);
    let calls: Vec<Value> = after.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    let operations: Vec<_> = calls
      .iter()
      .map(|call| call["message"]["command"]["operation"].as_str().unwrap())
      .collect();
    if revoked {
      assert_eq!(delivered["failure"], "forbidden");
      assert_eq!(
        operations,
        ["inspect", "inspect", "apply"],
        "revoked writer cannot inspect or reuse a fallback"
      );
    } else {
      assert_eq!(operations, ["inspect", "inspect", "apply", "inspect", "inspect"]);
    }
    for call in &calls {
      let intent = original_intent(call);
      assert_eq!(intent, original_intent(&calls[0]));
      assert_eq!(intent["projection_id"], delivered["operation_id"]);
      assert_eq!(intent["item_id"], "TASK-native");
    }
    let native: Value = serde_json::from_slice(&std::fs::read(directory.join("mode.json")).unwrap()).unwrap();
    assert_eq!(native["candidates"][0]["labels"], json!(["arbitrary", "custom-ready"]));
    let retained = get(&client, &path).await;
    assert_eq!(retained["version"], accepted["version"]);
    assert_eq!(retained["status_id"], accepted["status_id"]);
    assert_eq!(get(&client, &format!("{base}/factory-runs")).await["items"], json!([]));
    runtime.shutdown().await.unwrap();
    database.cleanup().await;
  }
}
