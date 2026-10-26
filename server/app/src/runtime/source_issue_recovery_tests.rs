//! HTTP authoring, controlled public owners and normal-startup recovery of retained Issue effects.
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
    let p = start_issue_project().await;
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
    // Stop supervised dispatch before acceptance. The same public command and one
    // explicit outbox pass leave a retry pending without racing another live pass.
    runtime.shutdown().await.unwrap();
    let installed = crate::runtime::integration_management::install(
      &config,
      fixture.store.clone(),
      CancellationToken::new(),
      &mut crate::runtime::tests::healthy_checks(),
    )
    .await
    .unwrap()
    .unwrap();
    let sources = installed.source_management.unwrap();
    let managed_issues = installed.issues.unwrap();
    let context = crate::runtime::integration_management::request_context("inventory-test");
    let policy = crate::runtime::integration_management::management_policy(Some("inventory-test"));
    let project_id = serde_json::from_value(issue["project_id"].clone()).unwrap();
    let issue_id = serde_json::from_value(issue["id"].clone()).unwrap();
    let at = SystemFactoryExecutionClock.now().unwrap().unix_millis();
    fixture.clock.0.store(at, Ordering::SeqCst);
    let accepted = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(fixture.store.clone(), policy.clone())),
    )
    .handle_command(
      &context,
      TransitionIssueCommand {
        intent: TransitionIssueIntent {
          project_id,
          id: issue_id,
          expected_version: serde_json::from_value(issue["version"].clone()).unwrap(),
          workflow_id: serde_json::from_value(issue["workflow_id"].clone()).unwrap(),
          workflow_version: serde_json::from_value(issue["workflow_version"].clone()).unwrap(),
          expected_status_id: serde_json::from_value(issue["status_id"].clone()).unwrap(),
          transition_id: serde_json::from_value(edge["id"].clone()).unwrap(),
          reason: None,
        },
        idempotency_key: IdempotencyKey::new("accepted-before-lost-response").unwrap(),
        moved_at: time(at),
      },
    )
    .await
    .unwrap()
    .issue;
    let projector = Arc::new(sources.reporter_projector(project_id, fixture.clock.clone()).unwrap());
    let first = reporting_worker(&fixture, projector, "controlled-issue-reporter", 10000);
    fixture.clock.0.store(at + 10, Ordering::SeqCst);
    assert_eq!(first.run_once(time(at + 10)).await.unwrap().retrying, 1);
    drop(first);
    let page = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueSynchronizationHandlers::new(
        fixture.store.clone(),
        fixture.clock.clone(),
      )),
    )
    .handle_query(
      &context,
      ListIssueSourceProjectionsQuery::try_new(project_id, issue_id, None, 1).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);
    let pending = serde_json::to_value(&page.items[0]).unwrap();
    assert_eq!(pending["state"], "pending");
    assert_eq!(pending["attempt"], 1);
    assert_eq!(pending["mapping"], mapping["mapping_reference"]);
    assert!(directory.join("reporter-applied").exists());
    assert_eq!(
      fixture.store.issue(project_id, issue_id, None).await.unwrap().status_id,
      accepted.status_id
    );
    // Regression: this pause exceeds the former three-second shutdown race.
    // It simulates a slow test body; no supervised worker remains to consume retry.
    tokio::time::sleep(Duration::from_secs(4)).await;
    if revoked {
      let connection = fixture.input.definition().connection.as_ref().unwrap();
      AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(IntegrationProjectAccessHandlers::new(fixture.store.clone(), policy)),
      )
      .handle_command(
        &context,
        PublishIntegrationProjectAccessCommand {
          request: PublishIntegrationProjectAccess {
            intent: IntegrationProjectAccessIntent {
              connection_id: connection.id,
              connection_version: connection.version,
              project_id,
              expected_current_version: Some(IntegrationVersion::INITIAL),
              resources: vec![IntegrationResourceSelection {
                reference: fixture.input.definition().source.resource.clone(),
                purposes: vec![IntegrationPurpose::TaskRead],
              }],
            },
            idempotency_key: IdempotencyKey::new("revoke-pending-writer").unwrap(),
            published_at: SystemFactoryExecutionClock.now().unwrap(),
          },
        },
      )
      .await
      .unwrap();
    } else {
      let mut definition = mapping["resource"]["definition"].clone();
      definition["status"]["push"]["statuses"][0]["add"] = json!([]);
      definition["status"]["push"]["statuses"][0]["remove"] = json!(["custom-ready"]);
      AuthorizedCommandHandler::new(managed_issues.policy, managed_issues.handlers)
        .handle_command(
          &context,
          PublishIssueSourceMappingCommand {
            intent: PublishIssueSourceMappingIntent {
              project_id,
              issue_id,
              expected_issue_version: accepted.version,
              expected_mapping: Some(serde_json::from_value(mapping["mapping_reference"].clone()).unwrap()),
              definition: serde_json::from_value(definition).unwrap(),
            },
            idempotency_key: IdempotencyKey::new("review-after-uncertain-write").unwrap(),
            published_at: SystemFactoryExecutionClock.now().unwrap(),
          },
        )
        .await
        .unwrap();
    }
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
      "one controlled pass must leave recovery pending"
    );
    let original = fixture
      .store
      .issue_push_projection(pending["operation_id"].as_str().unwrap().parse().unwrap())
      .await
      .unwrap();
    assert_eq!(original.record.state, FactoryOutboxState::Pending);
    assert_eq!(original.record.attempt, 1);
    drop(sources);
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
    assert_eq!(retained["version"], json!(accepted.version));
    assert_eq!(retained["status_id"], json!(accepted.status_id));
    assert_eq!(get(&client, &format!("{base}/factory-runs")).await["items"], json!([]));
    runtime.shutdown().await.unwrap();
    database.cleanup().await;
  }
}
