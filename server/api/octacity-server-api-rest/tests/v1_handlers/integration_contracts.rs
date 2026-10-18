//! Shared wire fixtures guard the published contract consumed by generated console clients.
use super::*;

#[test]
fn work_status_action_has_a_typed_provider_neutral_parameter_contract() {
  let document = openapi_document();
  assert_component_exists(&document, "FactoryWorkStatusActionParameters");
  let fixtures: serde_json::Value =
    serde_json::from_str(include_str!("../../fixtures/v1/integration-contracts.json")).unwrap();
  let parameters = &fixtures["FactoryWorkStatusActionParameters"][0];
  assert_json_matches_component(&document, "FactoryWorkStatusActionParameters", parameters);
  let schema = &document["components"]["schemas"]["FactoryWorkStatusActionParameters"];
  assert_eq!(schema["additionalProperties"], false);
  assert!(schema["properties"]["status"].get("enum").is_none());
  assert_eq!(schema["properties"]["status"]["x-max-utf8-bytes"], 2048);
  assert_eq!(
    schema["properties"]["outbox"]["anyOf"][0]["properties"]["max_attempts"]["maximum"],
    1024
  );
  assert_eq!(
    schema["properties"]["outbox"]["anyOf"][0]["properties"]["retry_delay_millis"]["maximum"],
    3_600_000
  );
}

#[test]
fn integration_openapi_publishes_schema_checked_shared_wire_examples() {
  let document = openapi_document();
  let fixtures: serde_json::Value =
    serde_json::from_str(include_str!("../../fixtures/v1/integration-contracts.json")).unwrap();
  for (component, examples) in fixtures.as_object().unwrap() {
    assert_component_exists(&document, component);
    for example in examples.as_array().unwrap() {
      assert_json_matches_component(&document, component, example);
    }
    assert_eq!(
      document["components"]["schemas"][component]["examples"], *examples,
      "missing public examples for {component}"
    );
  }
}

#[test]
fn credential_schema_documents_the_transient_utf8_byte_ceiling() {
  let document = openapi_document();
  let secret = &document["components"]["schemas"]["IntegrationCredentialSubmission"]["properties"]["value"];
  assert_eq!(secret["writeOnly"], true);
  assert_eq!(secret["x-max-utf8-bytes"], 16384);
}

fn assert_safe_response_schema(
  document: &serde_json::Value,
  schema: &serde_json::Value,
  visited: &mut BTreeSet<String>,
) {
  if let Some(reference) = schema["$ref"].as_str() {
    if visited.insert(reference.to_owned()) {
      assert_safe_response_schema(document, document.pointer(&reference[1..]).unwrap(), visited);
    }
    return;
  }
  assert_ne!(
    schema["writeOnly"], true,
    "a secret submission schema is reachable from an ordinary response"
  );
  if let Some(properties) = schema["properties"].as_object() {
    for forbidden in [
      "token",
      "value",
      "secret",
      "password",
      "credential_path",
      "provider_payload",
      "body",
    ] {
      assert!(
        !properties.contains_key(forbidden),
        "private response field {forbidden}"
      );
    }
  }
  match schema {
    serde_json::Value::Object(values) => {
      for (key, value) in values {
        if !["examples", "description"].contains(&key.as_str()) {
          assert_safe_response_schema(document, value, visited);
        }
      }
    }
    serde_json::Value::Array(values) => {
      for value in values {
        assert_safe_response_schema(document, value, visited);
      }
    }
    _ => {}
  }
}

#[test]
fn ordinary_integration_responses_cannot_reach_secret_submission_and_manual_intake_needs_no_connection() {
  let document = openapi_document();
  for (path, item) in document["paths"].as_object().unwrap() {
    if !path.contains("/integrations/") && !path.contains("/factory-source") {
      continue;
    }
    for operation in item.as_object().unwrap().values() {
      let response = &operation["responses"]["200"]["content"]["application/json"]["schema"];
      assert!(response.is_object(), "missing integration response for {path}");
      assert_safe_response_schema(&document, response, &mut BTreeSet::new());
      let request = &operation["requestBody"]["content"]["application/json"]["schema"]["$ref"];
      if request == "#/components/schemas/IntegrationCredentialSubmission" {
        assert_eq!(operation["operationId"], "manageIntegrationCredential");
      }
    }
  }
  let manual = &document["components"]["schemas"]["AdmitManualFactoryWorkRequest"]["properties"];
  for forbidden in ["connection", "input_id", "issue_id", "credential", "token"] {
    assert!(manual.get(forbidden).is_none());
  }
  let issue = &document["components"]["schemas"]["WorkSourceIssueLink"];
  assert_eq!(issue["properties"]["issue_id"]["maxLength"], 256);
  let admission = &document["components"]["schemas"]["WorkSourceAdmissionDiagnostic"];
  assert!(
    !admission["required"]
      .as_array()
      .unwrap()
      .contains(&serde_json::json!("issue"))
  );
}

#[test]
fn shared_publication_intents_round_trip_through_typed_application_contracts() {
  use octacity_server_application::{
    IntegrationAdministrationIntent, IntegrationConnectionIntent, IntegrationProjectAccessIntent,
    WorkSourceConfigurationIntent,
  };
  fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned>(value: &serde_json::Value) {
    let typed: T = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(typed).unwrap(), *value);
  }
  let fixtures: serde_json::Value =
    serde_json::from_str(include_str!("../../fixtures/v1/integration-contracts.json")).unwrap();
  round_trip::<IntegrationConnectionIntent>(&fixtures["IntegrationConnectionPublication"][0]["intent"]);
  round_trip::<IntegrationProjectAccessIntent>(&fixtures["IntegrationProjectAccessPublication"][0]["intent"]);
  for example in fixtures["IntegrationCredentialSubmission"].as_array().unwrap() {
    round_trip::<IntegrationAdministrationIntent>(&example["intent"]);
  }
  for example in fixtures["WorkSourceConfigurationSubmission"].as_array().unwrap() {
    round_trip::<WorkSourceConfigurationIntent>(&example["intent"]);
  }
}

#[test]
fn integration_fixture_validation_rejects_non_string_credential_values() {
  let document = openapi_document();
  let mut example = document["components"]["schemas"]["IntegrationCredentialSubmission"]["examples"][0].clone();
  example["value"] = serde_json::json!(42);
  assert!(
    std::panic::catch_unwind(|| {
      assert_json_matches_component(&document, "IntegrationCredentialSubmission", &example);
    })
    .is_err(),
    "a numeric credential must not satisfy string|null"
  );
  for valid in [serde_json::json!("transient-placeholder"), serde_json::Value::Null] {
    example["value"] = valid;
    assert_json_matches_component(&document, "IntegrationCredentialSubmission", &example);
  }
}

#[test]
fn integration_fixture_validation_rejects_oversized_utf8_status() {
  let document = openapi_document();
  let mut example = document["components"]["schemas"]["FactoryWorkStatusActionParameters"]["examples"][0].clone();
  example["status"] = serde_json::json!("я".repeat(1025));
  assert!(
    std::panic::catch_unwind(|| {
      assert_json_matches_component(&document, "FactoryWorkStatusActionParameters", &example);
    })
    .is_err(),
    "2050 UTF-8 bytes must not satisfy the 2048-byte ceiling"
  );
  example["status"] = serde_json::json!("я".repeat(1024));
  assert_json_matches_component(&document, "FactoryWorkStatusActionParameters", &example);
}

#[test]
fn integration_fixture_validation_rejects_oversized_preview_pages() {
  let document = openapi_document();
  let mut example = document["components"]["schemas"]["WorkSourcePreview"]["examples"][0].clone();
  example["candidates"] = serde_json::json!(vec![example["candidates"][0].clone(); 101]);
  assert!(
    std::panic::catch_unwind(|| {
      assert_json_matches_component(&document, "WorkSourcePreview", &example);
    })
    .is_err(),
    "101 candidates must not satisfy maxItems=100"
  );
  example["candidates"].as_array_mut().unwrap().pop();
  assert_json_matches_component(&document, "WorkSourcePreview", &example);
}
