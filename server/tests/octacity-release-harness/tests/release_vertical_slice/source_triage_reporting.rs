//! Review finite native effects through the installed public Reporter contract.
use super::*;
use octacity_server_factory::*;

pub(super) struct InstalledReporting {
  pub(super) permissions: FactoryPermissionSet,
  budget: BudgetLimit,
}

/// Operator bootstrap reads the installed server declaration, never a task or
/// model claim. These permissions belong only to the server action; the Agent
/// installation and both reasoning profiles keep their separate authority.
pub(super) fn install(execution: &ExecutionEnvironment, catalog: &mut Value) -> InstalledReporting {
  let installation: Value =
    serde_json::from_slice(&fs::read(&execution.environment.installation.catalog).unwrap()).unwrap();
  let profiles: Value =
    serde_json::from_slice(&fs::read(installation["source_profiles_file"].as_str().unwrap()).unwrap()).unwrap();
  let writer = &profiles["reporters"][0];
  let mut permissions = serde_json::to_value(FactoryPermissionSet::deny_all()).unwrap();
  permissions["plugins"] = json!([writer["write"]["plugin"], writer["write"]["capability"]["capability"]]);
  permissions["secret_profiles"] = json!([writer["credential_profile"]]);
  permissions["network_hosts"] = writer["write"]["network_hosts"].clone();
  let refs = catalog["projects"][0]["references"].as_array_mut().unwrap();
  refs.push(json!({"kind":"node_capability","alias":"source-adapter","reference":writer["write"]["plugin"]}));
  refs.push(json!({"kind":"secret_profile","alias":writer["credential_profile"],"reference":ImmutableReference::new(
    writer["credential_profile"].as_str().unwrap().parse().unwrap(),writer["version"].as_str().unwrap().parse().unwrap(),FactoryDigest::content_sha256(&serde_json::to_vec(writer).unwrap()))}));
  InstalledReporting {
    permissions: serde_json::from_value(permissions).unwrap(),
    budget: serde_json::from_value(writer["issue_delivery"]["budget"].clone()).unwrap(),
  }
}

pub(super) async fn select(
  execution: &ExecutionEnvironment,
  connection: &str,
  installed: &InstalledReporting,
) -> Value {
  let project = &execution.resources.project;
  let choices = get_json(
    &execution.environment.client,
    format!(
      "{}/api/v1/projects/{project}/factory-source-choices?connection_id={connection}&connection_version=1",
      execution.environment.origin
    ),
  )
  .await;
  let items = choices["items"].as_array().unwrap();
  let source = items
    .iter()
    .find(|item| item["purpose"] == "task_read" && item["locator"] == "owner/tickets")
    .unwrap();
  let reporter = items.iter().find(|item| item["purpose"] == "task_update").unwrap();
  let action = &reporter["reporting"]["action"];
  assert!(action.is_object(), "use the exact installed native action contract");
  let effect = json!({"status":"triage-ready","add":["ready / review"],"remove":[],"state":"open"});
  let manifest: Value = serde_json::from_slice(include_bytes!(
    "../../../../../config/factory/presets/triage/classification-gate-v1.json"
  ))
  .unwrap();
  let outcomes: serde_json::Map<String, Value> = manifest["nodes"][0]["outcomes"]
    .as_array()
    .unwrap()
    .iter()
    .map(|outcome| {
      let key = outcome["key"].as_str().unwrap();
      (
        key.to_owned(),
        if key == "requirements" {
          effect.clone()
        } else {
          Value::Null
        },
      )
    })
    .collect();
  json!({"reporter":reporter["binding"],"configuration":reporter["connection_configuration"],"permissions":installed.permissions,"mapping":{"statuses":[effect]},
  "selection":{"id":Uuid::new_v4(),"definition":{
    "target":{"source":source["binding"],"reporter":reporter["binding"],
      "connection":{"id":connection,"version":1,"access_version":1,"source_configuration":source["connection_configuration"],"reporter_configuration":reporter["connection_configuration"]},
      "policy":reporter["reporting"]["editable"]},
    "outcomes":outcomes,"dimensions":[],"receipt":action["output"],"action_outcome":action["outcome"]["key"],
    "budget":installed.budget,
    "execution":FlowExecutionPolicy::new(BudgetLimit::new(8,60_000,0,0,installed.budget.max_output_bytes().checked_mul(8).unwrap()).unwrap(),installed.permissions.clone(),1).unwrap(),
    "retry":{"max_attempts":2,"retry_delay_millis":1000}
  }}})
}

pub(super) fn flow_permissions(permissions: &FactoryPermissionSet, selected: Option<&Value>) -> Value {
  let mut result = serde_json::to_value(permissions).unwrap();
  if let Some(selected) = selected {
    for field in ["plugins", "secret_profiles", "network_hosts"] {
      result[field]
        .as_array_mut()
        .unwrap()
        .extend(selected["permissions"][field].as_array().unwrap().iter().cloned());
    }
  }
  result
}
