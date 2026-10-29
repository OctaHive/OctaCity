//! Native producer metadata is executable owner policy, independent of prior published Flows.
use super::*;

fn bound_status_parameters(plugin: &ImmutableReference) -> serde_json::Value {
  let source = WorkSourceBinding {
    installation: reference("tracker"),
    capability: reference("reader"),
    configuration: reference("reader-settings"),
    resource: reference("tickets"),
  };
  let mut writer = source.clone();
  writer.capability = plugin.clone();
  writer.configuration = reference("writer-settings");
  serde_json::json!({"status":"reviewed", "outbox":{"max_attempts":2,"retry_delay_millis":100}, "source_update": {
    "source":source, "reporter":writer,
    "connection":{"id":octacity_server_domain::IntegrationId::generate(),"version":1,"access_version":1,"source_configuration":source.configuration,"reporter_configuration":writer.configuration},
    "effects":{"status":"reviewed","add":[],"remove":[],"state":null}
  }})
}

#[test]
fn installed_work_status_contract_defines_its_exact_closed_receipt_without_a_published_flow() {
  let identity = reference("installed-reporter");
  let contract = FactoryWorkStatusActionContract::new(identity.clone()).unwrap();
  let reporter = reporter();
  let action = FactoryWorkStatusAction::installed(identity.clone(), reporter.clone()).unwrap();
  assert_eq!(contract.plugin, identity);
  assert_eq!(contract.action.as_str(), "set_status");
  assert_eq!(contract.outcome.key().as_str(), "queued");
  assert_eq!(contract.outcome.schema(), contract.output.reference());
  let binding = FlowActionBinding::new(
    identity,
    contract.action.clone(),
    bound_status_parameters(&contract.plugin),
  )
  .unwrap();
  action
    .validate(&binding, std::slice::from_ref(&contract.outcome))
    .unwrap();
  let direct = FlowActionBinding::new(
    contract.plugin.clone(),
    contract.action.clone(),
    serde_json::json!({"status":"reviewed"}),
  )
  .unwrap();
  assert!(
    action
      .validate(&direct, std::slice::from_ref(&contract.outcome))
      .is_err(),
    "the installed queued producer requires the existing ordered outbox"
  );
  assert!(FlowPayload::new(&contract.output, serde_json::json!({"receipt":"ab".repeat(32)})).is_ok());
  for invalid in [
    serde_json::json!({"receipt":"short"}),
    serde_json::json!({"receipt":"ab".repeat(32),"body":"undeclared private data"}),
  ] {
    assert!(FlowPayload::new(&contract.output, invalid).is_err());
  }
  assert!(
    reporter.requests.lock().unwrap().is_empty(),
    "reading and validating the native contract performs no write"
  );
}

#[test]
fn installed_source_producer_requires_explicit_source_and_managed_connection_pins() {
  let plugin = reference("installed-reporter");
  let contract = FactoryWorkStatusActionContract::new(plugin.clone()).unwrap();
  let action = FactoryWorkStatusAction::installed(plugin.clone(), reporter()).unwrap();
  let bound = bound_status_parameters(&plugin);
  for path in ["source_update", "source_update.connection"] {
    let mut parameters = bound.clone();
    if path == "source_update" {
      parameters["source_update"] = serde_json::Value::Null;
    } else {
      parameters["source_update"]["connection"] = serde_json::Value::Null;
    }
    let binding = FlowActionBinding::new(plugin.clone(), contract.action.clone(), parameters).unwrap();
    assert!(
      action
        .validate(&binding, std::slice::from_ref(&contract.outcome))
        .is_err(),
      "native producer requires {path}"
    );
  }
}
