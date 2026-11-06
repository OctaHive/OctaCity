//! Manual Issue admission with an exact result produced by an ordinary triage Build.
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
  Medium,
  Large,
  Rework,
  NonReproducingDefect,
}
impl Case {
  fn as_str(self) -> &'static str {
    match self {
      Self::Small | Self::Rework | Self::NonReproducingDefect => "small",
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
}

impl Ticket {
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
    let issue = resource_id(
      &post_management(
        client,
        origin,
        &path,
        "delivery-test-issue",
        json!({"title":"Correct the public facts output","description":description,"acceptance":acceptance}),
      )
      .await,
    );
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
    let schema = FlowDataSchema::new(
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
    assess["context_binding"] = json!({"work_task":"task","entries":{}});
    assess["build"] = json!(FlowBuildBinding {
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
    let triage_draft = workflow::draft(
      "assess",
      vec![assess],
      vec![workflow::edge("assess", "eligible", "terminal", "triaged")],
      "triaged",
      &schema,
      budget,
      &p.project_permissions,
    );
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
    let mut ticket = Self {
      path: format!("{path}/{issue}/executions"),
      issue,
      triage: String::new(),
    };
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
    draft["accepted_predecessor"] =
      json!({"definition":triage_flow["closure"]["root"],"terminal":"triaged","outcomes":["eligible"]});
    draft["root"]["input_schema"] = json!(schema.reference());
    draft["root"]["entry"] = json!(REQUIREMENTS_POLICY);
    let step = draft["root"]["nodes"]
      .as_array_mut()
      .unwrap()
      .iter_mut()
      .find(|node| node["key"] == entry)
      .unwrap();
    step["input_binding"] = json!({"sources":{"eligible":{"source":"predecessor","view":"payload","path":"/eligible"}},
      "mapping":{"value":{"mapping":"pointer","path":"/eligible"}}});
    add_requirements_review(draft, entry, &schema, &summary);
    draft["data_schemas"]
      .as_array_mut()
      .unwrap()
      .extend([json!(schema), json!(summary)]);
    ticket
  }

  pub(super) async fn start(
    &self,
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
    let response = client
      .post(format!("{origin}{}", self.path))
      .header(
        "idempotency-key",
        format!("delivery-test-start-{}", reference["id"].as_str().unwrap()),
      )
      .header("if-match", "\"1\"")
      .json(&json!({"target":target,"classification":{"priority":40,"risk":"low","metadata":{}}}))
      .send()
      .await
      .unwrap();
    let status = response.status();
    let response: Value = response.json().await.unwrap();
    assert!(status.is_success(), "Issue admission failed: {status} {response}");
    if let Some(predecessor) = predecessor {
      assert_eq!(response["resource"]["intent"]["target"]["predecessor_run"], predecessor);
    }
    string(&response["resource"]["association"], "run_id")
  }
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
