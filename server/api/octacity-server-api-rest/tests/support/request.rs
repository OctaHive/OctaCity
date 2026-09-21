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
