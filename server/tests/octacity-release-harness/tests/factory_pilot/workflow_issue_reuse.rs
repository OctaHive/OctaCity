//! Packaged reuse and freshness qualification through the existing Issue/Factory HTTP owners.
use super::*;
use octacity_server_factory::*;

pub(super) struct Replacement {
  ticket: workflow_ticket::Ticket,
  triage: Value,
  blockers: Vec<Value>,
}

impl Replacement {
  pub(super) fn issue(&self) -> &str {
    &self.ticket.issue
  }

  pub(super) async fn prepare(
    client: &Client,
    origin: &str,
    scenario: &workflow_author::AuthorScenario<'_>,
    definition: &Value,
    delivery: &Value,
    manual: &str,
  ) -> Self {
    let project = &scenario.resources.project;
    let capabilities = get_json(
      client,
      format!("{origin}/api/v1/projects/{project}/factory-authoring-capabilities"),
    )
    .await;
    let choice = &capabilities["issue_classification"];
    let schema: FlowDataSchema = serde_json::from_value(choice["field_contract"]["schema"].clone()).unwrap();
    let assessment = json!({"priority":"high","component":"api","kind":"feature_request","size":"small"});
    let budget = BudgetLimit::new(1, workflow_prelude::LOCAL_GATE_WINDOW_MILLIS, 0, 0, 4096).unwrap();
    let permissions = FactoryPermissionSet::deny_all();
    // This replacement is an ordinary deterministic policy, not the model triage of 7.1.
    let mut producer = workflow::node(
      "assess",
      "deterministic_gate",
      &schema,
      budget,
      &permissions,
      "eligible",
    );
    producer["input_binding"]["mapping"]["value"] = json!({"mapping":"constant","value":assessment});
    producer["gate"] = json!(
      FlowGateProgram::new(vec![], "eligible".parse().unwrap())
        .unwrap()
        .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Constant { value: assessment }).unwrap())
        .unwrap()
        .binding()
        .unwrap()
    );
    let mut draft = workflow::draft(
      "assess",
      vec![producer],
      vec![workflow::edge("assess", "eligible", "terminal", "triaged")],
      "triaged",
      &schema,
      budget,
      &permissions,
    );
    workflow_issue_snapshot::classify(&mut draft, choice, budget, &permissions);
    let flow = post_management(
      client,
      origin,
      "/api/v1/flow-definitions",
      "replacement-triage-flow",
      json!({"id":Uuid::new_v4(),"project_id":project,"definition":draft}),
    )
    .await;
    let mut definition = definition.clone();
    definition["flow"] = flow["resource"]["flow"].clone();
    let triage = post_management(
      client,
      origin,
      "/api/v1/factory-configurations",
      "replacement-triage-configuration",
      json!({"id":Uuid::new_v4(),"project_id":project,"definition":definition}),
    )
    .await;
    let original = get_json(client, format!("{origin}/api/v1/projects/{project}/issues/{manual}")).await;
    let mut content = original["content"].clone();
    content["priority"] = Value::Null;
    let created = post_management(
      client,
      origin,
      &format!("/api/v1/projects/{project}/issues"),
      "replacement-issue",
      content,
    )
    .await;
    let mut ticket = workflow_ticket::Ticket::for_issue(project, resource_id(&created));
    ticket.triage = ticket.start(client, origin, scenario, &triage, None).await;
    settle(client, origin, &ticket.triage).await;
    assert!(
      workflow::diagnostic(client, origin, &ticket.triage, "builds").await["items"]
        .as_array()
        .unwrap()
        .is_empty(),
      "the replacement Flow really uses a different execution implementation"
    );
    let path = format!("/api/v1/projects/{project}/issues/{}", ticket.issue);
    let assessed = get_json(client, format!("{origin}{path}")).await;
    assert_eq!(assessed["version"], 2);
    let mut changed = assessed["content"].clone();
    changed["acceptance"] = json!("Changed requirements need another assessment.");
    patch(client, origin, &path, 2, "replacement-changed-acceptance", &changed).await;
    let first = blocked(client, origin, scenario, &ticket.issue, delivery, "changed").await;
    patch(
      client,
      origin,
      &path,
      3,
      "replacement-restored-acceptance",
      &assessed["content"],
    )
    .await;
    let restored = blocked(client, origin, scenario, &ticket.issue, delivery, "restored").await;
    Self {
      ticket,
      triage,
      blockers: vec![first, restored],
    }
  }

  /// Called after the first real PR's Server crash/recovery, with the same workers and publication.
  pub(super) async fn deliver(
    &mut self,
    client: &Client,
    origin: &str,
    scenario: &workflow_author::AuthorScenario<'_>,
    configuration: &Value,
  ) {
    self.blockers.push(
      blocked(
        client,
        origin,
        scenario,
        &self.ticket.issue,
        configuration,
        "after-restart",
      )
      .await,
    );
    self.ticket.triage = self.ticket.start(client, origin, scenario, &self.triage, None).await;
    settle(client, origin, &self.ticket.triage).await;
    let run = self.ticket.start(client, origin, scenario, configuration, None).await;
    let completed = settle(client, origin, &run).await;
    self.ticket.verify_retained(client, origin, &completed, 5).await;
    let path = format!(
      "/api/v1/projects/{}/issues/{}",
      scenario.resources.project, self.ticket.issue
    );
    let current = get_json(client, format!("{origin}{path}")).await;
    let mut planning = current["planning"].clone();
    planning["size"] = json!("large");
    patch(
      client,
      origin,
      &format!("{path}/planning"),
      5,
      "replacement-later-size",
      &planning,
    )
    .await;
    self.ticket.verify_retained(client, origin, &completed, 5).await;
  }
}

async fn patch(client: &Client, origin: &str, path: &str, version: u64, key: &str, body: &Value) {
  let response = client
    .patch(format!("{origin}{path}"))
    .header("if-match", format!("\"{version}\""))
    .header("idempotency-key", key)
    .json(body)
    .send()
    .await
    .unwrap();
  let status = response.status();
  let response: Value = response.json().await.unwrap();
  assert!(status.is_success(), "Issue preparation failed: {status} {response}");
}

async fn settle(client: &Client, origin: &str, run: &str) -> Value {
  timeout(Duration::from_secs(300), async {
    loop {
      let state = get_json(client, format!("{origin}/api/v1/factory-runs/{run}")).await;
      if state["state"] == "completed" {
        return state;
      }
      assert!(
        !matches!(state["state"].as_str(), Some("failed" | "cancelled")),
        "replacement execution stopped: {state}"
      );
      let builds = workflow::diagnostic(client, origin, run, "builds").await;
      for row in builds["items"].as_array().unwrap() {
        let build = get_json(
          client,
          format!("{origin}/api/v1/builds/{}", string(&row["record"], "build_id")),
        )
        .await;
        assert!(
          !matches!(build["state"].as_str(), Some("failed" | "cancelled")),
          "replacement ordinary Build failed"
        );
      }
      sleep(Duration::from_millis(300)).await;
    }
  })
  .await
  .expect("replacement execution must settle")
}

async fn blocked(
  client: &Client,
  origin: &str,
  scenario: &workflow_author::AuthorScenario<'_>,
  issue: &str,
  configuration: &Value,
  phase: &str,
) -> Value {
  let project = &scenario.resources.project;
  let path = format!("/api/v1/projects/{project}/issues/{issue}");
  let current = get_json(client, format!("{origin}{path}")).await;
  let reference = &configuration["resource"]["configuration"]["reference"];
  let body = json!({"target":{"configuration_id":reference["id"],"configuration_version":reference["version"],
    "repository_id":scenario.resources.repository,"repository_version":1,
    "source":{"kind":"exact_revision","value":scenario.revision},"mapping":"title_description_acceptance_v1"}});
  let executions = get_json(client, format!("{origin}{path}/executions")).await;
  let run_ids = |page: &Value| {
    page["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|r| string(r, "id"))
      .collect::<std::collections::BTreeSet<_>>()
  };
  let runs = get_json(
    client,
    format!("{origin}/api/v1/projects/{project}/factory-runs?limit=100"),
  )
  .await;
  let mut failures = Vec::new();
  for suffix in ["/executions/review", "/executions"] {
    let response = client
      .post(format!("{origin}{path}{suffix}"))
      .header("if-match", format!("\"{}\"", current["version"]))
      .header(
        "idempotency-key",
        format!(
          "reject-{phase}-{}",
          if suffix.ends_with("review") { "review" } else { "start" }
        ),
      )
      .json(&body)
      .send()
      .await
      .unwrap();
    let status = response.status();
    let failure: Value = response.json().await.unwrap();
    assert_eq!(
      status,
      reqwest::StatusCode::BAD_REQUEST,
      "stale assessment cannot start delivery: {failure}"
    );
    assert!(
      failure["diagnostics"]["errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["code"] == "issue_snapshot_inapplicable"
          && e["field"].as_str().unwrap().starts_with("/issue_snapshot/fields/"))
    );
    failures.push(failure["diagnostics"]["errors"].clone());
  }
  assert_eq!(get_json(client, format!("{origin}{path}/executions")).await, executions);
  assert_eq!(
    run_ids(
      &get_json(
        client,
        format!("{origin}/api/v1/projects/{project}/factory-runs?limit=100")
      )
      .await
    ),
    run_ids(&runs)
  );
  json!({"phase":phase,"version":current["version"],"errors":failures,"execution_count":executions["items"].as_array().unwrap().len()})
}

pub(super) async fn assert_prepared(client: &Client, origin: &str, project: &str, manual: &str) -> String {
  let issues = get_json(client, format!("{origin}/api/v1/projects/{project}/issues?limit=100")).await;
  let items = issues["items"].as_array().unwrap();
  assert_eq!(
    items.len(),
    2,
    "the same delivery must serve a manual Issue and a replacement-triage Issue"
  );
  string(items.iter().find(|issue| issue["id"] != manual).unwrap(), "id")
}

impl Replacement {
  pub(super) async fn verify_finished(
    &self,
    client: &Client,
    origin: &str,
    scenario: &workflow_author::AuthorScenario<'_>,
    configuration: &Value,
    original_completions: &Value,
    delivery: &workflow_delivery::InstalledDelivery,
  ) {
    let issue = &self.ticket.issue;
    let path = format!("/api/v1/projects/{}/issues/{issue}", scenario.resources.project);
    let reference = &configuration["resource"]["configuration"]["reference"];
    let executions = get_json(client, format!("{origin}{path}/executions")).await;
    let selected: Vec<_> = executions["items"]
      .as_array()
      .unwrap()
      .iter()
      .filter(|request| {
        request["intent"]["target"]["configuration_id"] == reference["id"]
          && request["intent"]["target"]["configuration_version"] == reference["version"]
      })
      .collect();
    assert_eq!(
      selected.len(),
      1,
      "the replacement Issue uses the exact original delivery Configuration"
    );
    let request = selected[0];
    assert!(
      request["intent"]["target"]
        .get("predecessor_run")
        .is_none_or(Value::is_null)
    );
    let run = string(&request["association"], "run_id");
    let state = get_json(client, format!("{origin}/api/v1/factory-runs/{run}")).await;
    assert_eq!(state["state"], "completed");
    let completions = workflow::diagnostic(client, origin, &run, "node-completions").await;
    let outcomes = |page: &Value| {
      page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| (string(&row["record"], "node_key"), string(&row["record"], "outcome")))
        .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
      outcomes(&completions),
      outcomes(original_completions),
      "replacement triage cannot remove delivery checks"
    );
    let builds = workflow::diagnostic(client, origin, &run, "builds").await;
    assert_eq!(builds["items"].as_array().unwrap().len(), 4);
    let mut candidate = None;
    for row in builds["items"].as_array().unwrap() {
      let build = string(&row["record"], "build_id");
      let observed = get_json(client, format!("{origin}/api/v1/builds/{build}")).await;
      assert_eq!(observed["state"], "succeeded");
      let artifacts = get_json(client, format!("{origin}/api/v1/builds/{build}/artifacts?limit=100")).await;
      if let Some(facts) = artifacts["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == models::CANDIDATE_FACTS)
      {
        assert!(candidate.is_none());
        let download = client
          .post(format!("{origin}/api/v1/artifacts/{}/download", string(facts, "id")))
          .send()
          .await
          .unwrap()
          .error_for_status()
          .unwrap()
          .json::<Value>()
          .await
          .unwrap();
        candidate = Some(get_json(client, string(&download, "get_url")).await);
      }
    }
    let reviews = delivery
      .verify_reused_review(
        candidate.as_ref().expect("actual captured candidate"),
        scenario.revision,
      )
      .await;
    let detail = get_json(
      client,
      format!("{origin}{path}/executions/{}/snapshot", string(request, "id")),
    )
    .await;
    let snapshot = &detail["snapshot"];
    assert_eq!(detail["availability"], "available");
    assert_eq!(snapshot["version"], 5);
    assert_eq!(
      snapshot["values"],
      json!({"priority":"high","component":"api","kind":"feature_request","size":"small"})
    );
    assert_eq!(snapshot["attribution"].as_object().unwrap().len(), 4);
    for field in snapshot["attribution"].as_object().unwrap().values() {
      assert_eq!(field["origin"]["kind"], "pipeline");
      let producer = get_json(
        client,
        format!("{origin}/api/v1/factory-runs/{}", string(&field["origin"], "run_id")),
      )
      .await;
      assert_eq!(producer["state"], "completed");
      assert_ne!(producer["configuration_id"], reference["id"]);
    }
    let current = get_json(client, format!("{origin}{path}")).await;
    assert_eq!(current["version"], 6);
    assert_eq!(
      current["planning"]["size"], "large",
      "restart cannot refresh the admitted snapshot from current fields"
    );
    self.ticket.verify_retained(client, origin, &state, 5).await;
    if let Some(directory) = env::var_os("OCTACITY_RELEASE_EVIDENCE_DIR") {
      let directory = PathBuf::from(directory);
      assert!(directory.is_absolute() && directory.is_dir() && !directory.is_symlink());
      let report = json!({"configuration":reference,"replacement_issue":issue,
      "execution":request,"snapshot":snapshot,"run":state,"builds":builds,
      "completions":completions,"candidate":candidate,"reviews":reviews,"blockers":self.blockers});
      private_file(
        &directory,
        &format!("issue-snapshot-reuse-{run}.json"),
        &report.to_string(),
      );
    }
  }
}
