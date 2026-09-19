use std::{
  collections::BTreeSet,
  sync::{Arc, Mutex},
};

use async_trait::async_trait;
use axum::{
  body::{Body, to_bytes},
  http::{Method, Request, StatusCode},
};
use octacity_server_api_rest::{
  management_router_with_application,
  v1::{ErrorCode, MANAGEMENT_OPERATIONS, ManagementApplication},
};
use octacity_server_application::{
  AcceptManualTriggerCommand, ApplicationError, Command, CommandHandler, CreateBuildConfigurationCommand,
  CreatePipelineCommand, CreateProjectCommand, CreateRepositoryCommand, DeleteProjectCommand,
  GetBuildConfigurationQuery, GetPipelineQuery, GetProjectQuery, GetRepositoryQuery, JobEventPageProjection,
  JobEventProjection, ListProjectsQuery, ManualTriggerError, MoveProjectCommand,
  PublishBuildConfigurationVersionCommand, PublishPipelineVersionCommand, PublishRepositoryVersionCommand, Query,
  QueryHandler, ReadJobEventsQuery, RenameProjectCommand,
};
use tokio::{
  io::{AsyncReadExt as _, AsyncWriteExt as _},
  net::{TcpListener, TcpStream},
};
use tower::ServiceExt as _;

mod support;

use support::assert_json_matches_component;

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
unavailable_query!(GetBuildConfigurationQuery, "get_configuration");
unavailable_query!(ReadJobEventsQuery, "read_job_events");

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

#[tokio::test]
async fn every_section_four_route_dispatches_only_through_application_handlers() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    ManagementApplication::new(
      ["native".to_owned()],
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
    )
    .unwrap(),
  );
  let project_id = "11111111-1111-4111-8111-111111111111";
  let pipeline_id = "22222222-2222-4222-8222-222222222222";
  let repository_id = "33333333-3333-4333-8333-333333333333";
  let configuration_id = "44444444-4444-4444-8444-444444444444";
  let job_id = "55555555-5555-4555-8555-555555555555";

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
      "/api/v1/triggers/manual",
      "release-main-2026-09-18",
      None,
      include_str!("../fixtures/v1/accept-manual-trigger-request.json"),
    ),
    empty_request(
      "GET",
      &format!("/api/v1/jobs/{job_id}/events?after=0&limit=100&wait_ms=0"),
      None,
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
      "accept_manual_trigger",
      "read_job_events",
    ]
  );
}

#[tokio::test]
async fn transport_rejections_do_not_reach_an_application_handler() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    ManagementApplication::new(
      ["native".to_owned()],
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
    )
    .unwrap(),
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
    ManagementApplication::new(
      ["native".to_owned()],
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
    )
    .unwrap(),
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
    ManagementApplication::new(
      ["native".to_owned()],
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::new(JobEventApplication),
    )
    .unwrap(),
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
    ManagementApplication::new(
      ["native".to_owned()],
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
    )
    .unwrap(),
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
    ManagementApplication::new(
      ["native".to_owned()],
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&application),
    )
    .unwrap(),
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
    .replace("{job_id}", "55555555-5555-4555-8555-555555555555")
    .replace("{version}", "1")
}

#[derive(Debug)]
struct DocumentedHttpRequest {
  method: String,
  target: String,
  headers: Vec<(String, String)>,
  body: String,
}

fn documented_http_requests(document: &str) -> Vec<DocumentedHttpRequest> {
  let document = document.replace("\r\n", "\n");
  document
    .split("```http\n")
    .skip(1)
    .map(|remainder| remainder.split_once("\n```").expect("HTTP example fence must close").0)
    .map(|block| {
      let (head, body) = block
        .split_once("\n\n")
        .expect("HTTP example must separate headers and body");
      let mut lines = head.lines();
      let request_line = lines.next().expect("HTTP example must have a request line");
      let mut request_line = request_line.split_whitespace();
      let method = request_line.next().expect("HTTP method is required").to_owned();
      let target = request_line.next().expect("HTTP target is required").to_owned();
      assert_eq!(request_line.next(), Some("HTTP/1.1"));
      assert_eq!(request_line.next(), None);
      let headers = lines
        .map(|line| {
          let (name, value) = line.split_once(':').expect("HTTP header must contain ':'");
          (name.trim().to_owned(), value.trim().to_owned())
        })
        .collect();
      DocumentedHttpRequest {
        method,
        target,
        headers,
        body: body.trim_end().to_owned(),
      }
    })
    .collect()
}

#[test]
fn documented_http_requests_support_windows_line_endings() {
  let document = DOCUMENTED_SECTION_FOUR_WORKFLOW.replace('\n', "\r\n");

  assert_eq!(documented_http_requests(&document).len(), 5);
}

async fn send_documented_request(address: std::net::SocketAddr, request: &DocumentedHttpRequest) -> u16 {
  let mut stream = TcpStream::connect(address).await.unwrap();
  let mut encoded = format!("{} {} HTTP/1.1\r\nHost: {address}\r\n", request.method, request.target);
  for (name, value) in &request.headers {
    if !name.eq_ignore_ascii_case("host") && !name.eq_ignore_ascii_case("content-length") {
      encoded.push_str(&format!("{name}: {value}\r\n"));
    }
  }
  encoded.push_str(&format!(
    "Content-Length: {}\r\nConnection: close\r\n\r\n{}",
    request.body.len(),
    request.body
  ));
  stream.write_all(encoded.as_bytes()).await.unwrap();

  let mut response = Vec::new();
  stream.read_to_end(&mut response).await.unwrap();
  let response = String::from_utf8(response).unwrap();
  response
    .lines()
    .next()
    .expect("HTTP response must have a status line")
    .split_whitespace()
    .nth(1)
    .expect("HTTP response status is required")
    .parse()
    .unwrap()
}

fn request_examples() -> Vec<(&'static str, serde_json::Value)> {
  let mut create_pipeline: serde_json::Value =
    serde_json::from_str(include_str!("../fixtures/v1/create-pipeline-request.json")).unwrap();
  create_pipeline["dag"]["nodes"][0]["execution"]["octafile"] = serde_json::Value::Null;
  create_pipeline["dag"]["nodes"][0]["execution"]["concurrency"] = serde_json::Value::Null;
  let create_configuration: serde_json::Value =
    serde_json::from_str(include_str!("../fixtures/v1/create-build-configuration-request.json")).unwrap();
  let repository_version: serde_json::Value = serde_json::from_str(&repository_version_body()).unwrap();
  vec![
    (
      "CreateProjectRequest",
      serde_json::from_str(include_str!("../fixtures/v1/create-project-request.json")).unwrap(),
    ),
    ("RenameProjectRequest", serde_json::json!({"name": "Renamed"})),
    ("MoveProjectRequest", serde_json::json!({"parent_id": null})),
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
      "AcceptManualTriggerRequest",
      serde_json::from_str(include_str!("../fixtures/v1/accept-manual-trigger-request.json")).unwrap(),
    ),
  ]
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

fn repository_body(project_id: &str) -> String {
  let version: serde_json::Value = serde_json::from_str(&repository_version_body()).unwrap();
  serde_json::to_string(&serde_json::json!({
    "project_id": project_id,
    "name": "Source",
    "definition": version["definition"],
  }))
  .unwrap()
}

fn repository_version_body() -> String {
  r#"{"definition":{"vcs_integration_id":"55555555-5555-4555-8555-555555555555","repository_locator":"https://example.test/source.git","selection":{"allowed_references":["refs/heads/main"],"default_reference":"refs/heads/main","allow_exact_revision":true}}}"#.to_owned()
}

fn configuration_version_body() -> String {
  let create: serde_json::Value =
    serde_json::from_str(include_str!("../fixtures/v1/create-build-configuration-request.json")).unwrap();
  serde_json::to_string(&serde_json::json!({"definition": create["definition"]})).unwrap()
}
