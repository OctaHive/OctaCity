use super::*;

#[tokio::test]
async fn openapi_document_cannot_drift_from_registered_routes_and_v1_dtos() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );

  let response = routes
    .clone()
    .oneshot(empty_request("GET", "/api/v1/openapi.json", None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let document: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(document["openapi"], "3.1.0");
  assert_eq!(document["security"], serde_json::json!([]));
  assert_eq!(
    document["x-octacity-management-security"],
    serde_json::json!({
      "mode": "trusted_network_unauthenticated",
      "operator_authentication": false,
      "network_isolation_required": true,
      "agent_and_webhook_authentication_unchanged": true
    })
  );

  let documented = document["paths"]
    .as_object()
    .unwrap()
    .iter()
    .flat_map(|(path, item)| {
      item
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, operation)| operation["operationId"] != "getOpenApiDocument")
        .map(move |(method, operation)| {
          (
            method.to_ascii_uppercase(),
            path.clone(),
            operation["operationId"].as_str().unwrap().to_owned(),
          )
        })
    })
    .collect::<BTreeSet<_>>();
  let registered = MANAGEMENT_OPERATIONS
    .iter()
    .map(|operation| {
      (
        operation.method.to_owned(),
        operation.path.to_owned(),
        operation.operation_id.to_owned(),
      )
    })
    .collect::<BTreeSet<_>>();
  assert_eq!(documented, registered);

  for operation in MANAGEMENT_OPERATIONS {
    let method = operation.method.to_ascii_lowercase();
    let documented = &document["paths"][operation.path][&method];
    assert_eq!(documented["operationId"], operation.operation_id);
    assert_eq!(documented["security"], serde_json::json!([]));
    assert_eq!(
      documented["responses"][operation.success_status]["content"]["application/json"]["schema"]["$ref"],
      format!("#/components/schemas/{}", operation.response_schema)
    );
    assert_component_exists(&document, operation.response_schema);
    if let Some(request_schema) = operation.request_schema {
      assert_eq!(
        documented["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        format!("#/components/schemas/{request_schema}")
      );
      assert_component_exists(&document, request_schema);
      assert_eq!(
        documented["responses"]["413"]["$ref"],
        "#/components/responses/ManagementError"
      );
      assert_eq!(
        documented["responses"]["415"]["$ref"],
        "#/components/responses/ManagementError"
      );
    }
    for status in ["400", "404", "409", "500", "503"] {
      assert_eq!(
        documented["responses"][status]["$ref"],
        "#/components/responses/ManagementError"
      );
    }
    if operation.operation_id.contains("ManagedWebhook") {
      assert_eq!(
        documented["responses"]["422"]["$ref"],
        "#/components/responses/ManagementError"
      );
    }
    assert_required_header(documented, "Idempotency-Key", operation.idempotent_mutation);
    assert_required_header(documented, "If-Match", operation.optimistic_precondition);

    let response = routes
      .clone()
      .oneshot(
        Request::builder()
          .method(Method::from_bytes(operation.method.as_bytes()).unwrap())
          .uri(concrete_path(operation.path))
          .body(Body::empty())
          .unwrap(),
      )
      .await
      .unwrap();
    assert_ne!(
      response.status(),
      StatusCode::NOT_FOUND,
      "{} {}",
      operation.method,
      operation.path
    );
    assert_ne!(
      response.status(),
      StatusCode::METHOD_NOT_ALLOWED,
      "{} {}",
      operation.method,
      operation.path
    );
  }

  let managed_response = &document["components"]["schemas"]["ManagedWebhookResource"]["properties"];
  assert!(managed_response.get("administration_credential_handle").is_none());
  assert!(managed_response.get("verification_material_handle").is_none());

  let artifact = document["components"]["schemas"]["ArtifactResource"]["properties"]
    .as_object()
    .unwrap();
  let download = document["components"]["schemas"]["ArtifactDownload"]["properties"]
    .as_object()
    .unwrap();
  for provider_field in [
    "bucket",
    "key",
    "object_key",
    "credential",
    "access_key",
    "etag",
    "generation",
    "url",
  ] {
    assert!(artifact.get(provider_field).is_none());
    assert!(download.get(provider_field).is_none());
  }
  let cache_session = document["components"]["schemas"]["CacheSessionResource"]["properties"]
    .as_object()
    .unwrap();
  for secret_field in [
    "credential",
    "credential_hash",
    "bearer",
    "bearer_token",
    "scope_id",
    "fencing_token",
  ] {
    assert!(cache_session.get(secret_field).is_none());
  }
  let error_codes = [
    ErrorCode::InvalidRequest,
    ErrorCode::UnsupportedMediaType,
    ErrorCode::UnsupportedApiVersion,
    ErrorCode::PayloadTooLarge,
    ErrorCode::InvalidIdempotencyKey,
    ErrorCode::IdempotencyConflict,
    ErrorCode::PreconditionRequired,
    ErrorCode::PreconditionFailed,
    ErrorCode::NotFound,
    ErrorCode::Conflict,
    ErrorCode::CapabilityUnavailable,
    ErrorCode::Unavailable,
    ErrorCode::RateLimited,
    ErrorCode::Internal,
  ]
  .map(|code| serde_json::to_value(code).unwrap());
  assert_eq!(
    document["components"]["schemas"]["ErrorCode"]["enum"],
    serde_json::Value::Array(error_codes.into())
  );
  assert_eq!(
    document["components"]["responses"]["ManagementError"]["content"]["application/json"]["schema"]["$ref"],
    "#/components/schemas/ErrorResponse"
  );
  let list_limit = document["paths"]["/api/v1/projects"]["get"]["parameters"]
    .as_array()
    .unwrap()
    .iter()
    .find(|parameter| parameter["name"] == "limit")
    .unwrap();
  assert_eq!(list_limit["schema"]["maximum"], 200);
  let cache_limit = document["paths"]["/api/v1/builds/{build_id}/cache-sessions"]["get"]["parameters"]
    .as_array()
    .unwrap()
    .iter()
    .find(|parameter| parameter["name"] == "limit")
    .unwrap();
  assert_eq!(cache_limit["schema"]["maximum"], 100);

  for (schema, body) in request_examples() {
    assert_json_matches_component(&document, schema, &body);
  }
  assert_json_matches_component(
    &document,
    "ScheduleResource",
    &serde_json::json!({
      "trigger_id": "88888888-8888-4888-8888-888888888888",
      "trigger_version": 1,
      "configuration_id": "44444444-4444-4444-8444-444444444444",
      "configuration_version": 1,
      "enabled": true,
      "schedule": {
        "expression": "0 0 9 * * Mon-Fri *",
        "timezone": "Europe/Moscow",
        "missed_run_policy": {"kind": "run_once"}
      },
      "next_occurrence_at_unix_ms": 1_700_000_000_000_i64,
      "build": {
        "source": {"kind": "exact_revision", "value": "0123456789abcdef"},
        "parameters": {},
        "priority": 0
      }
    }),
  );
}
