//! A provider-owned native ticket enters the separately enabled downstream configuration once.
use super::*;
use octacity_server_factory::{
  BudgetLimit, FactoryPermissionSet, FlowDataMapping, FlowDataSchema, FlowGateProgram, FlowValueMapping,
  FlowValueSchema,
};
const ACCEPTANCE: &str = "Preserve the reviewed native task identity in the delivered candidate.";

pub(super) async fn run() {
  let database = database::PilotDatabase::create(&required_string("OCTACITY_POSTGRES_URL")).await;
  let mut env = source_environment::SourceEnvironment::start_with_factory_on(&database.url).await;
  let client = env.client.clone();
  let origin = env.origin.clone();
  let project = resource_id(
    &post_management(
      &client,
      &origin,
      "/api/v1/projects",
      &Uuid::new_v4().to_string(),
      json!({"name":format!("Source lineage {}", Uuid::new_v4())}),
    )
    .await,
  );
  let connection = source_setup::enroll_resource(
    &client,
    &origin,
    &env.installation,
    &project,
    &env.installation.resource,
    false,
  )
  .await;
  let mut ticket = source_provider::issue(9001, 17);
  ticket["body"] = json!("Bounded source task\n\n## Acceptance\nPreserve the reviewed native task identity.");
  let metadata = env.provider.repository.lock().await.clone();
  env.provider.resources.lock().await.insert(
    string(&env.installation.resource, "locator"),
    source_provider::NativeResource {
      metadata,
      issues: vec![ticket],
    },
  );
  let issue = import_issue(&env, &project, &connection).await;
  let issue_url = format!("{origin}/api/v1/projects/{project}/issues/{}", issue.as_str().unwrap());
  let original_issue = get_json(&client, issue_url.clone()).await;
  let mut content = original_issue["content"].clone();
  content["acceptance"] = json!(ACCEPTANCE);
  client
    .patch(issue_url)
    .header("idempotency-key", format!("source-lineage-acceptance-{project}"))
    .header("if-match", format!("\"{}\"", original_issue["version"]))
    .json(&content)
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap();
  env
    .install_factory(&source_factory_configuration::catalog(std::slice::from_ref(&project)))
    .await;
  let choices = get_json(
    &client,
    format!(
      "{origin}/api/v1/projects/{project}/factory-source-choices?connection_id={connection}&connection_version=1"
    ),
  )
  .await;
  let source = choices["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|item| item["purpose"] == "task_read")
    .unwrap();
  let repository = post_management(
    &client,
    &origin,
    "/api/v1/repositories",
    &format!("source-lineage-code-{project}"),
    json!({
      "project_id":project,"name":"Source lineage code","definition":{"vcs_integration_id":Uuid::new_v4(),
        "repository_locator":format!("https://code.example/engineering/{project}.git"),
        "selection":{"allowed_references":[],"default_reference":null,"allow_exact_revision":true}}
    }),
  )
  .await;
  let repository_id = resource_id(&repository);
  let version = get_json(
    &client,
    format!("{origin}/api/v1/repositories/{repository_id}/versions/1"),
  )
  .await;
  let repository =
    json!({"project_id":project,"id":repository_id,"version":1,"definition_digest":version["definition_digest"]});
  let handoff = FlowDataSchema::new(
    "release.ticket-handoff".parse().unwrap(),
    "v1".parse().unwrap(),
    FlowValueSchema::String {
      min_bytes: 36,
      max_bytes: 36,
    },
  )
  .unwrap();
  let (triage, triage_flow) = publish(&env, &project, "triage", &handoff, None).await;
  let (downstream, _) = publish(&env, &project, "delivery", &handoff, Some(&triage_flow)).await;
  let draft = |target: &Value, enabled| {
    json!({"target":target,"repository":repository,"source":source["binding"],"enabled":enabled,"reporter":null,
    "connection":{"id":connection,"version":1,"access_version":1,"source_configuration":source["connection_configuration"],"reporter_configuration":null}})
  };
  let path = |id| format!("/api/v1/projects/{project}/factory-source-inputs/{id}/versions");
  let triage_input = Uuid::new_v4();
  let downstream_input = Uuid::new_v4();
  publish_input(&env, &project, downstream_input, None, draft(&downstream, false)).await;
  publish_input(&env, &project, triage_input, None, draft(&triage, true)).await;
  let original = wait_for_admission(&env, &path(triage_input), 1).await;
  let original_run = string(&original, "run_id");
  let before = wait_for_terminal(&env, &original_run).await;
  assert_eq!(before["issue_association"]["issue_id"], issue);
  assert_eq!(
    get_json(&client, format!("{origin}{}/1/diagnostics", path(downstream_input))).await["admissions"],
    json!([])
  );

  publish_input(&env, &project, triage_input, Some(1), draft(&triage, false)).await;
  publish_input(&env, &project, downstream_input, Some(1), draft(&downstream, true)).await;
  let admitted = wait_for_admission(&env, &path(downstream_input), 2).await;
  let run = string(&admitted, "run_id");
  assert_ne!(run, original_run);
  let after = wait_for_terminal(&env, &run).await;
  assert_eq!(after["issue_association"]["issue_id"], issue);
  let attempts = get_json(
    &client,
    format!("{origin}/api/v1/factory-runs/{run}/node-attempts?limit=100"),
  )
  .await;
  assert!(
    attempts["items"][0]["record"]["input_summary"]["incoming_digest"].is_string(),
    "automatic Delivery must consume the immutable result of the preceding triage Flow {triage_flow}"
  );
  for accepted_run in [&original_run, &run] {
    let completions = get_json(
      &client,
      format!("{origin}/api/v1/factory-runs/{accepted_run}/node-completions"),
    )
    .await;
    assert_eq!(completions["items"][0]["record"]["public_result"], project);
  }
  env.restart().await;
  let repeated = get_json(&client, format!("{origin}{}/2/diagnostics", path(downstream_input))).await;
  assert_eq!(repeated["admissions"].as_array().unwrap().len(), 1);
  assert_eq!(repeated["admissions"][0]["association"], admitted);
  env.stop().await;
  database.cleanup().await;
}

async fn publish(
  env: &source_environment::SourceEnvironment,
  project: &str,
  stage: &str,
  schema: &FlowDataSchema,
  predecessor: Option<&Value>,
) -> (Value, Value) {
  let budget = BudgetLimit::new(2, 60_000, 0, 0, 1024 * 1024).unwrap();
  let permissions = FactoryPermissionSet::deny_all();
  let gate = FlowGateProgram::new(vec![], "accepted".parse().unwrap())
    .unwrap()
    .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap())
    .unwrap()
    .binding()
    .unwrap();
  let binding = if predecessor.is_some() {
    json!({"sources":{"previous":{"source":"incoming","schema":schema.reference(),"path":""}},
      "mapping":{"value":{"mapping":"pointer","path":"/previous"}}})
  } else {
    json!({"sources":{},"mapping":{"value":{"mapping":"constant","value":project}}})
  };
  let predecessor =
    predecessor.map(|reference| json!({"definition":reference,"terminal":"ready","outcomes":["accepted"]}));
  let flow = post_management(
    &env.client,
    &env.origin,
    "/api/v1/flow-definitions",
    &format!("{stage}-{project}"),
    json!({
      "id":Uuid::new_v4(),"project_id":project,"definition":{
        "accepted_predecessor":predecessor,
        "root":{"input_schema":schema.reference(),"entry":stage,
          "nodes":[{"key":stage,"kind":"deterministic_gate","input_schema":schema.reference(),
            "outcomes":[{"key":"accepted","kind":"success","schema":schema.reference(),"public_result":true}],
            "budget":budget,"permissions":permissions,"required":true,
            "input_binding":binding,
            "gate":gate,
            "accepted_work_outcomes":["accepted"]}],
          "transitions":[{"predecessor":stage,"outcome":"accepted","target":{"kind":"terminal","key":"ready"},
            "max_repeats":0,"exhausted_terminal":null,"starts_cycle":false}],
          "terminals":[{"key":"ready","schema":schema.reference()}],"context_projections":[],"data_projections":[],
          "execution":{"budget":budget,"permissions":permissions,"max_active_nodes":1}},
      "limits":{"max_depth":2,"max_expanded_nodes":4,"max_repeat_count":1,"max_fan_out":1,"max_wip":1,
          "budget":budget,"permissions":permissions},"data_schemas":[schema],"pools":{}}
    }),
  )
  .await;
  let flow = flow["resource"]["flow"].clone();
  let policies = format!("/api/v1/projects/{project}/factory-policies");
  let launch = post_management(
    &env.client,
    &env.origin,
    &policies,
    &format!("{stage}-launch-{project}"),
    json!({
      "kind":"launch","id":Uuid::new_v4(),"definition":{"name":format!("{stage} source launch"),
        "allow_manual":false,"allow_automatic_source":true}
    }),
  )
  .await;
  let access = post_management(&env.client,&env.origin,&policies,&format!("{stage}-access-{project}"),json!({
    "kind":"access","id":Uuid::new_v4(),"definition":{"name":format!("{stage} source access"),"permissions":permissions}
  })).await;
  let configuration = post_management(&env.client,&env.origin,"/api/v1/factory-configurations",&format!("{stage}-configuration-{project}"),json!({
    "id":Uuid::new_v4(),"project_id":project,"definition":{"admission_policy":{"project_policy":launch["policy"]["policy"]["reference"]},
      "permission_ceiling":{"project_policy":access["policy"]["policy"]["reference"]},"hard_budget":budget,"wip_limits":{"max_active_runs":2,"max_active_nodes":1},
      "enabled":true,"flow":flow}
  })).await;
  (
    configuration["resource"]["configuration"]["reference"].clone(),
    flow["closure"]["root"].clone(),
  )
}

async fn publish_input(
  env: &source_environment::SourceEnvironment,
  project: &str,
  id: Uuid,
  version: Option<u32>,
  definition: Value,
) {
  let (header, precondition) = version.map_or_else(
    || ("if-none-match", "*".to_owned()),
    |v| ("if-match", format!("\"{v}\"")),
  );
  source_setup::publish(&env.client,&env.origin,&format!("/api/v1/projects/{project}/factory-source-inputs/{id}/versions"),
    &format!("{id}-{}",version.unwrap_or_default()),header,&precondition,
    json!({"intent":{"project_id":project,"group":null,"inputs":[{"id":id,"expected_version":version,"definition":definition}]}})).await;
}

async fn wait_for_admission(env: &source_environment::SourceEnvironment, path: &str, version: u32) -> Value {
  let url = format!("{}{path}/{version}/diagnostics", env.origin);
  let result = timeout(Duration::from_secs(45), async {
    loop {
      let result = get_json(&env.client, url.clone()).await;
      if result["admissions"][0]["association"].is_object() {
        return result["admissions"][0]["association"].clone();
      }
      sleep(Duration::from_millis(250)).await;
    }
  })
  .await;
  match result {
    Ok(admitted) => admitted,
    Err(_) => {
      let diagnostics = get_json(&env.client, url).await;
      let requests = env.provider.requests.lock().await;
      let paths: std::collections::BTreeSet<_> = requests
        .iter()
        .filter_map(|request| {
          let mut request_line = request.lines().next()?.split_whitespace();
          Some((request_line.next()?, request_line.next()?.split('?').next()?))
        })
        .collect();
      panic!("the selected Source admission must settle: {diagnostics}; provider requests: {paths:?}");
    }
  }
}

async fn wait_for_terminal(env: &source_environment::SourceEnvironment, run: &str) -> Value {
  timeout(Duration::from_secs(30), async {
    loop {
      let result = get_json(&env.client, format!("{}/api/v1/factory-runs/{run}", env.origin)).await;
      match result["state"].as_str() {
        Some("completed") => return result,
        Some("failed" | "cancelled") => panic!("Source Flow did not complete: {result}"),
        _ => sleep(Duration::from_millis(250)).await,
      }
    }
  })
  .await
  .expect("the admitted Source Flow must settle")
}

async fn import_issue(env: &source_environment::SourceEnvironment, project: &str, connection: &str) -> Value {
  let base = format!("{}/api/v1/projects/{project}", env.origin);
  let choices = get_json(
    &env.client,
    format!("{base}/issue-source-choices?connection_id={connection}&connection_version=1"),
  )
  .await;
  let choice = &choices["items"][0];
  let import = Uuid::new_v4();
  source_setup::publish(
    &env.client,
    &env.origin,
    &format!("/api/v1/projects/{project}/issue-source-imports/{import}/versions"),
    &import.to_string(),
    "if-none-match",
    "*",
    json!({"intent":{"id":import,"project_id":project,"expected_current_version":null,
      "definition":{"source":choice["binding"],"enabled":true,"connection":{"id":connection,"version":1,
      "access_version":1,"source_configuration":choice["connection_configuration"],"reporter_configuration":null}}}}),
  )
  .await;
  timeout(Duration::from_secs(30), async {
    loop {
      let issues = get_json(&env.client, format!("{base}/issues")).await;
      if let Some(issue) = issues["items"][0]["id"].as_str() {
        return json!(issue);
      }
      sleep(Duration::from_millis(250)).await;
    }
  })
  .await
  .expect("the native task must have its canonical Issue before triage")
}
