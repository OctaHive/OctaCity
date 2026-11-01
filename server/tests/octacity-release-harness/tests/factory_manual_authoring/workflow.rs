//! Operator-owned documents, assignments and graph authored solely through HTTP.
use super::*;
use octacity_server_application::FactoryCodexBuildProfile;

const ASSESS: &str = "Assess Project fit. Retain duplicate uncertainty without inventing search evidence.";
const CLASSIFY: &str = "Classify only the accepted eligible ticket using the selected component and priority guidance.";

async fn document(client: &Client, origin: &str, project: &str, name: &str, content: &str) -> Value {
  let published = post_management(
    client,
    origin,
    &format!("/api/v1/projects/{project}/input-documents"),
    name,
    json!({"name":name,"content":content}),
  )
  .await;
  let read = get_json(
    client,
    format!(
      "{origin}/api/v1/projects/{project}/input-documents/{}",
      published["artifact"]["artifact_id"].as_str().unwrap()
    ),
  )
  .await;
  assert_eq!(read["content"], content);
  published["artifact"].clone()
}
async fn schema(client: &Client, origin: &str, project: &str, name: &str, shape: FlowValueSchema) -> Value {
  post_management(
    client,
    origin,
    "/api/v1/flow-data-schemas/validate",
    name,
    json!({"project_id":project,"identity":name,"version":"v1","shape":shape}),
  )
  .await
}
fn object(fields: &[(&str, FlowValueSchema)]) -> FlowValueSchema {
  FlowValueSchema::Object {
    fields: fields
      .iter()
      .map(|(name, shape)| (name.parse().unwrap(), FlowFieldSchema::required(shape.clone())))
      .collect(),
  }
}
fn text() -> FlowValueSchema {
  FlowValueSchema::String {
    min_bytes: 1,
    max_bytes: 256,
  }
}
fn constant(value: Value) -> Value {
  json!({"value":{"mapping":"constant","value":value}})
}
fn record(node: &str, outcome: &str, path: &str) -> Value {
  json!({"sources":{"selected":{"source":"record","node":node,"outcome":outcome,"view":"payload","path":path}},"mapping":{"value":{"mapping":"pointer","path":"/selected"}}})
}
fn context(entries: &[(&str, &str, &Value)]) -> Value {
  json!({"entries":entries.iter().map(|(alias,kind,artifact)|(alias.to_string(),json!({"source_kind":kind,"source":constant(json!({"kind":"artifact","value":artifact}))}))).collect::<BTreeMap<_,_>>()})
}
fn node(name: &str, kind: &str, input: &Value, outcomes: &[(&str, &Value)], permissions: &Value) -> Value {
  json!({"key":name,"kind":kind,"input_schema":input["reference"],"outcomes":outcomes.iter().map(|(key,schema)|json!({"key":key,"kind":"success","schema":schema["reference"],"public_result":true})).collect::<Vec<_>>(),"budget":BudgetLimit::new(1,120000,1000,1000,16*1024*1024).unwrap(),"permissions":permissions,"required":true,"subflow":null,"code":{"kind":"none"}})
}
fn edge(from: &str, outcome: &str, kind: &str, to: &str) -> Value {
  json!({"predecessor":from,"outcome":outcome,"target":{"kind":kind,"key":to},"max_repeats":0,"exhausted_terminal":null,"starts_cycle":false})
}
fn select(node: &mut Value, publication: &Value, build: Value) {
  node["profile_selection"] = json!({"role":publication["role"]["reference"],"candidates":{"chosen":{"profile":publication["profiles"][0],"build":build}},"policy":{"mode":"manual","candidate":"chosen"},"failure":{"kind":"manual_selection"},"selection_budget":BudgetUsage::default()});
}
async fn resolve(client: &Client, origin: &str, project: &str, role: &Value, outcome: &str) -> Value {
  let binding = post_management(
    client,
    origin,
    &format!("/api/v1/projects/{project}/factory-execution-bindings/resolve"),
    outcome,
    json!({"profile":role["profiles"][0]["reference"],"capability":"manual-runtime","result_outcome":outcome}),
  )
  .await;
  assert!(binding["incompatibility"].is_null(), "{binding}");
  binding["build"].clone()
}

pub(super) async fn run(
  client: &Client,
  origin: &str,
  resources: &installation::Resources,
  profile: &FactoryCodexBuildProfile,
  first: &Value,
) -> String {
  let project = &resources.project;
  let task = document(
    client,
    origin,
    project,
    "Ticket",
    "ticket-request-canary: add copying of an Issue identifier from its card",
  )
  .await;
  let goals = document(
    client,
    origin,
    project,
    "Project goals",
    "project-goals-canary: improve the operator console for software work coordination",
  )
  .await;
  let components = document(
    client,
    origin,
    project,
    "Components",
    "component-guide-canary: console means operator-facing interface",
  )
  .await;
  let priorities = document(
    client,
    origin,
    project,
    "Priorities",
    "priority-guide-canary: normal means a useful non-urgent improvement",
  )
  .await;
  let criteria = document(
    client,
    origin,
    project,
    "Process completion",
    "process-criteria-canary: retain the assessment and classification, or an explicit review outcome",
  )
  .await;
  let second = installation::publish_role(client, origin, resources, profile, "manual-second-agent").await;
  let boolean = schema(client, origin, project, "manual.input", FlowValueSchema::Boolean).await;
  let assessment = schema(
    client,
    origin,
    project,
    "manual.assessment",
    object(&[
      ("eligible", FlowValueSchema::Boolean),
      ("summary", text()),
      ("duplicate_search", text()),
    ]),
  )
  .await;
  let summary = schema(client, origin, project, "manual.summary", text()).await;
  let classification = schema(
    client,
    origin,
    project,
    "manual.classification",
    object(&[("component", text()), ("priority", text())]),
  )
  .await;
  let permissions = json!(profile.project_permissions);
  let deny = json!(FactoryPermissionSet::deny_all());
  let mut inspect = node(
    "inspect",
    "reasoning",
    &boolean,
    &[("assessed", &assessment)],
    &permissions,
  );
  inspect["task"] = json!([ASSESS]);
  inspect["input_binding"] = json!({"sources":{},"mapping":constant(json!(true))});
  inspect["context_binding"] = context(&[("task", "task", &task), ("goals", "specification", &goals)]);
  select(
    &mut inspect,
    first,
    resolve(client, origin, project, first, "assessed").await,
  );
  let mut eligible = node(
    "eligibility-check",
    "deterministic_gate",
    &assessment,
    &[("eligible", &summary), ("review", &summary)],
    &deny,
  );
  eligible["input_binding"] = record("inspect", "assessed", "");
  eligible["gate"]=post_management(client,origin,&format!("/api/v1/projects/{project}/factory-gate-programs/validate"),"eligibility-program",json!({"program":{
    "rules":[{"when":{"operation":"compare","left":{"source":"path","path":"/eligible"},"comparison":"equal","right":{"source":"literal","value":true}},"outcome":"eligible"}],"fallback":"review",
    "outputs":{"eligible":{"value":{"mapping":"pointer","path":"/summary"}},"review":{"value":{"mapping":"pointer","path":"/summary"}}}
  }})).await;
  let mut classify = node(
    "route",
    "reasoning",
    &summary,
    &[("classified", &classification)],
    &permissions,
  );
  classify["task"] = json!([CLASSIFY]);
  classify["input_binding"] = record("eligibility-check", "eligible", "");
  classify["context_binding"] = context(&[
    ("task", "task", &task),
    ("components", "specification", &components),
    ("priorities", "specification", &priorities),
  ]);
  select(
    &mut classify,
    &second,
    resolve(client, origin, project, &second, "classified").await,
  );
  let mut ready = node(
    "route-check",
    "deterministic_gate",
    &classification,
    &[("ready", &classification), ("review", &classification)],
    &deny,
  );
  ready["input_binding"] = record("route", "classified", "");
  ready["gate"]=post_management(client,origin,&format!("/api/v1/projects/{project}/factory-gate-programs/validate"),"routing-program",json!({"program":{
    "rules":[{"when":{"operation":"compare","left":{"source":"path","path":"/priority"},"comparison":"equal","right":{"source":"literal","value":"normal"}},"outcome":"ready"}],"fallback":"review",
    "outputs":{"ready":{"value":{"mapping":"pointer","path":""}},"review":{"value":{"mapping":"pointer","path":""}}}
  }})).await;
  let budget = BudgetLimit::new(8, 600000, 4000, 4000, 64 * 1024 * 1024).unwrap();
  let draft = json!({"root":{"input_schema":boolean["reference"],"entry":"inspect","nodes":[inspect,eligible,classify,ready],"transitions":[edge("inspect","assessed","node","eligibility-check"),edge("eligibility-check","eligible","node","route"),edge("eligibility-check","review","terminal","assessment-review"),edge("route","classified","node","route-check"),edge("route-check","ready","terminal","finished"),edge("route-check","review","terminal","routing-review")],
    "terminals":[{"key":"finished","schema":classification["reference"]},{"key":"assessment-review","schema":summary["reference"]},{"key":"routing-review","schema":classification["reference"]}],"context_projections":[],"data_projections":[],"code_sources":{},
    "process":{"criteria":{"artifact":criteria,"recipients":["inspect","route"]}},"execution":{"budget":budget,"permissions":permissions,"max_active_nodes":1}},
    "limits":{"max_depth":1,"max_expanded_nodes":8,"max_repeat_count":1,"max_fan_out":1,"max_wip":1,"budget":budget,"permissions":permissions},"data_schemas":[boolean,assessment,summary,classification],"pools":{}});
  let id = Uuid::new_v4();
  let request = json!({"id":id,"project_id":project,"definition":draft});
  post_management(
    client,
    origin,
    "/api/v1/flow-definitions/validate",
    "review-flow",
    request.clone(),
  )
  .await;
  assert_eq!(
    get_json(client, format!("{origin}/api/v1/projects/{project}/builds?limit=100")).await["items"],
    json!([])
  );
  let flow = post_management(client, origin, "/api/v1/flow-definitions", "publish-flow", request).await;
  let config=post_management(client,origin,"/api/v1/factory-configurations","publish-factory",json!({"id":Uuid::new_v4(),"project_id":project,"definition":{"admission_policy":"manual-admission","permission_ceiling":"manual-ceiling","hard_budget":budget,"wip_limits":{"max_active_runs":1,"max_active_nodes":1},"enabled":true,"flow":flow["resource"]["flow"]}})).await;
  let admitted=post_management(client,origin,&format!("/api/v1/projects/{project}/factory-work"),"admit-ticket",json!({
    "configuration_id":config["resource"]["configuration"]["reference"]["id"],"configuration_version":1,"external_identity":"manual/ticket-1","source":{"kind":"project_input_v1"},"artifacts":{"task":task,"acceptance":criteria,"specifications":[]},"classification":{"priority":40,"risk":"low","metadata":{}}
  })).await;
  string(&admitted, "factory_run_id")
}
