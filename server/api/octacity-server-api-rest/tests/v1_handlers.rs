use std::{
  collections::BTreeSet,
  sync::{Arc, Mutex},
  time::Duration,
};

use async_trait::async_trait;
use axum::{
  body::{Body, to_bytes},
  http::{Method, Request, StatusCode},
};
use octacity_server_api_rest::{
  management_router_with_application,
  v1::{
    AgentManagementApplication, BuildManagementApplication, CatalogManagementApplication,
    ConfigurationManagementApplication, DefinitionManagementApplication, ErrorCode, ExecutionManagementApplication,
    JobEventManagementApplication, MANAGEMENT_OPERATIONS, ManagementApplication, ManagementApplicationHandlers,
    ManualTriggerManagementApplication, PipelineManagementApplication, ProjectManagementApplication,
  },
};
use octacity_server_application::{
  AcceptManualTriggerCommand, ApplicationError, CancelBuildCommand, Command, CommandHandler, CreateAgentPoolCommand,
  CreateBuildConfigurationCommand, CreatePipelineCommand, CreateProjectCommand, CreateRepositoryCommand,
  CreateTriggerDefinitionCommand, DeleteAgentPoolCommand, DeleteProjectCommand, DrainAgentCommand, GetAgentPoolQuery,
  GetAgentQuery, GetAttemptQuery, GetBuildConfigurationQuery, GetBuildQuery, GetJobQuery, GetPipelineQuery,
  GetProjectQuery, GetRepositoryQuery, IssueAgentEnrollmentCommand, JobEventPageProjection, JobEventProjection,
  ListAgentPoolsQuery, ListAgentsQuery, ListProjectsQuery, ManualTriggerError, MoveProjectCommand,
  PublishAgentPoolVersionCommand, PublishBuildConfigurationVersionCommand, PublishPipelineVersionCommand,
  PublishProjectPolicyCommand, PublishRepositoryVersionCommand, Query, QueryHandler, ReadJobEventsQuery,
  ReassignAgentPoolCommand, RenameProjectCommand, RetryBuildCommand,
};
use tokio::net::TcpListener;
use tower::ServiceExt as _;

mod support;

use support::{
  agent_pool_create_body, agent_pool_publish_body, assert_json_matches_component, configuration_version_body,
  documented_http_requests, repository_body, repository_version_body, request_examples, send_documented_request,
};

const DOCUMENTED_SECTION_FOUR_WORKFLOW: &str = include_str!("../../../../docs/management-rest-v1.md");

#[derive(Default)]
struct RecordingApplication {
  calls: Mutex<Vec<&'static str>>,
  successful_workflow: bool,
}

impl RecordingApplication {
  fn successful_workflow() -> Self {
    Self {
      calls: Mutex::default(),
      successful_workflow: true,
    }
  }

  fn record(&self, operation: &'static str) {
    self.calls.lock().unwrap().push(operation);
  }
}

macro_rules! unavailable_command {
  ($command:ty, $operation:literal) => {
    #[async_trait]
    impl CommandHandler<$command> for RecordingApplication {
      type Error = ApplicationError;

      async fn handle_command(&self, _command: $command) -> Result<<$command as Command>::Outcome, Self::Error> {
        self.record($operation);
        Err(ApplicationError::unavailable())
      }
    }
  };
}

macro_rules! unavailable_query {
  ($query:ty, $operation:literal) => {
    #[async_trait]
    impl QueryHandler<$query> for RecordingApplication {
      type Error = ApplicationError;

      async fn handle_query(&self, _query: $query) -> Result<<$query as Query>::Outcome, Self::Error> {
        self.record($operation);
        Err(ApplicationError::unavailable())
      }
    }
  };
}

unavailable_command!(RenameProjectCommand, "rename_project");
unavailable_command!(MoveProjectCommand, "move_project");
unavailable_command!(DeleteProjectCommand, "delete_project");
unavailable_query!(GetProjectQuery, "get_project");
unavailable_query!(ListProjectsQuery, "list_projects");
unavailable_command!(PublishPipelineVersionCommand, "publish_pipeline");
unavailable_query!(GetPipelineQuery, "get_pipeline");
unavailable_command!(PublishRepositoryVersionCommand, "publish_repository");
unavailable_query!(GetRepositoryQuery, "get_repository");
unavailable_command!(PublishBuildConfigurationVersionCommand, "publish_configuration");
unavailable_command!(PublishProjectPolicyCommand, "publish_project_policy");
unavailable_command!(CreateTriggerDefinitionCommand, "create_trigger_definition");
unavailable_query!(GetBuildConfigurationQuery, "get_configuration");
unavailable_query!(ReadJobEventsQuery, "read_job_events");
unavailable_command!(CreateAgentPoolCommand, "create_agent_pool");
unavailable_command!(PublishAgentPoolVersionCommand, "publish_agent_pool");
unavailable_command!(DeleteAgentPoolCommand, "delete_agent_pool");
unavailable_query!(GetAgentPoolQuery, "get_agent_pool");
unavailable_query!(ListAgentPoolsQuery, "list_agent_pools");
unavailable_query!(GetAgentQuery, "get_agent");
unavailable_query!(ListAgentsQuery, "list_agents");
unavailable_command!(ReassignAgentPoolCommand, "reassign_agent_pool");
unavailable_command!(DrainAgentCommand, "drain_agent");
unavailable_command!(IssueAgentEnrollmentCommand, "issue_agent_enrollment");
unavailable_query!(GetBuildQuery, "get_build");
unavailable_query!(GetAttemptQuery, "get_attempt");
unavailable_query!(GetJobQuery, "get_job");
unavailable_command!(CancelBuildCommand, "cancel_build");
unavailable_command!(RetryBuildCommand, "retry_build");

#[async_trait]
impl CommandHandler<CreateProjectCommand> for RecordingApplication {
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: CreateProjectCommand,
  ) -> Result<<CreateProjectCommand as Command>::Outcome, Self::Error> {
    self.record("create_project");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(
      serde_json::from_value(serde_json::json!({
        "disposition": "applied",
        "project": {
          "id": command.id,
          "parent_id": command.parent_id,
          "name": command.name,
          "version": 1,
          "created_at": command.created_at,
          "updated_at": command.created_at
        }
      }))
      .unwrap(),
    )
  }
}

#[async_trait]
impl CommandHandler<CreatePipelineCommand> for RecordingApplication {
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: CreatePipelineCommand,
  ) -> Result<<CreatePipelineCommand as Command>::Outcome, Self::Error> {
    self.record("create_pipeline");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(
      serde_json::from_value(serde_json::json!({
        "disposition": "applied",
        "pipeline": {
          "id": command.id,
          "project_id": command.project_id,
          "name": command.name,
          "version": 1,
          "nodes": [{
            "id": "build",
            "name": "Build",
            "dependency_policy": "all_succeeded",
            "required_capabilities": ["native"],
            "execution": {
              "octafile": null,
              "commands": ["build"],
              "arguments": [],
              "concurrency": null,
              "parallel": false,
              "failfast": true
            }
          }],
          "edges": [],
          "published_at": command.published_at
        }
      }))
      .unwrap(),
    )
  }
}

#[async_trait]
impl CommandHandler<CreateRepositoryCommand> for RecordingApplication {
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: CreateRepositoryCommand,
  ) -> Result<<CreateRepositoryCommand as Command>::Outcome, Self::Error> {
    self.record("create_repository");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    let definition = serde_json::to_value(command.definition).unwrap();
    Ok(
      serde_json::from_value(serde_json::json!({
        "disposition": "applied",
        "repository": {
          "id": command.id,
          "project_id": command.project_id,
          "name": command.name,
          "version": 1,
          "vcs_integration_id": definition["vcs_integration_id"],
          "repository_locator": definition["repository_locator"],
          "selection": definition["selection"],
          "published_at": command.published_at
        }
      }))
      .unwrap(),
    )
  }
}

#[async_trait]
impl CommandHandler<CreateBuildConfigurationCommand> for RecordingApplication {
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: CreateBuildConfigurationCommand,
  ) -> Result<<CreateBuildConfigurationCommand as Command>::Outcome, Self::Error> {
    self.record("create_configuration");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    let mut definition = serde_json::to_value(command.definition).unwrap();
    definition["triggers"] = std::mem::take(&mut definition["triggers"]["allowed"]);
    Ok(
      serde_json::from_value(serde_json::json!({
        "disposition": "applied",
        "configuration": {
          "id": command.id,
          "project_id": command.project_id,
          "name": command.name,
          "version": 1,
          "enabled": definition["enabled"],
          "job_concurrency_limit": definition["job_concurrency_limit"],
          "repository_id": definition["repository_id"],
          "repository_version": definition["repository_version"],
          "pipeline_id": definition["pipeline_id"],
          "pipeline_version": definition["pipeline_version"],
          "parameters": definition["parameters"],
          "triggers": definition["triggers"],
          "agent_requirements": definition["agent_requirements"],
          "allowed_pools": definition["allowed_pools"],
          "runtime": definition["runtime"],
          "cache": definition["cache"],
          "artifacts": definition["artifacts"],
          "retry": definition["retry"],
          "published_at": command.published_at
        }
      }))
      .unwrap(),
    )
  }
}

#[async_trait]
impl CommandHandler<AcceptManualTriggerCommand> for RecordingApplication {
  type Error = ManualTriggerError;

  async fn handle_command(
    &self,
    _command: AcceptManualTriggerCommand,
  ) -> Result<<AcceptManualTriggerCommand as Command>::Outcome, Self::Error> {
    self.record("accept_manual_trigger");
    if !self.successful_workflow {
      return Err(ManualTriggerError::unavailable());
    }
    Ok(
      serde_json::from_value(serde_json::json!({
        "outcome": "accepted",
        "disposition": "applied",
        "trigger_occurrence_id": "88888888-8888-4888-8888-888888888888",
        "build_id": "99999999-9999-4999-8999-999999999999",
        "attempt_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "ready_job_ids": ["bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"]
      }))
      .unwrap(),
    )
  }
}

struct JobEventApplication;

#[async_trait]
impl QueryHandler<ReadJobEventsQuery> for JobEventApplication {
  type Error = ApplicationError;

  async fn handle_query(&self, query: ReadJobEventsQuery) -> Result<JobEventPageProjection, Self::Error> {
    assert_eq!(query.after_sequence, 4);
    assert_eq!(query.limit, 2);
    assert_eq!(query.wait.as_millis(), 25);
    Ok(JobEventPageProjection {
      events: vec![JobEventProjection {
        sequence: 5,
        kind: "progress".to_owned(),
        occurred_at_unix_ms: 1_234,
        payload: serde_json::json!({"step": "compile"}),
      }],
      cursor: 5,
    })
  }
}

fn recording_management_application<E>(
  application: Arc<RecordingApplication>,
  job_events: Arc<E>,
) -> ManagementApplication
where
  E: QueryHandler<ReadJobEventsQuery, Error = ApplicationError> + 'static,
{
  ManagementApplication::new(
    ["native".to_owned()],
    Duration::from_secs(900),
    octacity_server_application::AgentEnrollmentSecretKey::new([7; 32]),
    ManagementApplicationHandlers::new(
      CatalogManagementApplication::new(
        ProjectManagementApplication::new(Arc::clone(&application)),
        PipelineManagementApplication::new(Arc::clone(&application)),
        ConfigurationManagementApplication::new(Arc::clone(&application)),
        DefinitionManagementApplication::new(Arc::clone(&application)),
      ),
      AgentManagementApplication::new(
        Arc::clone(&application),
        Arc::clone(&application),
        Arc::clone(&application),
      ),
      ExecutionManagementApplication::new(
        BuildManagementApplication::new(Arc::clone(&application)),
        ManualTriggerManagementApplication::new(application),
        JobEventManagementApplication::new(job_events),
      ),
    ),
  )
  .unwrap()
}

#[tokio::test]
async fn every_registered_route_dispatches_only_through_application_handlers() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );
  let project_id = "11111111-1111-4111-8111-111111111111";
  let pipeline_id = "22222222-2222-4222-8222-222222222222";
  let repository_id = "33333333-3333-4333-8333-333333333333";
  let configuration_id = "44444444-4444-4444-8444-444444444444";
  let job_id = "55555555-5555-4555-8555-555555555555";
  let pool_id = "66666666-6666-4666-8666-666666666666";
  let agent_id = "77777777-7777-4777-8777-777777777777";
  let build_id = "99999999-9999-4999-8999-999999999999";
  let attempt_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

  let requests = vec![
    json_request(
      "POST",
      "/api/v1/projects",
      "create-project",
      None,
      include_str!("../fixtures/v1/create-project-request.json"),
    ),
    json_request(
      "POST",
      &format!("/api/v1/projects/{project_id}/rename"),
      "rename-project",
      Some("\"1\""),
      r#"{"name":"Renamed"}"#,
    ),
    json_request(
      "POST",
      &format!("/api/v1/projects/{project_id}/move"),
      "move-project",
      Some("\"1\""),
      r#"{"parent_id":null}"#,
    ),
    json_request(
      "POST",
      &format!("/api/v1/projects/{project_id}/policy-versions"),
      "publish-project-policy",
      None,
      r#"{"policy":{"pools":{"mode":"replace","value":[]},"repositories":{"mode":"replace","value":[]},"secret_profiles":{"mode":"replace","value":[]},"identity_profiles":{"mode":"replace","value":[]},"runtimes":{"mode":"replace","value":[]},"cache":{"mode":"replace","value":{"namespaces":[],"read":false,"write":false,"max_bytes":0}},"artifacts":{"mode":"replace","value":{"artifact_count":0,"artifact_bytes":0,"report_count":0,"report_bytes":0,"single_output_bytes":0}},"concurrency":{"mode":"replace","value":{"active_builds":1,"active_jobs":1}},"retention":{"mode":"replace","value":{"build_seconds":1,"log_seconds":1,"artifact_seconds":1,"cache_seconds":1}}}}"#,
    ),
    empty_request(
      "DELETE",
      &format!("/api/v1/projects/{project_id}"),
      Some(("delete-project", "\"1\"")),
    ),
    empty_request("GET", &format!("/api/v1/projects/{project_id}"), None),
    empty_request("GET", "/api/v1/projects?limit=10", None),
    json_request(
      "POST",
      "/api/v1/pipelines",
      "create-pipeline",
      None,
      include_str!("../fixtures/v1/create-pipeline-request.json"),
    ),
    json_request(
      "POST",
      &format!("/api/v1/pipelines/{pipeline_id}/versions"),
      "publish-pipeline",
      Some("\"1\""),
      r#"{"dag":{"nodes":[{"id":"build","name":"Build","dependency_policy":"all_succeeded","required_capabilities":["native"],"execution":{"commands":["build"]}}],"edges":[]}}"#,
    ),
    empty_request("GET", &format!("/api/v1/pipelines/{pipeline_id}/versions/1"), None),
    json_request(
      "POST",
      "/api/v1/repositories",
      "create-repository",
      None,
      &repository_body(project_id),
    ),
    json_request(
      "POST",
      &format!("/api/v1/repositories/{repository_id}/versions"),
      "publish-repository",
      Some("\"1\""),
      &repository_version_body(),
    ),
    empty_request("GET", &format!("/api/v1/repositories/{repository_id}/versions/1"), None),
    json_request(
      "POST",
      "/api/v1/build-configurations",
      "create-configuration",
      None,
      include_str!("../fixtures/v1/create-build-configuration-request.json"),
    ),
    json_request(
      "POST",
      &format!("/api/v1/build-configurations/{configuration_id}/versions"),
      "publish-configuration",
      Some("\"1\""),
      &configuration_version_body(),
    ),
    empty_request(
      "GET",
      &format!("/api/v1/build-configurations/{configuration_id}/versions/1"),
      None,
    ),
    json_request(
      "POST",
      "/api/v1/trigger-definitions/manual",
      "create-trigger-definition",
      None,
      &format!(
        r#"{{"configuration_id":"{configuration_id}","configuration_version":1,"enabled":true,"definition":{{}}}}"#
      ),
    ),
    json_request(
      "POST",
      "/api/v1/triggers/manual",
      "release-main-2026-09-18",
      None,
      include_str!("../fixtures/v1/accept-manual-trigger-request.json"),
    ),
    empty_request("GET", &format!("/api/v1/builds/{build_id}"), None),
    empty_request(
      "POST",
      &format!("/api/v1/builds/{build_id}/cancel"),
      Some(("cancel-build", "\"1\"")),
    ),
    empty_request(
      "POST",
      &format!("/api/v1/builds/{build_id}/retry"),
      Some(("retry-build", "\"1\"")),
    ),
    empty_request("GET", &format!("/api/v1/attempts/{attempt_id}"), None),
    empty_request("GET", &format!("/api/v1/jobs/{job_id}"), None),
    empty_request(
      "GET",
      &format!("/api/v1/jobs/{job_id}/events?after=0&limit=100&wait_ms=0"),
      None,
    ),
    json_request(
      "POST",
      "/api/v1/agent-enrollments",
      "issue-agent-enrollment",
      None,
      &format!(
        r#"{{"pool_id":"{pool_id}","pool_version":1,"expected_platform":{{"operating_system":"linux","architecture":"amd64"}}}}"#
      ),
    ),
    json_request(
      "POST",
      "/api/v1/agent-pools",
      "create-agent-pool",
      None,
      agent_pool_create_body(),
    ),
    json_request(
      "POST",
      &format!("/api/v1/agent-pools/{pool_id}/versions"),
      "publish-agent-pool",
      Some("\"1\""),
      agent_pool_publish_body(),
    ),
    empty_request("GET", &format!("/api/v1/agent-pools/{pool_id}/versions/1"), None),
    empty_request("GET", "/api/v1/agent-pools?limit=10", None),
    empty_request(
      "DELETE",
      &format!("/api/v1/agent-pools/{pool_id}"),
      Some(("delete-agent-pool", "\"1\"")),
    ),
    empty_request("GET", &format!("/api/v1/agents/{agent_id}"), None),
    empty_request("GET", "/api/v1/agents?limit=10", None),
    json_request(
      "POST",
      &format!("/api/v1/agents/{agent_id}/pool"),
      "reassign-agent-pool",
      Some("\"1\""),
      &format!(r#"{{"pool_id":"{pool_id}"}}"#),
    ),
    json_request(
      "POST",
      &format!("/api/v1/agents/{agent_id}/drain"),
      "drain-agent",
      Some("\"1\""),
      r#"{"mode":"graceful"}"#,
    ),
  ];

  for (index, request) in requests.into_iter().enumerate() {
    let response = routes.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE, "request {index}");
  }

  assert_eq!(
    *application.calls.lock().unwrap(),
    [
      "create_project",
      "rename_project",
      "move_project",
      "publish_project_policy",
      "delete_project",
      "get_project",
      "list_projects",
      "create_pipeline",
      "publish_pipeline",
      "get_pipeline",
      "create_repository",
      "publish_repository",
      "get_repository",
      "create_configuration",
      "publish_configuration",
      "get_configuration",
      "create_trigger_definition",
      "accept_manual_trigger",
      "get_build",
      "cancel_build",
      "retry_build",
      "get_attempt",
      "get_job",
      "read_job_events",
      "issue_agent_enrollment",
      "create_agent_pool",
      "publish_agent_pool",
      "get_agent_pool",
      "list_agent_pools",
      "delete_agent_pool",
      "get_agent",
      "list_agents",
      "reassign_agent_pool",
      "drain_agent",
    ]
  );
}

#[tokio::test]
async fn transport_rejections_do_not_reach_an_application_handler() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );

  let response = routes
    .clone()
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/api/v1/projects")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"Root","unknown":true}"#))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);

  let response = routes
    .oneshot(json_request(
      "POST",
      "/api/v1/projects/11111111-1111-4111-8111-111111111111/rename",
      "rename",
      None,
      r#"{"name":"Renamed"}"#,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
  assert!(application.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn unsupported_api_versions_return_the_stable_version_error() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );

  let response = routes
    .oneshot(empty_request("GET", "/api/v2/projects", None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["code"], "unsupported_api_version");
  assert!(application.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn job_event_parameters_and_page_use_the_stable_rest_contract() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::new(JobEventApplication)),
  );

  let response = routes
    .clone()
    .oneshot(empty_request(
      "GET",
      "/api/v1/jobs/55555555-5555-4555-8555-555555555555/events?after=4&limit=2&wait_ms=25",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(
    body,
    serde_json::json!({
      "items": [{
        "sequence": 5,
        "kind": "progress",
        "occurred_at_unix_ms": 1234,
        "payload": {"step": "compile"}
      }],
      "cursor": 5
    })
  );

  let invalid = routes
    .oneshot(empty_request(
      "GET",
      "/api/v1/jobs/55555555-5555-4555-8555-555555555555/events?wait_ms=30001",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn every_documented_section_four_request_reaches_a_running_contract_server() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = listener.local_addr().unwrap();
  let server = tokio::spawn(async move {
    axum::serve(listener, routes).await.unwrap();
  });

  let requests = documented_http_requests(DOCUMENTED_SECTION_FOUR_WORKFLOW);
  assert_eq!(requests.len(), 5, "every workflow step must be executable HTTP");
  for (request, expected_status) in requests.iter().zip([201, 201, 201, 201, 200]) {
    let status = send_documented_request(address, request).await;
    assert_eq!(
      status, expected_status,
      "{} {} must return its documented success status",
      request.method, request.target
    );
  }

  server.abort();
  let _ = server.await;
  assert_eq!(
    *application.calls.lock().unwrap(),
    [
      "create_project",
      "create_pipeline",
      "create_repository",
      "create_configuration",
      "accept_manual_trigger",
    ]
  );
}

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
    assert_required_header(documented, "Idempotency-Key", operation.idempotent_mutation);
    assert_required_header(documented, "If-Match", operation.optimistic_precondition);

    let concrete_path = concrete_path(operation.path);
    let response = routes
      .clone()
      .oneshot(
        Request::builder()
          .method(Method::from_bytes(operation.method.as_bytes()).unwrap())
          .uri(concrete_path)
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

  for (schema, body) in request_examples() {
    assert_json_matches_component(&document, schema, &body);
  }
}

fn assert_component_exists(document: &serde_json::Value, schema: &str) {
  assert!(
    document["components"]["schemas"].get(schema).is_some(),
    "missing component schema {schema}"
  );
}

fn assert_required_header(operation: &serde_json::Value, name: &str, expected: bool) {
  let actual = operation["parameters"]
    .as_array()
    .into_iter()
    .flatten()
    .any(|parameter| parameter["in"] == "header" && parameter["name"] == name && parameter["required"] == true);
  assert_eq!(actual, expected, "header drift for {name}");
}

fn concrete_path(path: &str) -> String {
  path
    .replace("{project_id}", "11111111-1111-4111-8111-111111111111")
    .replace("{pipeline_id}", "22222222-2222-4222-8222-222222222222")
    .replace("{repository_id}", "33333333-3333-4333-8333-333333333333")
    .replace("{configuration_id}", "44444444-4444-4444-8444-444444444444")
    .replace("{build_id}", "99999999-9999-4999-8999-999999999999")
    .replace("{attempt_id}", "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
    .replace("{job_id}", "55555555-5555-4555-8555-555555555555")
    .replace("{pool_id}", "66666666-6666-4666-8666-666666666666")
    .replace("{version}", "1")
}

#[test]
fn documented_http_requests_support_windows_line_endings() {
  let unix_document = DOCUMENTED_SECTION_FOUR_WORKFLOW.replace("\r\n", "\n");
  let document = unix_document.replace('\n', "\r\n");

  assert_eq!(documented_http_requests(&document).len(), 5);
}

fn json_request(method: &str, uri: &str, idempotency_key: &str, version: Option<&str>, body: &str) -> Request<Body> {
  let mut request = Request::builder()
    .method(method)
    .uri(uri)
    .header("content-type", "application/json")
    .header("idempotency-key", idempotency_key);
  if let Some(version) = version {
    request = request.header("if-match", version);
  }
  request.body(Body::from(body.to_owned())).unwrap()
}

fn empty_request(method: &str, uri: &str, mutation: Option<(&str, &str)>) -> Request<Body> {
  let mut request = Request::builder().method(method).uri(uri);
  if let Some((key, version)) = mutation {
    request = request.header("idempotency-key", key).header("if-match", version);
  }
  request.body(Body::empty()).unwrap()
}
