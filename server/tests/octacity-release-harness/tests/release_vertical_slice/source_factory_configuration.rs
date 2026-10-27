//! Public Flow/configuration/Repository publication; no direct store seeding or alternate executor.
use super::*;
use octacity_server_factory::*;

pub(super) fn reference(name: &str) -> ImmutableReference {
  ImmutableReference::new(
    name.parse().unwrap(),
    "v1".parse().unwrap(),
    FactoryDigest::content_sha256(name.as_bytes()),
  )
}

pub(super) fn catalog(projects: &[String]) -> Value {
  json!({"version":1,"codex":[],"projects":projects.iter().map(|project|json!({"project_id":project,
    "references":[{"kind":"admission_policy","alias":"source-admission","reference":reference("source-admission")},
      {"kind":"permission_ceiling","alias":"source-ceiling","reference":reference("source-ceiling")},
      {"kind":"node_capability","alias":"issue-action","reference":reference("issue-action")}],
    "build_configurations":[]})).collect::<Vec<_>>(),
    "issue_status_actions":[{"identity":reference("issue-action"),"principal":"packaged-nginx"}]})
}

pub(super) async fn publish(
  environment: &source_environment::SourceEnvironment,
  project: &str,
  action: Option<Value>,
) -> (Value, Value) {
  let client = &environment.client;
  let origin = &environment.origin;
  let identity = Uuid::new_v4();
  let schema = FlowDataSchema::new(
    "source.input".parse().unwrap(),
    "v1".parse().unwrap(),
    FlowValueSchema::Object {
      fields: Default::default(),
    },
  )
  .unwrap();
  let budget = BudgetLimit::new(8, 600_000, 0, 0, 131072).unwrap();
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: if action.is_some() {
      vec![reference("issue-action")]
    } else {
      vec![]
    },
    ..Default::default()
  })
  .unwrap();
  let output = action.as_ref().map_or_else(
    || schema.clone(),
    |_| {
      FlowDataSchema::new(
        "issue.receipt".parse().unwrap(),
        "v1".parse().unwrap(),
        FlowValueSchema::Object {
          fields: std::collections::BTreeMap::from([(
            "receipt".parse().unwrap(),
            FlowFieldSchema::required(FlowValueSchema::String {
              min_bytes: 64,
              max_bytes: 64,
            }),
          )]),
        },
      )
      .unwrap()
    },
  );
  let mut node = json!({"key":"reviewed-disposition","kind":"human_gate","input_schema":schema.reference(),
    "outcomes":[{"key":"approved","kind":"success","schema":output.reference()}],
    "budget":budget,"permissions":permissions,"required":true,"subflow":null,
    "input_binding":{"sources":{},"mapping":{"value":{"mapping":"constant","value":{}}}}});
  if let Some(action) = action {
    node["kind"] = json!("trusted_action");
    node["action"] = json!({"plugin":reference("issue-action"),"action":"transition_issue","parameters":action});
  }
  let schemas = if output == schema {
    vec![schema.clone()]
  } else {
    vec![schema.clone(), output.clone()]
  };
  let flow = post_management(client,origin,"/api/v1/flow-definitions",&format!("{identity}-flow"),json!({
    "id":Uuid::new_v4(),"project_id":project,"definition":{
      "root":{"input_schema":schema.reference(),"entry":"reviewed-disposition","nodes":[node],
        "transitions":[{"predecessor":"reviewed-disposition","outcome":"approved","target":{"kind":"terminal","key":"finished"},
          "max_repeats":0,"exhausted_terminal":null,"starts_cycle":false}],
        "terminals":[{"key":"finished","schema":output.reference()}],"context_projections":[],"data_projections":[],
        "execution":{"budget":budget,"permissions":permissions,"max_active_nodes":1}},
      "limits":{"max_depth":2,"max_expanded_nodes":4,"max_repeat_count":1,"max_fan_out":1,"max_wip":4,
        "budget":budget,"permissions":permissions},"data_schemas":schemas,"pools":{}}
  })).await;
  let configuration = post_management(client,origin,"/api/v1/factory-configurations",&format!("{identity}-factory"),json!({
    "id":Uuid::new_v4(),"project_id":project,"definition":{"admission_policy":"source-admission",
      "permission_ceiling":"source-ceiling","hard_budget":budget,"wip_limits":{"max_active_runs":8,"max_active_nodes":4},
      "enabled":true,"flow":flow["resource"]["flow"]}
  })).await;
  let repository = post_management(client,origin,"/api/v1/repositories",&format!("{identity}-code"),json!({
    "project_id":project,"name":format!("Independent code {identity}"),"definition":{"vcs_integration_id":Uuid::new_v4(),
      "repository_locator":format!("https://code.example/engineering/{project}.git"),
      "selection":{"allowed_references":[],"default_reference":null,"allow_exact_revision":true}}
  })).await;
  let repository_id = resource_id(&repository);
  let repository = get_json(
    client,
    format!("{origin}/api/v1/repositories/{repository_id}/versions/1"),
  )
  .await;
  (
    configuration["resource"]["configuration"]["reference"].clone(),
    json!({"project_id":project,"id":repository_id,"version":1,"definition_digest":repository["definition_digest"]}),
  )
}
