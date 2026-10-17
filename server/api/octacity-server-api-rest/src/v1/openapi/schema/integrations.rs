//! Credential-free connection envelopes and one protected write-only submission.
use octacity_server_application::MAX_INTEGRATION_NAME_BYTES;
use serde_json::{Map, Value, json};
fn reference(name: &str) -> Value {
  json!({"$ref":format!("#/components/schemas/{name}")})
}
fn object(properties: Value, required: &[&str]) -> Value {
  json!({"type":"object","additionalProperties":false,"properties":properties,"required":required})
}
fn array(item: Value, maximum: usize) -> Value {
  json!({"type":"array","items":item,"maxItems":maximum})
}
fn text(maximum: usize) -> Value {
  json!({"type":"string","minLength":1,"maxLength":maximum})
}
fn intent(action: Value) -> Value {
  object(
    json!({"id":{"type":"string","format":"uuid"},"expected_metadata_version":{"type":"integer","minimum":1},
    "expected_revision":{"type":"integer","minimum":0},"action":action}),
    &["id", "expected_metadata_version", "expected_revision", "action"],
  )
}
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let mut name = text(MAX_INTEGRATION_NAME_BYTES);
  name["description"] = json!(format!(
    "At most {MAX_INTEGRATION_NAME_BYTES} UTF-8 bytes; no surrounding whitespace or control characters."
  ));
  let pin = || reference("FactoryImmutableReferenceResource");
  let positive = || json!({"type":"integer","minimum":1});
  let identity = || json!({"type":"string","format":"uuid"});
  let credential = object(
    json!({"provider":text(128),"reference":text(128)}),
    &["provider", "reference"],
  );
  let purpose = json!({"type":"string","enum":["task_read","task_update","repository_read","delivery"]});
  let capability = object(
    json!({"purpose":purpose,"capability":pin(),"configuration":pin(),"credential":credential}),
    &["purpose", "capability", "configuration", "credential"],
  );
  let resource = object(
    json!({"reference":pin(),"native_id":text(1024),"locator":text(1024),"capabilities":array(capability,4)}),
    &["reference", "native_id", "locator", "capabilities"],
  );
  let definition = object(
    json!({"name":name,"provider":text(128),"installation":pin(),"adapter":pin(),"endpoint":text(2048),"authentication":text(128),"resources":array(resource,64)}),
    &[
      "name",
      "provider",
      "installation",
      "adapter",
      "endpoint",
      "authentication",
      "resources",
    ],
  );
  let draft = object(
    json!({"name":name,"installation":pin(),"endpoint":text(2048),"authentication":text(128),
    "resources":array(object(json!({"reference":pin(),"purposes":array(purpose.clone(),4)}), &["reference","purposes"]),64)}),
    &["name", "installation", "endpoint", "authentication", "resources"],
  );
  let metadata_intent = object(
    json!({"id":identity(),"expected_current_version":{"type":["integer","null"],"minimum":1},"draft":draft}),
    &["id", "expected_current_version", "draft"],
  );
  schemas.insert(
    "IntegrationConnectionPublication".into(),
    object(json!({"intent":metadata_intent}), &["intent"]),
  );
  schemas.insert("IntegrationConnection".into(), object(json!({"id":identity(),"version":positive(),"digest":reference("FactoryDigest"),"definition":definition,"published_at_unix_ms":{"type":"integer"}}), &["id","version","digest","definition","published_at_unix_ms"]));
  schemas.insert("IntegrationConnectionMutation".into(), object(json!({"disposition":{"type":"string","enum":["applied","replayed"]},"connection":reference("IntegrationConnection")}), &["disposition","connection"]));
  let grant_resources = array(
    object(
      json!({"reference":pin(),"purposes":array(purpose.clone(),4)}),
      &["reference", "purposes"],
    ),
    octacity_server_application::MAX_INTEGRATION_RESOURCES,
  );
  let grant_intent = object(
    json!({"connection_id":identity(),"project_id":identity(),"connection_version":positive(),
    "expected_current_version":{"type":["integer","null"],"minimum":1},"resources":grant_resources}),
    &[
      "connection_id",
      "project_id",
      "connection_version",
      "expected_current_version",
      "resources",
    ],
  );
  schemas.insert(
    "IntegrationProjectAccessPublication".into(),
    object(json!({"intent":grant_intent}), &["intent"]),
  );
  schemas.insert("IntegrationProjectAccess".into(),object(json!({"connection_id":identity(),"project_id":identity(),"connection_version":positive(),"version":positive(),"resources":grant_resources,"published_at_unix_ms":{"type":"integer"}}),
    &["connection_id","project_id","connection_version","version","resources","published_at_unix_ms"]));
  schemas.insert("IntegrationProjectAccessMutation".into(),object(json!({"disposition":{"type":"string","enum":["applied","replayed"]},"access":reference("IntegrationProjectAccess")}), &["disposition","access"]));
  schemas.insert(
    "IntegrationProjectAccessState".into(),
    object(
      json!({"access":{"anyOf":[reference("IntegrationProjectAccess"),{"type":"null"}]}}),
      &["access"],
    ),
  );
  let usable_capability = object(
    json!({"purpose":purpose,"capability":pin(),"configuration":pin()}),
    &["purpose", "capability", "configuration"],
  );
  let usable_resource = object(
    json!({"reference":pin(),"native_id":text(1024),"locator":text(1024),"capabilities":array(usable_capability,4)}),
    &["reference", "native_id", "locator", "capabilities"],
  );
  schemas.insert(
    "IntegrationProjectResources".into(),
    object(
      json!({"resources":array(usable_resource,octacity_server_application::MAX_INTEGRATION_RESOURCES)}),
      &["resources"],
    ),
  );
  let credential_action = object(
    json!({"operation":{"const":"credential"},"reference":credential,"kind":{"type":"string","enum":["provision","adopt","rotate","revoke"]},"activate_connection":{"type":"boolean","default":false}}),
    &["operation", "reference", "kind"],
  );
  schemas.insert("IntegrationCredentialSubmission".into(), object(json!({"intent":intent(credential_action),"value":{"type":["string","null"],"minLength":1,"maxLength":16384,"writeOnly":true,
    "description":"Transient protected input. Omit for adoption, revocation or recovery. Never returned or retained in ordinary storage."}}), &["intent"]));
  let state_action = object(
    json!({"operation":{"type":"string","enum":["activate","disable"]}}),
    &["operation"],
  );
  schemas.insert(
    "IntegrationStateChange".into(),
    object(json!({"intent":intent(state_action)}), &["intent"]),
  );
  schemas.insert("IntegrationAdministrationReceipt".into(), object(json!({"operation_id":{"type":"string","pattern":"^[0-9a-f]{64}$"},"integration_id":identity(),"revision":positive(),"metadata_version":positive(),
    "status":{"type":"string","enum":["prepared","completed","abandoned"]},"requested_at_unix_ms":{"type":"integer"}}), &["operation_id","integration_id","revision","metadata_version","status","requested_at_unix_ms"]));
  let configured = object(
    json!({"reference":credential,"configured":{"type":"boolean"}}),
    &["reference", "configured"],
  );
  schemas.insert("IntegrationAdministrationState".into(), object(json!({"id":identity(),"revision":{"type":"integer","minimum":0},"enabled":{"type":"boolean"},"active_version":{"type":["integer","null"],"minimum":1},
    "credentials":{"type":"array","items":configured,"description":"Configured status for retained credential references across immutable metadata versions; the per-version resource ceiling does not limit this history."},"preparation_pending":{"type":"boolean"}}), &["id","revision","enabled","active_version","credentials","preparation_pending"]));
  schemas.insert(
    "IntegrationPreparationCleanup".into(),
    object(
      json!({"limit":{"type":"integer","minimum":1,"maximum":100}}),
      &["limit"],
    ),
  );
  schemas.insert(
    "IntegrationPreparationCleanupReceipt".into(),
    object(
      json!({"cleaned":{"type":"integer","minimum":0,"maximum":100}}),
      &["cleaned"],
    ),
  );
}
