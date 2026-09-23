use axum::{
  body::Body,
  http::{Request, request::Builder},
};

pub fn concrete_path(path: &str) -> String {
  path
    .replace("{project_id}", "11111111-1111-4111-8111-111111111111")
    .replace("{pipeline_id}", "22222222-2222-4222-8222-222222222222")
    .replace("{repository_id}", "33333333-3333-4333-8333-333333333333")
    .replace("{configuration_id}", "44444444-4444-4444-8444-444444444444")
    .replace("{trigger_id}", "88888888-8888-4888-8888-888888888888")
    .replace("{build_id}", "99999999-9999-4999-8999-999999999999")
    .replace("{attempt_id}", "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
    .replace("{job_id}", "55555555-5555-4555-8555-555555555555")
    .replace("{artifact_id}", "cccccccc-cccc-4ccc-8ccc-cccccccccccc")
    .replace("{pool_id}", "66666666-6666-4666-8666-666666666666")
    .replace("{integration_id}", "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")
    .replace("{version}", "1")
}

pub fn json_request(
  method: &str,
  uri: &str,
  idempotency_key: &str,
  version: Option<&str>,
  body: &str,
) -> Request<Body> {
  let request = Request::builder()
    .method(method)
    .uri(uri)
    .header("content-type", "application/json")
    .header("idempotency-key", idempotency_key);
  request_with_version(request, version)
    .body(Body::from(body.to_owned()))
    .unwrap()
}

pub fn empty_request(method: &str, uri: &str, mutation: Option<(&str, &str)>) -> Request<Body> {
  let request = Request::builder().method(method).uri(uri);
  let request = match mutation {
    Some((key, version)) => request.header("idempotency-key", key).header("if-match", version),
    None => request,
  };
  request.body(Body::empty()).unwrap()
}

fn request_with_version(request: Builder, version: Option<&str>) -> Builder {
  match version {
    Some(version) => request.header("if-match", version),
    None => request,
  }
}

pub fn repository_body(project_id: &str) -> String {
  let version: serde_json::Value = serde_json::from_str(&repository_version_body()).unwrap();
  serde_json::to_string(&serde_json::json!({
    "project_id": project_id,
    "name": "Source",
    "definition": version["definition"],
  }))
  .unwrap()
}

pub fn repository_version_body() -> String {
  r#"{"definition":{"vcs_integration_id":"55555555-5555-4555-8555-555555555555","repository_locator":"https://example.test/source.git","selection":{"allowed_references":["refs/heads/main"],"default_reference":"refs/heads/main","allow_exact_revision":true}}}"#.to_owned()
}

pub fn configuration_version_body() -> String {
  let create: serde_json::Value = serde_json::from_str(include_str!(
    "../../fixtures/v1/create-build-configuration-request.json"
  ))
  .unwrap();
  serde_json::to_string(&serde_json::json!({"definition": create["definition"]})).unwrap()
}

pub fn request_examples() -> Vec<(&'static str, serde_json::Value)> {
  let mut create_pipeline: serde_json::Value =
    serde_json::from_str(include_str!("../../fixtures/v1/create-pipeline-request.json")).unwrap();
  create_pipeline["dag"]["nodes"][0]["execution"]["octafile"] = serde_json::Value::Null;
  create_pipeline["dag"]["nodes"][0]["execution"]["concurrency"] = serde_json::Value::Null;
  let create_configuration: serde_json::Value = serde_json::from_str(include_str!(
    "../../fixtures/v1/create-build-configuration-request.json"
  ))
  .unwrap();
  let repository_version: serde_json::Value = serde_json::from_str(&repository_version_body()).unwrap();
  vec![
    (
      "CreateProjectRequest",
      serde_json::from_str(include_str!("../../fixtures/v1/create-project-request.json")).unwrap(),
    ),
    ("RenameProjectRequest", serde_json::json!({"name": "Renamed"})),
    ("MoveProjectRequest", serde_json::json!({"parent_id": null})),
    (
      "PublishProjectPolicyRequest",
      serde_json::json!({"policy": {
        "pools": {"mode": "replace", "value": []},
        "repositories": {"mode": "replace", "value": []},
        "secret_profiles": {"mode": "replace", "value": []},
        "identity_profiles": {"mode": "replace", "value": []},
        "runtimes": {"mode": "replace", "value": []},
        "cache": {"mode": "replace", "value": {"namespaces": [], "read": false, "write": false, "max_bytes": 0}},
        "artifacts": {"mode": "replace", "value": {"artifact_count": 0, "artifact_bytes": 0, "report_count": 0, "report_bytes": 0, "single_output_bytes": 0}},
        "concurrency": {"mode": "replace", "value": {"active_builds": 1, "active_jobs": 1}},
        "retention": {"mode": "replace", "value": {"build_seconds": 1, "log_seconds": 1, "artifact_seconds": 1, "cache_seconds": 1}}
      }}),
    ),
    ("CreatePipelineRequest", create_pipeline.clone()),
    (
      "PublishPipelineVersionRequest",
      serde_json::json!({"dag": create_pipeline["dag"]}),
    ),
    (
      "CreateRepositoryRequest",
      serde_json::json!({"project_id": "project", "name": "Source", "definition": repository_version["definition"]}),
    ),
    ("PublishRepositoryVersionRequest", repository_version),
    ("CreateBuildConfigurationRequest", create_configuration.clone()),
    (
      "PublishBuildConfigurationVersionRequest",
      serde_json::json!({"definition": create_configuration["definition"]}),
    ),
    (
      "CreateManualTriggerDefinitionRequest",
      serde_json::json!({
        "configuration_id": "configuration",
        "configuration_version": 1,
        "enabled": true,
        "definition": {}
      }),
    ),
    (
      "CreateScheduledTriggerDefinitionRequest",
      serde_json::json!({
        "configuration_id": "configuration",
        "configuration_version": 1,
        "enabled": true,
        "schedule": {
          "expression": "0 0 9 * * Mon-Fri *",
          "timezone": "Europe/Moscow",
          "missed_run_policy": {"kind": "catch_up", "maximum_occurrences": 4}
        },
        "build": {
          "source": {"kind": "exact_revision", "value": "0123456789abcdef"},
          "parameters": {},
          "priority": 0
        }
      }),
    ),
    (
      "CreateUnmanagedWebhookRequest",
      serde_json::json!({
        "configuration_id": "configuration",
        "configuration_version": 1,
        "enabled": true,
        "adapter_id": "github",
        "adapter_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "verification_material_handle": "secret:webhook",
        "verification_headers": ["x-hub-signature-256"],
        "repository_id": "repository",
        "event_kind": "push",
        "parameters": {},
        "priority": 0
      }),
    ),
    (
      "AcceptManualTriggerRequest",
      serde_json::from_str(include_str!("../../fixtures/v1/accept-manual-trigger-request.json")).unwrap(),
    ),
    (
      "CreateAgentPoolRequest",
      serde_json::from_str(super::agent_pool::create_body()).unwrap(),
    ),
    (
      "PublishAgentPoolVersionRequest",
      serde_json::from_str(super::agent_pool::publish_body()).unwrap(),
    ),
    (
      "IssueAgentEnrollmentRequest",
      serde_json::from_str(include_str!("../../fixtures/v1/issue-agent-enrollment-request.json")).unwrap(),
    ),
    ("DrainAgentRequest", serde_json::json!({"mode": "graceful"})),
  ]
}
