//! Triage qualification reuses the packaged Source gateway and durable Issue owner.
use super::*;

pub(super) async fn raw_source() {
  let mut environment = source_environment::SourceEnvironment::start_with_factory().await;
  {
    let mut replies = environment.provider.replies.lock().await;
    replies.clear();
    for _ in 0..128 {
      replies.push_back(source_provider::Reply::json(json!([source_provider::issue(9001, 17)])));
    }
  }
  let project = source_setup::enroll_writer(&environment.client, &environment.origin, &environment.installation).await;
  environment.install_source_completion(&project, &"a".repeat(40)).await;
  environment
    .install_factory(&source_factory_configuration::catalog(std::slice::from_ref(&project)))
    .await;
  let (target, repository) = source_factory_configuration::publish(&environment, &project, None).await;
  let connections = get_json(
    &environment.client,
    format!("{}/api/v1/integrations?limit=100", environment.origin),
  )
  .await;
  let connection = connections["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|item| item["definition"]["name"] == format!("Packaged tasks {project}"))
    .unwrap()["id"]
    .as_str()
    .unwrap();
  let choices = get_json(
    &environment.client,
    format!(
      "{}/api/v1/projects/{project}/factory-source-choices?connection_id={connection}&connection_version=1",
      environment.origin
    ),
  )
  .await;
  let source = choices["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|choice| choice["purpose"] == "task_read")
    .unwrap();
  assert_eq!(
    source["preparation"]["kind"], "process_completion",
    "raw triage must select reviewed process criteria independently of ticket acceptance"
  );
  assert_eq!(source["preparation"]["value"]["project_id"], project);
  let id = Uuid::new_v4();
  let intent = json!({"project_id":project,"group":null,"inputs":[{"id":id,"expected_version":null,
    "definition":{"target":target,"repository":repository,"source":source["binding"],"enabled":true,"reporter":null,
      "predicate":{"state":"open","label_presence":"none","all_of":[],"any_of":[],"none_of":[]},
      "connection":{"id":connection,"version":1,"access_version":1,"source_configuration":source["connection_configuration"],"reporter_configuration":null}}}]});
  source_setup::publish(
    &environment.client,
    &environment.origin,
    &format!("/api/v1/projects/{project}/factory-source-inputs/{id}/versions"),
    &id.to_string(),
    "if-none-match",
    "*",
    json!({"intent":intent}),
  )
  .await;
  let diagnostics_path = format!(
    "{}/api/v1/projects/{project}/factory-source-inputs/{id}/versions/1/diagnostics",
    environment.origin
  );
  let deadline = Instant::now() + Duration::from_secs(30);
  let diagnostics = loop {
    let observed = get_json(&environment.client, diagnostics_path.clone()).await;
    if observed["admissions"].as_array().unwrap().len() == 1 {
      break observed;
    }
    assert!(
      Instant::now() < deadline,
      "raw input must reach ordinary admission: {observed}"
    );
    sleep(Duration::from_millis(100)).await;
  };
  let admitted = &diagnostics["admissions"][0];
  assert!(
    admitted["issue"].is_null(),
    "execution must not require creating a canonical card"
  );
  let run = admitted["association"]["run_id"].as_str().unwrap();
  let work = get_json(
    &environment.client,
    format!("{}/api/v1/factory-runs/{run}/work", environment.origin),
  )
  .await;
  assert_eq!(work["items"].as_array().unwrap().len(), 1);
  assert_eq!(work["items"][0]["record"]["subject"]["repository_id"], repository["id"]);
  assert_eq!(work["items"][0]["record"]["subject"]["base_revision"], "a".repeat(40));
  assert_eq!(admitted["reporting"], json!([]));
  let cards = get_json(
    &environment.client,
    format!("{}/api/v1/projects/{project}/issues", environment.origin),
  )
  .await;
  assert_eq!(cards["items"], json!([]));
  fs::write(
    environment.evidence.join("triage-raw-source.json"),
    serde_json::to_vec_pretty(&json!({
      "project_id":project,"input_id":id,"preparation":source["preparation"],"diagnostics":diagnostics,"work":work
    }))
    .unwrap(),
  )
  .unwrap();
  environment.record(&project, "packaged-triage-source.json").await;
  environment.stop().await;
}

pub(super) async fn issue_mcp() {
  let mut environment = source_environment::SourceEnvironment::start().await;
  let client = &environment.client;
  let origin = &environment.origin;
  let project = resource_id(
    &post_management(
      client,
      origin,
      "/api/v1/projects",
      &Uuid::new_v4().to_string(),
      json!({"name":format!("Packaged triage MCP {}",Uuid::new_v4())}),
    )
    .await,
  );
  let mut issues = Vec::new();
  for (title, description) in [
    ("Candidate public", "Retained requirements"),
    ("Candidate hidden", "private-hidden-candidate-canary"),
  ] {
    let created = post_management(
      client,
      origin,
      &format!("/api/v1/projects/{project}/issues"),
      &Uuid::new_v4().to_string(),
      json!({"title":title,"description":description,"acceptance":null,"priority":null}),
    )
    .await;
    issues.push(created["resource"].clone());
  }
  let token = format!("ocmcp.packaged-triage.{}", "ab".repeat(32));
  environment
    .install_issue_reader(&project, issues[0]["id"].as_str().unwrap(), &token)
    .await;
  let initialized = request(
    &environment,
    &token,
    None,
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
  )
  .await;
  assert_eq!(
    initialized.status(),
    200,
    "normal gateway must preserve the separate MCP bearer"
  );
  let session = initialized.headers()["mcp-session-id"].to_str().unwrap().to_owned();
  let mut observations = Vec::new();
  for (name, arguments) in [
    ("search_issues", json!({"query":"Candidate","limit":1})),
    ("read_issue", json!({"issue_id":issues[0]["id"]})),
    ("read_issue", json!({"issue_id":issues[1]["id"]})),
    (
      "search_issues",
      json!({"query":"Candidate","project_id":Uuid::new_v4()}),
    ),
    (
      "write_issue",
      json!({"issue_id":issues[0]["id"],"title":"Unauthorized mutation"}),
    ),
  ] {
    let result = request(
      &environment,
      &token,
      Some(&session),
      json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":arguments}}),
    )
    .await;
    assert_eq!(result.status(), 200);
    observations.push(result.json::<Value>().await.unwrap());
  }
  let content = |index: usize| {
    serde_json::from_str::<Value>(observations[index]["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
  };
  assert_eq!(content(0)["items"][0]["id"], issues[0]["id"]);
  assert_eq!(content(0)["items"].as_array().unwrap().len(), 1);
  assert_eq!(content(0)["scope"]["project_id"], project);
  for field in [
    "id",
    "project_id",
    "version",
    "content",
    "workflow_id",
    "workflow_version",
    "status_id",
  ] {
    assert_eq!(content(1)["issue"][field], issues[0][field]);
  }
  assert_eq!(observations[2]["result"]["isError"], true);
  assert_eq!(observations[3]["error"]["code"], -32602);
  assert_eq!(observations[4]["error"]["code"], -32602);
  assert!(
    !serde_json::to_string(&observations)
      .unwrap()
      .contains("private-hidden-candidate-canary")
  );
  for credential in [
    source_evidence::READER,
    source_evidence::WRITER,
    "fixture-model-only",
    "fixture-external-mcp-read-only",
  ] {
    let denied = request(
      &environment,
      credential,
      None,
      json!({"jsonrpc":"2.0","id":3,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
    )
    .await;
    assert_eq!(
      denied.status(),
      401,
      "another credential purpose must not authenticate as Issue MCP"
    );
  }
  for issue in &issues {
    let original = get_json(
      &environment.client,
      format!(
        "{}/api/v1/projects/{project}/issues/{}",
        environment.origin,
        issue["id"].as_str().unwrap()
      ),
    )
    .await;
    assert_eq!(&original, issue, "investigation cannot mutate either canonical Issue");
  }
  fs::write(
    environment.evidence.join("triage-issue-mcp.json"),
    serde_json::to_vec_pretty(&json!({
      "project_id":project,"visible_issue":issues[0]["id"],"hidden_issue":issues[1]["id"],"observations":observations
    }))
    .unwrap(),
  )
  .unwrap();
  environment.record(&project, "packaged-triage-mcp.json").await;
  environment.stop().await;
}

async fn request(
  environment: &source_environment::SourceEnvironment,
  token: &str,
  session: Option<&str>,
  message: Value,
) -> reqwest::Response {
  let mut request = environment
    .client
    .post(format!("{}/mcp/issues", environment.origin))
    .bearer_auth(token)
    .header("accept", "application/json, text/event-stream");
  if let Some(session) = session {
    request = request.header("mcp-session-id", session);
  }
  request.json(&message).send().await.unwrap()
}
