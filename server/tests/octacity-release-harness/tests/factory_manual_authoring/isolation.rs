//! Unrelated bulletin workflow: explicit scalar handoffs, selected documents and code.
use super::workflow::*;
use super::*;
use octacity_server_application::FactoryCodexBuildProfile;

pub(super) struct Authored {
  pub run: String,
  pub documents: [Value; 3],
  pub task: Value,
  pub criteria: Value,
  pub code: Vec<(String, String)>,
}

pub(super) async fn run(
  client: &Client,
  origin: &str,
  resources: &installation::Resources,
  profile: &FactoryCodexBuildProfile,
  role: &Value,
  code: Option<(&code::CodeInstallation, &code::CodeInstallation)>,
) -> Authored {
  let project = &resources.project;
  if let Some((a, b)) = code {
    assert_ne!(a.repository, b.repository);
    assert_ne!(a.revision, b.revision);
  } else {
    assert_eq!(
      get_json(
        client,
        format!("{origin}/api/v1/projects/{project}/repositories?limit=100")
      )
      .await["items"],
      json!([])
    );
  }
  let task = document(
    client,
    origin,
    project,
    "Bulletin request",
    "bulletin-request-canary: prepare a translated bulletin",
  )
  .await;
  let criteria = document(
    client,
    origin,
    project,
    "Bulletin completion",
    "bulletin-criteria-canary: retain an independently audited translation",
  )
  .await;
  let mut documents = Vec::new();
  for (name, content) in [
    ("Style", "style-guide-canary"),
    ("Glossary", "glossary-canary"),
    ("Checklist", "checklist-canary"),
  ] {
    documents.push(document(client, origin, project, name, content).await);
  }
  // This document is available to the Project but deliberately attached to no step.
  document(
    client,
    origin,
    project,
    "Private background",
    "bulletin-unselected-project-canary",
  )
  .await;
  let boolean = schema(client, origin, project, "bulletin.input", FlowValueSchema::Boolean).await;
  let scalar = schema(client, origin, project, "bulletin.scalar", text()).await;
  let caption = schema(
    client,
    origin,
    project,
    "bulletin.caption",
    object(&[("caption", text()), ("private_note", text())]),
  )
  .await;
  let translation = schema(
    client,
    origin,
    project,
    "bulletin.translation",
    object(&[("translation", text()), ("private_note", text())]),
  )
  .await;
  let approved = schema(
    client,
    origin,
    project,
    "bulletin.approved",
    object(&[("approved", FlowValueSchema::Boolean)]),
  )
  .await;
  let permissions = json!(code.map_or(profile, |(a, _)| &a.profile).project_permissions);
  let mut roles = vec![role.clone()];
  if let Some((a, b)) = code {
    roles.push(installation::publish_role(client, origin, resources, &a.profile, "bulletin-a-agent").await);
    roles.push(installation::publish_role(client, origin, resources, &b.profile, "bulletin-b-agent").await);
  }
  let mut nodes = Vec::new();
  for (index, (key, assignment, input, output)) in [
    ("compose", "Compose a bulletin.", &boolean, &caption),
    ("translate", "Translate the accepted caption.", &scalar, &translation),
    ("audit", "Audit the accepted translation.", &scalar, &approved),
  ]
  .into_iter()
  .enumerate()
  {
    let mut step = node(key, "reasoning", input, &[("done", output)], &permissions);
    step["task"] = json!([assignment]);
    step["input_binding"] = match index {
      0 => json!({"sources":{},"mapping":constant(json!(true))}),
      1 => record("compose", "done", "/caption"),
      _ => record("translate", "done", "/translation"),
    };
    step["context_binding"] = context(&[("guide", "specification", &documents[index])]);
    step["context_binding"]["work_task"] = json!("task");
    let selected = if code.is_some() { &roles[index] } else { role };
    let capability = match (code.is_some(), index) {
      (true, 1) => "manual-code-runtime",
      (true, 2) => "manual-code-runtime-1",
      _ => "manual-runtime",
    };
    let binding = post_management(
      client,
      origin,
      &format!("/api/v1/projects/{project}/factory-execution-bindings/resolve"),
      &format!("resolve-bulletin-{key}"),
      json!({"profile":selected["profiles"][0]["reference"],"capability":capability,"result_outcome":"done"}),
    )
    .await;
    assert!(binding["incompatibility"].is_null(), "{binding}");
    select(&mut step, selected, binding["build"].clone());
    if code.is_some() && index > 0 {
      step["task"] = json!([
        assignment,
        if index == 1 {
          "Read only repository A."
        } else {
          "Read only repository B."
        }
      ]);
      step["code"] = json!({"kind":"source","source":if index == 1 { "a" } else { "b" },"access":"read_only","scope":{"kind":"selected","entries":[{"kind":"path","path":"README.md"}]},"revision":{"kind":"admitted_base"}});
    } else {
      step["permissions"] = json!(profile.project_permissions);
    }
    nodes.push(step);
  }
  let budget = BudgetLimit::new(8, 600000, 4000, 4000, 64 * 1024 * 1024).unwrap();
  let mut draft = json!({"root":{"input_schema":boolean["reference"],"entry":"compose","nodes":nodes,
    "transitions":[edge("compose","done","node","translate"),edge("translate","done","node","audit"),edge("audit","done","terminal","finished")],
    "terminals":[{"key":"finished","schema":approved["reference"]}],"context_projections":[],"data_projections":[],"code_sources":{},
    "process":{"criteria":{"artifact":criteria,"recipients":["compose","translate","audit"]}},"execution":{"budget":budget,"permissions":permissions,"max_active_nodes":1}},
    "limits":{"max_depth":1,"max_expanded_nodes":8,"max_repeat_count":1,"max_fan_out":1,"max_wip":1,"budget":budget,"permissions":permissions},
    "data_schemas":[boolean,scalar,caption,translation,approved],"pools":{}});
  if let Some((a, b)) = code {
    for (key, source) in [("a", a), ("b", b)] {
      let repository = get_json(
        client,
        format!("{origin}/api/v1/repositories/{}/versions/1", source.repository),
      )
      .await;
      assert!(repository["definition_digest"].is_string());
      draft["root"]["code_sources"][key] = json!({"repository":{"project_id":project,"id":source.repository,"version":1,"definition_digest":repository["definition_digest"]},"selection":{"kind":"exact_revision","value":source.revision},"access":"read_only","scope":{"kind":"selected","entries":[{"kind":"path","path":"README.md"}]}});
    }
  }
  reject_unselected_authority(client, origin, project, &draft, code.is_some()).await;
  let request = json!({"id":Uuid::new_v4(),"project_id":project,"definition":draft});
  post_management(
    client,
    origin,
    "/api/v1/flow-definitions/validate",
    "validate-bulletin",
    request.clone(),
  )
  .await;
  let flow = post_management(client, origin, "/api/v1/flow-definitions", "publish-bulletin", request).await;
  let configuration = post_management(client,origin,"/api/v1/factory-configurations","configure-bulletin",json!({"id":Uuid::new_v4(),"project_id":project,"definition":{"admission_policy":"manual-admission","permission_ceiling":"manual-ceiling","hard_budget":budget,"wip_limits":{"max_active_runs":1,"max_active_nodes":1},"enabled":true,"flow":flow["resource"]["flow"]}})).await;
  let admitted = post_management(client,origin,&format!("/api/v1/projects/{project}/factory-work"),"admit-bulletin",json!({"configuration_id":configuration["resource"]["configuration"]["reference"]["id"],"configuration_version":1,"external_identity":"bulletin/1","source":{"kind":"project_input_v1"},"artifacts":{"task":task,"acceptance":criteria,"specifications":[]},"classification":{"priority":40,"risk":"low","metadata":{}}})).await;
  Authored {
    run: string(&admitted, "factory_run_id"),
    documents: documents.try_into().unwrap(),
    task,
    criteria,
    code: code.map_or_else(Vec::new, |(a, b)| {
      vec![
        (a.repository.clone(), a.revision.clone()),
        (b.repository.clone(), b.revision.clone()),
      ]
    }),
  }
}

pub(super) async fn completed(client: &Client, origin: &str, authored: &Authored) -> Value {
  let mut retained = serde_json::Map::new();
  for collection in [
    "node-completions",
    "calls",
    "builds",
    "work",
    "candidates",
    "evidence",
    "delivery",
  ] {
    let page = get_json(
      client,
      format!("{origin}/api/v1/factory-runs/{}/{collection}?limit=100", authored.run),
    )
    .await;
    assert!(page["next_cursor"].is_null());
    retained.insert(collection.into(), page["items"].clone());
  }
  for collection in ["candidates", "evidence", "delivery"] {
    assert_eq!(
      retained[collection],
      json!([]),
      "model data grants no verified facts or delivery authority"
    );
  }
  let completions = retained["node-completions"].as_array().unwrap();
  assert_eq!(completions.len(), 3);
  for (key, expected) in [
    (
      "compose",
      json!({"caption":"selected-caption","private_note":"compose-unselected-canary"}),
    ),
    (
      "translate",
      json!({"translation":"selected-translation","private_note":"translate-unselected-canary"}),
    ),
    ("audit", json!({"approved":true})),
  ] {
    let record = &completions.iter().find(|row| row["record"]["node_key"] == key).unwrap()["record"];
    assert_eq!(record["outcome"], "done");
    assert_eq!(record["public_result"], expected);
  }
  let calls = retained["calls"].as_array().unwrap();
  let builds = retained["builds"].as_array().unwrap();
  assert_eq!(calls.len(), 3);
  assert_eq!(builds.len(), 3);
  let mut receipts = serde_json::Map::new();
  let mut contexts = std::collections::BTreeSet::new();
  let mut inputs = std::collections::BTreeSet::new();
  let mut identities = std::collections::BTreeSet::new();
  let mut jobs = std::collections::BTreeSet::new();
  for call in calls {
    assert_eq!(
      call["record"]["profile"]["model_or_tool"],
      calls[0]["record"]["profile"]["model_or_tool"]
    );
    if authored.code.is_empty() {
      assert_eq!(
        call["record"]["profile"]["selected_profile"],
        calls[0]["record"]["profile"]["selected_profile"]
      );
    }
  }
  for link in builds {
    let link = &link["record"];
    assert!(identities.insert(string(link, "build_id")));
    assert_eq!(link["jobs"].as_array().unwrap().len(), 1);
    assert!(jobs.insert(link["jobs"][0].as_str().unwrap()));
    let call = &calls
      .iter()
      .find(|row| row["record"]["node"]["id"] == link["node_attempt_id"])
      .unwrap()["record"];
    assert!(contexts.insert(string(&call["profile"], "context_digest")));
    assert!(inputs.insert(string(&call["node"], "input_digest")));
    let build = get_json(client, format!("{origin}/api/v1/builds/{}", string(link, "build_id"))).await;
    assert_eq!(build["state"], "succeeded");
    let step = call["node"]["node_key"].as_str().unwrap();
    let index = match step {
      "compose" => 0,
      "translate" => 1,
      "audit" => 2,
      _ => panic!("unexpected step"),
    };
    if index > 0 && !authored.code.is_empty() {
      assert_eq!(build["repository_id"], authored.code[index - 1].0);
      assert_eq!(build["immutable_revision"], authored.code[index - 1].1);
    } else {
      assert_eq!(build["source"], json!({"kind":"source_free_v1"}));
      for field in ["repository_id", "immutable_revision"] {
        assert!(build.get(field).is_none());
      }
    }
    assert_eq!(build["factory_origin"]["input_digest"], call["node"]["input_digest"]);
    let trace = verification::output_bytes(client, origin, &string(link, "build_id"), "codex-run-trace").await;
    let events: Vec<Value> = std::str::from_utf8(&trace)
      .unwrap()
      .lines()
      .map(|line| serde_json::from_str(line).unwrap())
      .collect();
    let receipt = events
      .iter()
      .find_map(|row| row["event"]["item"].get("input_receipt"))
      .expect("finite VM probe must retain the exact execution package");
    let package: FactoryRoleExecutionPackage = serde_json::from_value(receipt.clone()).unwrap();
    let context = &package.inputs().context;
    assert_eq!(json!(context.id()), call["profile"]["context_id"]);
    assert_eq!(json!(context.digest().unwrap()), call["profile"]["context_digest"]);
    assert_eq!(json!(package.inputs().task.node_attempt_id()), call["node"]["id"]);

    let actual: BTreeMap<_, _> = context
      .entries()
      .iter()
      .filter_map(|entry| match entry.source() {
        FactoryContextReference::Artifact(artifact) => Some((
          artifact.artifact_id().to_string(),
          serde_json::to_value(artifact).unwrap(),
        )),
        _ => None,
      })
      .collect();
    let expected = [&authored.task, &authored.criteria, &authored.documents[index]]
      .into_iter()
      .map(|artifact| (string(artifact, "artifact_id"), artifact.clone()))
      .collect();
    assert_eq!(actual, expected, "only exact selected documents may enter Context");
    assert_eq!(context.entries().len(), 3);
    if index > 0 && !authored.code.is_empty() {
      let subject = package.inputs().task.subject().exact().unwrap();
      assert_eq!(subject.repository_id().to_string(), authored.code[index - 1].0);
      assert_eq!(subject.base_revision().as_str(), authored.code[index - 1].1);
    } else {
      assert!(matches!(
        package.inputs().task.subject(),
        FactoryTaskSubject::WorkInputV1(_)
      ));
    }
    let input = events
      .iter()
      .find_map(|row| row["event"]["item"].get("node_receipt"))
      .expect("retained exact node input probe");
    assert_eq!(input["input_digest"], call["node"]["input_digest"]);
    assert_eq!(input["input"]["node"], step);
    assert_eq!(input["input"]["work_task"], authored.task);
    assert_eq!(
      input["input"]["payload"]["value"],
      match index {
        0 => json!(true),
        1 => json!("selected-caption"),
        _ => json!("selected-translation"),
      }
    );
    if index == 0 {
      assert!(input["input"].get("sources").is_none());
    } else {
      let predecessor = if index == 1 { "compose" } else { "translate" };
      let result = &completions
        .iter()
        .find(|row| row["record"]["node_key"] == predecessor)
        .unwrap()["record"];
      assert_eq!(
        input["input"]["sources"],
        json!([{"node_attempt_id":result["node_attempt_id"],"digest":result["output_digest"]}])
      );
    }
    receipts.insert(format!("input-{step}"), input.clone());
  }
  assert_eq!(retained["work"][0]["record"]["subject"]["kind"], "project_input_v1");
  retained.extend(receipts);
  Value::Object(retained)
}

/// Review and publication must reject unsupported grants and foreign retained producers
/// before a Job or immutable business definition is created.
async fn reject_unselected_authority(client: &Client, origin: &str, project: &str, draft: &Value, repositories: bool) {
  let mut unsupported = draft.clone();
  unsupported["root"]["nodes"][0]["permissions"]["network_hosts"] = json!(["undeclared.example"]);
  let mut foreign = draft.clone();
  foreign["root"]["nodes"][2]["input_binding"]["sources"]["selected"]["factory_run_id"] = json!(Uuid::new_v4());
  let mut cases = vec![("unsupported-network", unsupported), ("foreign-run-evidence", foreign)];
  if repositories {
    for (name, field, value) in [
      (
        "foreign-candidate-evidence",
        "revision",
        json!({"kind":"accepted_candidate","node":"translate","outcome":"done"}),
      ),
      ("undeclared-source", "source", json!("not-declared")),
      ("unsupported-source-write", "access", json!("read_write")),
      ("unsupported-source-scope", "scope", json!({"kind":"repository"})),
    ] {
      let mut invalid = draft.clone();
      invalid["root"]["nodes"][2]["code"][field] = value;
      cases.push((name, invalid));
    }
  }
  for (name, definition) in cases {
    for endpoint in ["/api/v1/flow-definitions/validate", "/api/v1/flow-definitions"] {
      let response = client
        .post(format!("{origin}{endpoint}"))
        .header("idempotency-key", name)
        .json(&json!({"id":Uuid::new_v4(),"project_id":project,"definition":definition}))
        .send()
        .await
        .unwrap();
      assert!(
        matches!(response.status().as_u16(), 400 | 403 | 409 | 422),
        "{name} must be rejected before publication: {}",
        response.status()
      );
    }
  }
  for collection in ["flow-definitions", "factory-runs", "builds"] {
    assert_eq!(
      get_json(
        client,
        format!("{origin}/api/v1/projects/{project}/{collection}?limit=100")
      )
      .await["items"],
      json!([])
    );
  }
}
