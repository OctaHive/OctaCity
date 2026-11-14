//! Manual Issue admission with an exact producer result or independent classified Issue snapshot.
use super::*;
use octacity_server_factory::{
  BudgetLimit, EvidenceOutputKind, FactoryKey, FactoryPermissionSet, FlowBuildBinding, FlowDataSchema, FlowFieldSchema,
  FlowTemplateNode, FlowValueSchema,
};

pub(super) const REQUIREMENTS_POLICY: &str = "requirements_review_policy";
pub(super) const REQUIREMENTS_REVIEW: &str = "requirements_review";
pub(super) const REQUIREMENTS_APPROVED: &str = "requirements_approved";

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Case {
  Small,
  IssueSnapshot,
  ManualSnapshot,
  MaterialSnapshot,
  Medium,
  Large,
  Rework,
  NonReproducingDefect,
}
impl Case {
  pub(super) fn issue_snapshot(self) -> bool {
    matches!(
      self,
      Self::IssueSnapshot | Self::ManualSnapshot | Self::MaterialSnapshot
    )
  }
  fn as_str(self) -> &'static str {
    match self {
      Self::Small
      | Self::IssueSnapshot
      | Self::ManualSnapshot
      | Self::MaterialSnapshot
      | Self::Rework
      | Self::NonReproducingDefect => "small",
      Self::Medium => "medium",
      Self::Large => "large",
    }
  }
}

pub(super) fn requires_review(case: Option<Case>) -> bool {
  matches!(case, Some(Case::Medium | Case::Large))
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

fn object(fields: impl IntoIterator<Item = (&'static str, FlowValueSchema)>) -> FlowValueSchema {
  FlowValueSchema::Object {
    fields: fields
      .into_iter()
      .map(|(name, shape)| (key(name), FlowFieldSchema::required(shape)))
      .collect(),
  }
}

pub(super) struct Ticket {
  path: String,
  pub(super) issue: String,
  pub(super) triage: String,
  snapshot: Option<workflow_issue_snapshot::Retained>,
}

impl Ticket {
  pub(super) fn for_issue(project: &str, issue: String) -> Self {
    Self {
      path: format!("/api/v1/projects/{project}/issues/{issue}/executions"),
      issue,
      triage: String::new(),
      snapshot: None,
    }
  }

  pub(super) async fn prepare(
    client: &Client,
    origin: &str,
    scenario: &workflow_author::AuthorScenario<'_>,
    profile: &models::InstalledModel,
    configuration: &Value,
    draft: &mut Value,
    entry: &str,
  ) -> Self {
    let project = &scenario.resources.project;
    if scenario.ticket_case == Some(Case::MaterialSnapshot) {
      workflow_issue_materials::browser(client, origin, project, scenario.directory, "fields", "", "").await;
    }
    let contract = if scenario.ticket_case.is_some_and(Case::issue_snapshot) {
      Some(if scenario.ticket_case == Some(Case::MaterialSnapshot) {
        workflow_issue_snapshot::read_contract(client, origin, project).await
      } else {
        workflow_issue_snapshot::contract(client, origin, project).await
      })
    } else {
      None
    };
    if scenario.ticket_case == Some(Case::MaterialSnapshot) {
      let catalog = get_json(
        client,
        format!("{origin}/api/v1/projects/{project}/issue-property-definitions"),
      )
      .await;
      let properties = catalog["definition"]["properties"].as_array().unwrap();
      for name in ["Material environment", "Assessment severity"] {
        assert!(
          properties.iter().any(|field| field["name"] == name),
          "Project settings must publish the ordinary/classification field {name}"
        );
      }
    }
    let path = format!("/api/v1/projects/{project}/issues");
    let defect = scenario.ticket_case == Some(Case::NonReproducingDefect);
    let (description, acceptance) = if defect {
      (
        "The public/facts.txt file reportedly contains the exact marker corrupted.",
        "Verify the reported marker on the admitted revision before authoring a fix.",
      )
    } else {
      (
        "Change public/facts.txt from baseline to facts.",
        "The public file contains exactly facts and the original tests pass.",
      )
    };
    let mut content =
      json!({"title":"Correct the public facts output","description":description,"acceptance":acceptance});
    if scenario.ticket_case == Some(Case::ManualSnapshot) {
      content["priority"] = json!("high");
      content["planning"] = json!({"definition_version":1,"labels":[],"values":{},
        "component":"api","type":"feature_request","size":"small"});
    }
    let issue = resource_id(&post_management(client, origin, &path, "delivery-test-issue", content).await);
    if scenario.ticket_case == Some(Case::MaterialSnapshot) {
      workflow_issue_materials::browser(client, origin, project, scenario.directory, "materials", &issue, "").await;
      let current = get_json(client, format!("{origin}{path}/{issue}")).await;
      assert_eq!(
        current["materials"].as_array().map_or(0, Vec::len),
        3,
        "the Console must select each material owner explicitly"
      );
    }
    let summary = FlowDataSchema::new(
      key("pilot.ticket-summary"),
      key("v1"),
      object([
        ("eligible", FlowValueSchema::Boolean),
        (
          "work_kind",
          FlowValueSchema::Enum {
            values: vec![json!("feature_request"), json!("defect")],
          },
        ),
        (
          "reproducibility",
          FlowValueSchema::Enum {
            values: vec![json!("not_applicable"), json!("not_reproduced")],
          },
        ),
        (
          "component",
          FlowValueSchema::Enum {
            values: vec![json!("api")],
          },
        ),
        (
          "size",
          FlowValueSchema::Enum {
            values: vec![json!("small"), json!("medium"), json!("large")],
          },
        ),
      ]),
    )
    .unwrap();
    let envelope = FlowDataSchema::new(
      key("pilot.ticket-triage"),
      key("v1"),
      object([
        ("summary", summary.shape().clone()),
        (
          "priority_assessment",
          FlowValueSchema::Integer {
            minimum: 0,
            maximum: 100,
          },
        ),
      ]),
    )
    .unwrap();
    let mut schema = contract.as_ref().map_or_else(
      || envelope.clone(),
      |choice| serde_json::from_value(choice["field_contract"]["schema"].clone()).unwrap(),
    );
    if scenario.ticket_case == Some(Case::MaterialSnapshot) {
      schema = workflow_issue_materials::schema(&schema);
    }
    if matches!(
      scenario.ticket_case,
      Some(Case::ManualSnapshot | Case::MaterialSnapshot)
    ) {
      let ticket = Self::for_issue(project, issue);
      let executions = get_json(client, format!("{origin}{}", ticket.path)).await;
      assert!(
        executions["items"].as_array().unwrap().is_empty(),
        "a manually classified Issue must need no triage Run"
      );
      bind_delivery(draft, entry, &schema, &envelope, &summary, None);
      if scenario.ticket_case == Some(Case::MaterialSnapshot) {
        workflow_issue_materials::extend(client, origin, project, draft, &schema, profile).await;
      }
      return ticket;
    }
    let p = &profile.profile;
    let budget = BudgetLimit::new(1, 120_000, 100, 100, 8 * 1024 * 1024).unwrap();
    let mut assess = workflow::node(
      "assess",
      "reasoning",
      &schema,
      budget,
      &p.project_permissions,
      "eligible",
    );
    assess["input_binding"]["mapping"]["value"] = json!({"mapping":"constant","value":{
      "summary":{"eligible":true,"component":"api","size":scenario.ticket_case.unwrap().as_str(),
        "work_kind":if defect {"defect"} else {"feature_request"},
        "reproducibility":if defect {"not_reproduced"} else {"not_applicable"}},"priority_assessment":80
    }});
    if contract.is_some() {
      assess["input_binding"]["mapping"]["value"] = json!({"mapping":"constant","value":{
        "priority":"high","component":"api","kind":"feature_request","size":"small"
      }});
    }
    assess["context_binding"] = json!({"work_task":"task","entries":{}});
    assess["build"] = json!(FlowBuildBinding {
      execution_permissions: None,
      tool: p.toolchain.executable.clone(),
      plugin: p.toolchain.plugin.clone(),
      result_tool: None,
      capture_write_paths: None,
      capture_deliverables: None,
      model_or_tool: p.toolchain.model.clone(),
      task_digest: p.task_digest(),
      build_configuration: p.build_configuration.clone(),
      result_output: key("codex-run-result"),
      result_output_kind: EvidenceOutputKind::Report,
      result_outcome: key("eligible"),
      evidence: vec![],
    });
    let mut triage_draft = workflow::draft(
      "assess",
      vec![assess],
      vec![workflow::edge("assess", "eligible", "terminal", "triaged")],
      "triaged",
      &schema,
      budget,
      &p.project_permissions,
    );
    if let Some(choice) = &contract {
      workflow_issue_snapshot::classify(&mut triage_draft, choice, budget, &p.project_permissions);
    }
    let triage = post_management(
      client,
      origin,
      "/api/v1/flow-definitions",
      "delivery-test-triage",
      json!({"id":Uuid::new_v4(),"project_id":project,"definition":triage_draft}),
    )
    .await;
    let triage_flow = &triage["resource"]["flow"];
    let mut configuration = configuration.clone();
    configuration["flow"] = triage_flow.clone();
    let triage = post_management(
      client,
      origin,
      "/api/v1/factory-configurations",
      "delivery-test-triage-config",
      json!({"id":Uuid::new_v4(),"project_id":project,"definition":configuration}),
    )
    .await;
    let mut ticket = Self::for_issue(project, issue);
    ticket.triage = ticket.start(client, origin, scenario, &triage, None).await;
    timeout(Duration::from_secs(120), async {
      loop {
        let run = get_json(client, format!("{origin}/api/v1/factory-runs/{}", ticket.triage)).await;
        match run["state"].as_str() {
          Some("completed") => break,
          Some("failed" | "cancelled") => panic!("the ordinary triage Build did not complete: {run}"),
          _ => sleep(Duration::from_millis(300)).await,
        }
      }
    })
    .await
    .expect("the ordinary triage Build did not settle");
    let builds = workflow::diagnostic(client, origin, &ticket.triage, "builds").await;
    assert_eq!(builds["items"].as_array().unwrap().len(), 1);
    if scenario.ticket_case == Some(Case::IssueSnapshot) {
      let classified = get_json(client, format!("{origin}{path}/{}", ticket.issue)).await;
      assert_eq!(
        classified["content"]["priority"], "high",
        "triage must apply its assessment to the Issue"
      );
      assert_eq!(classified["planning"]["component"], "api");
      assert_eq!(classified["planning"]["size"], "small");
      assert_eq!(classified["planning"]["type"], "feature_request");
      assert_eq!(classified["classification"]["fields"].as_array().unwrap().len(), 4);
    }
    bind_delivery(
      draft,
      entry,
      &schema,
      &envelope,
      &summary,
      contract.is_none().then_some(triage_flow),
    );
    ticket
  }

  pub(super) async fn start(
    &mut self,
    client: &Client,
    origin: &str,
    scenario: &workflow_author::AuthorScenario<'_>,
    configuration: &Value,
    predecessor: Option<&str>,
  ) -> String {
    let reference = &configuration["resource"]["configuration"]["reference"];
    let mut target = json!({"configuration_id":reference["id"],"configuration_version":reference["version"],
      "repository_id":scenario.resources.repository,"repository_version":1,
      "source":{"kind":"exact_revision","value":scenario.revision},"mapping":"title_description_acceptance_v1"});
    if let Some(predecessor) = predecessor {
      target["predecessor_run"] = json!(predecessor);
    }
    let snapshot = configuration["resource"]["configuration"]["flow"]["issue_snapshot"].is_object();
    let mut body = json!({"target":target});
    let issue = get_json(
      client,
      format!("{origin}{}", self.path.strip_suffix("/executions").unwrap()),
    )
    .await;
    let version = issue["version"].clone();
    let reviewed = if snapshot {
      let response = client
        .post(format!("{origin}{}/review", self.path))
        .header("if-match", format!("\"{version}\""))
        .json(&body)
        .send()
        .await
        .unwrap();
      let status = response.status();
      let review: Value = response.json().await.unwrap();
      assert!(status.is_success(), "snapshot review failed: {status} {review}");
      if scenario.ticket_case == Some(Case::MaterialSnapshot) {
        assert_eq!(
          review["snapshot"]["values"]["context"],
          json!([
            "ISSUE_TEXT_SELECTED_v1",
            "PROJECT_TEXT_SELECTED_v1",
            "COMMENT_TEXT_SELECTED_v1"
          ]),
          "the consumer must explicitly freeze the three selected texts"
        );
      }
      assert_eq!(review["snapshot"]["admission_classification"]["priority"], 80);
      body["expected_snapshot_digest"] = review["snapshot_digest"].clone();
      Some(review)
    } else {
      body["classification"] = json!({"priority":40,"risk":"low","metadata":{}});
      None
    };
    if scenario.ticket_case == Some(Case::MaterialSnapshot) {
      workflow_issue_materials::browser(
        client,
        origin,
        &scenario.resources.project,
        scenario.directory,
        "launch",
        &self.issue,
        scenario.revision,
      )
      .await;
      let executions = get_json(client, format!("{origin}{}", self.path)).await;
      assert_eq!(
        executions["items"].as_array().unwrap().len(),
        1,
        "one explicit Console confirmation admits one execution"
      );
      let request = &executions["items"][0];
      let reviewed = reviewed.unwrap();
      assert_eq!(request["snapshot_digest"], reviewed["snapshot_digest"]);
      assert_eq!(request["intent"]["target"], target);
      self.snapshot = Some(workflow_issue_snapshot::Retained {
        execution: string(request, "id"),
        reviewed,
      });
      return string(&request["association"], "run_id");
    }
    let response = client
      .post(format!("{origin}{}", self.path))
      .header(
        "idempotency-key",
        format!(
          "delivery-test-start-{}-{}-{version}",
          self.issue,
          reference["id"].as_str().unwrap()
        ),
      )
      .header("if-match", format!("\"{version}\""))
      .json(&body)
      .send()
      .await
      .unwrap();
    let status = response.status();
    let response: Value = response.json().await.unwrap();
    assert!(status.is_success(), "Issue admission failed: {status} {response}");
    if let Some(predecessor) = predecessor {
      assert_eq!(response["resource"]["intent"]["target"]["predecessor_run"], predecessor);
    }
    if let Some(reviewed) = reviewed {
      assert_eq!(response["resource"]["snapshot_digest"], reviewed["snapshot_digest"]);
      assert!(
        response["resource"]["intent"]["target"]
          .get("predecessor_run")
          .is_none_or(Value::is_null)
      );
      self.snapshot = Some(workflow_issue_snapshot::Retained {
        execution: string(&response["resource"], "id"),
        reviewed,
      });
    }
    string(&response["resource"]["association"], "run_id")
  }

  pub(super) async fn verify_snapshot(&self, client: &Client, origin: &str, case: Option<Case>, run: &Value) {
    if case == Some(Case::MaterialSnapshot) {
      let probes = workflow_issue_materials::verify_probes(client, origin, run["id"].as_str().unwrap()).await;
      let (request, snapshot) =
        workflow_issue_materials::verify_retained(client, origin, &self.path, self.snapshot.as_ref().unwrap(), run)
          .await;
      workflow_issue_materials::save_native_evidence(run, &request, &snapshot, &probes);
    }
    if matches!(case, Some(Case::IssueSnapshot | Case::ManualSnapshot)) {
      self
        .verify_retained(client, origin, run, if self.triage.is_empty() { 1 } else { 2 })
        .await;
    }
  }

  pub(super) async fn verify_retained(&self, client: &Client, origin: &str, run: &Value, expected_version: u64) {
    let retained = self
      .snapshot
      .as_ref()
      .expect("Delivery starts from a reviewed Issue snapshot");
    assert_eq!(retained.reviewed["snapshot"]["version"], expected_version);
    retained
      .verify(
        client,
        origin,
        &self.path,
        (!self.triage.is_empty()).then_some(self.triage.as_str()),
        run,
      )
      .await;
  }
}

fn bind_delivery(
  draft: &mut Value,
  entry: &str,
  schema: &FlowDataSchema,
  envelope: &FlowDataSchema,
  summary: &FlowDataSchema,
  producer: Option<&Value>,
) {
  if let Some(triage_flow) = producer {
    draft["accepted_predecessor"] =
      json!({"definition":triage_flow["closure"]["root"],"terminal":"triaged","outcomes":["eligible"]});
  }
  draft["root"]["input_schema"] = json!(envelope.reference());
  draft["root"]["entry"] = json!(REQUIREMENTS_POLICY);
  let step = draft["root"]["nodes"]
    .as_array_mut()
    .unwrap()
    .iter_mut()
    .find(|node| node["key"] == entry)
    .unwrap();
  step["input_binding"] = json!({"sources":{"eligible":{"source":"predecessor","view":"payload","path":"/eligible"}},
      "mapping":{"value":{"mapping":"pointer","path":"/eligible"}}});
  add_requirements_review(draft, entry, envelope, summary);
  if producer.is_none() {
    workflow_issue_snapshot::consume(draft, schema);
    draft["data_schemas"].as_array_mut().unwrap().push(json!(envelope));
  }
  draft["data_schemas"]
    .as_array_mut()
    .unwrap()
    .extend([json!(schema), json!(summary)]);
}

fn add_requirements_review(draft: &mut Value, next: &str, triage: &FlowDataSchema, summary: &FlowDataSchema) {
  let part: Value = serde_json::from_str(include_str!(
    "../../../../../config/factory/presets/ticket-delivery/requirements-approval-v1.json"
  ))
  .unwrap();
  let template = |key: &str| -> FlowTemplateNode {
    serde_json::from_value(
      part["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["key"] == key)
        .unwrap()
        .clone(),
    )
    .unwrap()
  };
  let rule = template(REQUIREMENTS_POLICY);
  let mut policy = workflow::node(
    REQUIREMENTS_POLICY,
    "deterministic_gate",
    summary,
    rule.budget,
    &FactoryPermissionSet::deny_all(),
    "automatic",
  );
  policy["input_schema"] = json!(triage.reference());
  policy["input_binding"] = json!({"sources":{"triage":{"source":"incoming","schema":triage.reference(),"path":""}},
    "mapping":{"value":{"mapping":"pointer","path":"/triage"}}});
  policy["outcomes"]
    .as_array_mut()
    .unwrap()
    .push(json!({"key":"review","kind":"success","schema":summary.reference()}));
  policy["gate"] = json!(rule.gate.unwrap().binding().unwrap());
  policy["accepted_work_outcomes"] = json!(rule.accepted_work_outcomes);
  let response_schema = &part["schemas"]["requirements_review_decision"];
  let response = FlowDataSchema::new(
    key(response_schema["identity"].as_str().unwrap()),
    key(response_schema["version"].as_str().unwrap()),
    serde_json::from_value(response_schema["shape"].clone()).unwrap(),
  )
  .unwrap();
  let review_rule = template(REQUIREMENTS_REVIEW);
  let mut review = workflow::node(
    REQUIREMENTS_REVIEW,
    "human_gate",
    &response,
    BudgetLimit::new(1, 90_000, 0, 0, review_rule.budget.max_output_bytes()).unwrap(),
    &FactoryPermissionSet::deny_all(),
    "approved",
  );
  review["input_schema"] = json!(summary.reference());
  review["input_binding"] = json!(review_rule.input_binding);
  review["operator_requests"] = json!(review_rule.operator_requests);
  review["outcomes"]
    .as_array_mut()
    .unwrap()
    .push(json!({"key":"rejected","kind":"failure","schema":response.reference()}));
  let approved_rule = template(REQUIREMENTS_APPROVED);
  let mut approved = workflow::node(
    REQUIREMENTS_APPROVED,
    "deterministic_gate",
    summary,
    approved_rule.budget,
    &FactoryPermissionSet::deny_all(),
    "accepted",
  );
  approved["input_binding"] = json!(approved_rule.input_binding);
  approved["gate"] = json!(approved_rule.gate.unwrap().binding().unwrap());
  approved["accepted_work_outcomes"] = json!(approved_rule.accepted_work_outcomes);
  draft["root"]["nodes"]
    .as_array_mut()
    .unwrap()
    .extend([policy, review, approved]);
  draft["root"]["transitions"].as_array_mut().unwrap().extend([
    workflow::edge(REQUIREMENTS_POLICY, "automatic", "node", next),
    workflow::edge(REQUIREMENTS_POLICY, "review", "node", REQUIREMENTS_REVIEW),
    workflow::edge(REQUIREMENTS_REVIEW, "approved", "node", REQUIREMENTS_APPROVED),
    workflow::edge(REQUIREMENTS_REVIEW, "rejected", "terminal", "needs_correction"),
    workflow::edge(REQUIREMENTS_APPROVED, "accepted", "node", next),
  ]);
  draft["root"]["terminals"]
    .as_array_mut()
    .unwrap()
    .push(json!({"key":"needs_correction","schema":response.reference()}));
  draft["data_schemas"].as_array_mut().unwrap().push(json!(response));
}

pub(super) async fn approve_requirements(client: &Client, origin: &str, run: &str, case: Case) {
  timeout(Duration::from_secs(120), async {
    loop {
      let attempts = workflow::diagnostic(client, origin, run, "node-attempts").await;
      let status = get_json(client, format!("{origin}/api/v1/factory-runs/{run}")).await;
      if let Some(row) = attempts["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["record"]["node_key"] == REQUIREMENTS_REVIEW && row["record"]["active"] == true)
      {
        let builds = workflow::diagnostic(client, origin, run, "builds").await;
        let attempt = string(&row["record"], "id");
        let action = get_json(client, format!("{origin}/api/v1/factory-runs/{run}/nodes/{attempt}/action")).await;
        if action["state"] != "waiting" {
          sleep(Duration::from_millis(250)).await;
          continue;
        }
        let builds = builds["items"].as_array().unwrap();
        if case == Case::Large {
          let completions = workflow::diagnostic(client, origin, run, "node-completions").await;
          assert!(completions["items"].as_array().unwrap().iter().any(|row| {
            row["record"]["node_key"] == "specification_author"
          }), "a large ticket reached human review before producing a specification");
          assert_eq!(builds.len(), 1, "only specification authoring may execute before approval");
        } else {
          assert!(builds.is_empty(), "test authoring waits for exact requirements approval");
        }
        let receipt = client.post(format!("{origin}/api/v1/factory-runs/{run}/nodes/{attempt}/requests"))
          .header("idempotency-key", format!("approve-{attempt}-{}", action["run_version"]))
          .header("if-match", format!("\"{}\"", action["run_version"]))
          .json(&json!({"operation":"escalation","disposition":"resume","reason":"Approved the retained ticket requirements."}))
          .send().await.unwrap();
        let status = receipt.status();
        let receipt: Value = receipt.json().await.unwrap();
        if status == reqwest::StatusCode::CONFLICT {
          let current = get_json(client, format!("{origin}/api/v1/factory-runs/{run}/nodes/{attempt}/action")).await;
          if current["state"] == "waiting" && current["run_version"] != action["run_version"] {
            continue;
          }
        }
        assert!(status.is_success(), "requirements approval failed: {status} {receipt}");
        assert_eq!(receipt["run_id"], run);
        return;
      }
      assert!(
        !matches!(status["state"].as_str(), Some("completed" | "failed" | "cancelled")),
        "a matching ticket terminated before requesting requirements review: {status}"
      );
      sleep(Duration::from_millis(250)).await;
    }
  })
  .await
  .expect("a matching ticket must request requirements review");
}

pub(super) async fn assert_non_reproduction(client: &Client, origin: &str, run: &str) {
  timeout(Duration::from_secs(120), async {
    loop {
      let status = get_json(client, format!("{origin}/api/v1/factory-runs/{run}")).await;
      let attempts = workflow::diagnostic(client, origin, run, "node-attempts").await;
      assert!(
        !attempts["items"].as_array().unwrap().iter().any(|row| {
          row["record"]["node_key"]
            .as_str()
            .is_some_and(|node| node.starts_with("author-"))
        }),
        "an unresolved defect reached test authoring before a completed reproduction check"
      );
      match status["state"].as_str() {
        Some("completed") => break,
        Some("failed" | "cancelled") => panic!("reproduction did not complete: {status}"),
        _ => sleep(Duration::from_millis(300)).await,
      }
    }
  })
  .await
  .expect("the negative reproduction must settle");
  let completions = workflow::diagnostic(client, origin, run, "node-completions").await;
  assert!(completions["items"].as_array().unwrap().iter().any(|row| {
    row["record"]["node_key"] == "reproduction-result" && row["record"]["outcome"] == "cannot_reproduce"
  }));
  let builds = workflow::diagnostic(client, origin, run, "builds").await;
  assert_eq!(
    builds["items"].as_array().unwrap().len(),
    1,
    "only the actual reproduction Build was dispatched"
  );
}

pub(super) async fn assert_bounded_rework(
  client: &Client,
  origin: &str,
  run: &str,
  implementation: &str,
  decision: &str,
) {
  timeout(Duration::from_secs(600), async {
    loop {
      let state = get_json(client, format!("{origin}/api/v1/factory-runs/{run}")).await;
      let attempts = workflow::diagnostic(client, origin, run, "node-attempts").await;
      if matches!(
        state["state"].as_str(),
        Some("waiting" | "completed" | "failed" | "cancelled")
      ) {
        let attempts = attempts["items"].as_array().unwrap();
        assert_eq!(
          attempts
            .iter()
            .filter(|row| row["record"]["node_key"] == implementation)
            .count(),
          2,
          "a release reviewer finding allows one additional implementation attempt"
        );
        let completions = workflow::diagnostic(client, origin, run, "node-completions").await;
        assert_eq!(
          completions["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| { row["record"]["node_key"] == decision && row["record"]["outcome"] == "rework" })
            .count(),
          2,
          "each candidate must have its own negative independent decision"
        );
        assert_eq!(state["state"], "waiting", "a second rejection pauses delivery");
        let active = state["diagnostics"]["current_nodes"].as_array().unwrap();
        assert!(active.iter().any(|node| node["node"] == workflow_rework::PAUSED));
        break;
      }
      sleep(Duration::from_millis(500)).await;
    }
  })
  .await
  .expect("the released rework case must settle");
}
