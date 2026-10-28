use std::sync::Arc;

use axum::{
  body::Body,
  http::{Request, StatusCode},
};
use octacity_server_api_mcp::{IssueMcpApplication, IssueMcpAuthority, issue_mcp_router};
use octacity_server_application::{
  IssueHandlers, IssueReadPrincipal, ManagementVisibility, ProviderCredentialValue, Timestamp,
  TrustedNetworkManagementPolicy, testing::manual_issue_store,
};
use serde_json::json;
use tower::ServiceExt as _;

type Queries = IssueHandlers<octacity_server_application::testing::InMemoryProjectStore>;
const PROJECT: &str = "0199a6f4-d56c-7440-9aa2-6a320f862795";

fn context() -> octacity_server_application::ManagementRequestContext {
  octacity_server_application::ManagementRequestContext::trusted_network(
    octacity_server_application::ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
  )
}
async fn queries() -> Arc<Queries> {
  Arc::new(IssueHandlers::new(
    manual_issue_store(PROJECT.parse().unwrap()).await,
    Arc::new(TrustedNetworkManagementPolicy),
  ))
}
async fn seed(queries: &Arc<Queries>, title: &str, description: &str) -> octacity_server_application::IssueRevision {
  use octacity_server_application::{AuthorizedCommandHandler, CreateIssueCommand, CreateIssueIntent, IssueContent};
  AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), queries.clone())
    .handle_command(
      &context(),
      CreateIssueCommand {
        intent: CreateIssueIntent {
          project_id: PROJECT.parse().unwrap(),
          planning: Default::default(),
          content: IssueContent {
            title: title.into(),
            description: description.into(),
            acceptance: None,
            priority: None,
          },
        },
        idempotency_key: title.parse().unwrap(),
        created_at: Timestamp::from_unix_millis(10).unwrap(),
      },
    )
    .await
    .unwrap()
    .issue
}
async fn rpc(router: axum::Router, message: serde_json::Value) -> axum::response::Response {
  router
    .oneshot(
      Request::post("/mcp/issues")
        .header("authorization", "Bearer fixture-mcp-read-only")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-11-25")
        .body(Body::from(message.to_string()))
        .unwrap(),
    )
    .await
    .unwrap()
}
async fn json_response(response: axum::response::Response) -> serde_json::Value {
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024).await.unwrap();
  serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn hidden_absent_and_foreign_issue_details_have_the_same_opaque_tool_failure() {
  use octacity_server_application::{
    AuthorizedCommandHandler, CreateIssueCommand, CreateIssueIntent, CreateProjectCommand, ManagementResource,
    ManagementResourceIdentity, ManagementResourceKind, ProjectHandlers,
  };
  let project = PROJECT.parse().unwrap();
  let foreign_project = "0199a6f4-d56c-7440-9aa2-6a320f862796".parse().unwrap();
  let store = manual_issue_store(project).await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  AuthorizedCommandHandler::new(policy.clone(), Arc::new(ProjectHandlers::new(store.clone())))
    .handle_command(
      &context(),
      CreateProjectCommand {
        id: foreign_project,
        parent_id: None,
        name: serde_json::from_value(json!("Foreign tasks")).unwrap(),
        idempotency_key: "foreign-project".parse().unwrap(),
        created_at: Timestamp::from_unix_millis(2).unwrap(),
      },
    )
    .await
    .unwrap();
  let queries = Arc::new(IssueHandlers::new(store, policy.clone()));
  let public = seed(&queries, "Visible ticket", "Visible requirements").await;
  let hidden = seed(&queries, "Hidden ticket", "Hidden requirements").await;
  let foreign = AuthorizedCommandHandler::new(policy, queries.clone())
    .handle_command(
      &context(),
      CreateIssueCommand {
        intent: CreateIssueIntent {
          project_id: foreign_project,
          planning: Default::default(),
          content: hidden.content.clone(),
        },
        idempotency_key: "foreign-issue".parse().unwrap(),
        created_at: Timestamp::from_unix_millis(11).unwrap(),
      },
    )
    .await
    .unwrap()
    .issue;
  let only_public = ManagementVisibility::restricted([ManagementResource::instance(
    ManagementResourceKind::Issue,
    ManagementResourceIdentity::new(public.id.to_string()).unwrap(),
  )
  .unwrap()])
  .unwrap();
  let principal = |visibility| {
    IssueReadPrincipal::new(
      "finite-reader",
      project,
      visibility,
      Timestamp::from_unix_millis(2_000_000_000_000).unwrap(),
    )
    .unwrap()
  };
  let restricted = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(principal(only_public))),
    queries.clone(),
  ));
  let whole_project = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(principal(ManagementVisibility::all()))),
    queries,
  ));
  let absent = "0199a6f4-d56c-7440-9aa2-6a320f862799".to_owned();
  let mut failures = Vec::new();
  for (router, identity) in [
    (&restricted, hidden.id.to_string()),
    (&restricted, absent.clone()),
    (&whole_project, foreign.id.to_string()),
    (&whole_project, absent),
  ] {
    for version in [None, Some(1)] {
      let mut arguments = json!({"issue_id":identity});
      if let Some(version) = version {
        arguments["version"] = json!(version);
      }
      let response = json_response(
        rpc(
          router.clone(),
          json!({"jsonrpc":"2.0","id":9,"method":"tools/call",
        "params":{"name":"read_issue","arguments":arguments}}),
        )
        .await,
      )
      .await;
      assert_eq!(response["result"]["isError"], true);
      assert_eq!(response["result"]["structuredContent"]["status"], "not_available");
      assert!(!response.to_string().contains("Hidden requirements"));
      failures.push(response["result"].clone());
    }
  }
  assert!(failures.windows(2).all(|pair| pair[0] == pair[1]));
}

#[tokio::test]
async fn title_search_paginates_visible_rows_through_empty_match_pages_and_fences_the_query() {
  use octacity_server_application::{ManagementResource, ManagementResourceIdentity, ManagementResourceKind};
  let queries = queries().await;
  let mut visible = Vec::new();
  for title in ["Ticket alpha", "Ticket beta", "Ticket gamma"] {
    visible.push(seed(&queries, title, "Visible requirements").await);
  }
  visible.sort_by_key(|issue| issue.id);
  let query = visible.last().unwrap().content.title.clone();
  let hidden = seed(&queries, &format!("Hidden {query}"), "Hidden requirements").await;
  let visibility = ManagementVisibility::restricted(visible.iter().map(|issue| {
    ManagementResource::instance(
      ManagementResourceKind::Issue,
      ManagementResourceIdentity::new(issue.id.to_string()).unwrap(),
    )
    .unwrap()
  }))
  .unwrap();
  let principal = IssueReadPrincipal::new(
    "finite-reader",
    visible[0].project_id,
    visibility,
    Timestamp::from_unix_millis(2_000_000_000_000).unwrap(),
  )
  .unwrap();
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(principal)),
    queries,
  ));
  let mut cursor: Option<String> = None;
  let mut first_cursor = None;
  for index in 0..3 {
    let mut arguments = json!({"query":query,"limit":1});
    if let Some(cursor) = &cursor {
      arguments["cursor"] = json!(cursor);
    }
    let response = json_response(
      rpc(
        router.clone(),
        json!({"jsonrpc":"2.0","id":index,"method":"tools/call",
      "params":{"name":"search_issues","arguments":arguments}}),
      )
      .await,
    )
    .await;
    assert!(
      response.get("error").is_none(),
      "continuation must preserve the authorized search"
    );
    let page = &response["result"]["structuredContent"];
    assert_eq!(page["coverage"]["scanned_entries"], 1);
    assert_eq!(page["coverage"]["has_more"], index < 2);
    assert_eq!(page["items"].as_array().unwrap().len(), usize::from(index == 2));
    assert!(!response.to_string().contains(&hidden.id.to_string()));
    if index == 2 {
      assert_eq!(page["items"][0]["id"], visible[2].id.to_string());
    }
    cursor = page["next_cursor"].as_str().map(str::to_owned);
    if index == 0 {
      first_cursor = cursor.clone();
    }
  }
  assert!(cursor.is_none());
  let mismatch = json_response(rpc(router, json!({"jsonrpc":"2.0","id":"changed-query","method":"tools/call",
    "params":{"name":"search_issues","arguments":{"query":"Different query","limit":1,"cursor":first_cursor.unwrap()}}})).await).await;
  assert_eq!(mismatch["error"]["code"], -32602);
}

#[tokio::test]
async fn detail_read_preserves_exact_retained_revision_and_observation_identity() {
  use octacity_server_application::{AuthorizedCommandHandler, EditIssueCommand, EditIssueIntent};
  let queries = queries().await;
  let original = seed(&queries, "Original ticket", "Original requirements").await;
  let mut edited = original.content.clone();
  edited.description = "Later requirements".into();
  AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), queries.clone())
    .handle_command(
      &context(),
      EditIssueCommand {
        intent: EditIssueIntent {
          project_id: original.project_id,
          id: original.id,
          expected_version: original.version,
          content: edited,
        },
        idempotency_key: "edit-ticket".parse().unwrap(),
        edited_at: Timestamp::from_unix_millis(20).unwrap(),
      },
    )
    .await
    .unwrap();
  let principal = IssueReadPrincipal::new(
    "finite-reader",
    original.project_id,
    ManagementVisibility::all(),
    Timestamp::from_unix_millis(2_000_000_000_000).unwrap(),
  )
  .unwrap();
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(principal)),
    queries,
  ));
  let response = json_response(
    rpc(
      router,
      json!({"jsonrpc":"2.0","id":"exact-revision","method":"tools/call",
    "params":{"name":"read_issue","arguments":{"issue_id":original.id,"version":1}}}),
    )
    .await,
  )
  .await;
  assert!(
    response.get("error").is_none(),
    "an authorized exact revision must be readable"
  );
  let observed = &response["result"]["structuredContent"];
  assert_eq!(
    observed["issue"],
    json!({
      "id":original.id, "project_id":PROJECT, "number":1, "version":1, "applicability_version":1,
      "content_available":true,
      "content":{"title":"Original ticket","description":"Original requirements","acceptance":null,"priority":null},
      "workflow_id":original.workflow_id,"workflow_version":1,"status_id":original.status_id,
      "created_at":10,"updated_at":10,
    })
  );
  assert_eq!(observed["scope"]["project_id"], original.project_id.to_string());
  assert!(observed["observed_at_unix_ms"].as_i64().unwrap() > 0);
  assert_eq!(observed["content_digest"].as_str().unwrap().len(), 64);
  assert!(!response.to_string().contains("Later requirements"));
}

#[tokio::test]
async fn title_search_filters_hidden_issues_before_page_coverage_and_never_mutates() {
  use octacity_server_application::{
    AuthorizedCommandHandler, AuthorizedQueryHandler, CreateIssueCommand, CreateIssueIntent, GetIssueQuery,
    IssueContent, ManagementRequestContext, ManagementRequestId, ManagementResource, ManagementResourceIdentity,
    ManagementResourceKind,
  };
  let project = "0199a6f4-d56c-7440-9aa2-6a320f862795".parse().unwrap();
  let store = manual_issue_store(project).await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let queries = Arc::new(IssueHandlers::new(store.clone(), policy.clone()));
  let commands = AuthorizedCommandHandler::new(policy, queries.clone());
  let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  let mut issues = Vec::new();
  for (key, title) in [
    ("public", "Lease Timeout"),
    ("unrelated", "Document setup"),
    ("hidden", "Secret timeout"),
  ] {
    issues.push(
      commands
        .handle_command(
          &context,
          CreateIssueCommand {
            intent: CreateIssueIntent {
              project_id: project,
              planning: Default::default(),
              content: IssueContent {
                title: title.into(),
                description: String::new(),
                acceptance: None,
                priority: None,
              },
            },
            idempotency_key: key.parse().unwrap(),
            created_at: Timestamp::from_unix_millis(10).unwrap(),
          },
        )
        .await
        .unwrap()
        .issue,
    );
  }
  let visibility = ManagementVisibility::restricted(issues[..2].iter().map(|issue| {
    ManagementResource::instance(
      ManagementResourceKind::Issue,
      ManagementResourceIdentity::new(issue.id.to_string()).unwrap(),
    )
    .unwrap()
  }))
  .unwrap();
  let principal = IssueReadPrincipal::new(
    "finite-reader",
    project,
    visibility,
    Timestamp::from_unix_millis(2_000_000_000_000).unwrap(),
  )
  .unwrap();
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(principal)),
    queries.clone(),
  ));
  let response = router
    .oneshot(
      Request::post("/mcp/issues")
        .header("authorization", "Bearer fixture-mcp-read-only")
        .header("content-type", "application/json")
        .body(Body::from(
          json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
      "params":{"name":"search_issues","arguments":{"query":"TIMEOUT","limit":50}}})
          .to_string(),
        ))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 65_536).await.unwrap();
  let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  assert!(response.get("error").is_none(), "search must produce a scoped result");
  let page = &response["result"]["structuredContent"];
  assert_eq!(page["items"].as_array().unwrap().len(), 1);
  assert_eq!(page["items"][0]["id"], issues[0].id.to_string());
  assert_eq!(page["scope"]["project_id"], project.to_string());
  assert_eq!(page["coverage"]["scanned_entries"], 2);
  assert_eq!(page["coverage"]["has_more"], false);
  assert_eq!(page["coverage"]["consistent_snapshot"], false);
  assert!(page["next_cursor"].is_null());
  assert!(page["observed_at_unix_ms"].as_i64().unwrap() > 0);
  assert_eq!(page["content_digest"].as_str().unwrap().len(), 64);
  assert!(!response.to_string().contains("Secret timeout"));
  assert!(!response.to_string().contains(&issues[2].id.to_string()));
  let ordinary_reads = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), queries);
  for issue in issues {
    assert_eq!(
      ordinary_reads
        .handle_query(
          &context,
          GetIssueQuery {
            project_id: project,
            id: issue.id,
            version: None,
          }
        )
        .await
        .unwrap(),
      issue
    );
  }
}

struct NoVerifiedIdentity;

struct CurrentAuthority(std::sync::RwLock<Option<IssueReadPrincipal>>);
#[async_trait::async_trait]
impl IssueMcpAuthority for CurrentAuthority {
  async fn authenticate(&self, credential: &ProviderCredentialValue, _: Timestamp) -> Option<IssueReadPrincipal> {
    if credential.expose_for_installation() != b"fixture-mcp-read-only" {
      return None;
    }
    self.0.read().unwrap().clone()
  }
}

struct PausedQueries {
  inner: Arc<IssueHandlers<octacity_server_application::testing::InMemoryProjectStore>>,
  started: Arc<tokio::sync::Notify>,
  resume: Arc<tokio::sync::Notify>,
}
#[async_trait::async_trait]
impl octacity_server_application::ManagementQueryUseCase<octacity_server_application::GetIssueQuery> for PausedQueries {
  type Error = octacity_server_application::ApplicationError;
  async fn execute_management_query(
    &self,
    context: &octacity_server_application::ManagementRequestContext,
    grant: &octacity_server_application::ManagementAuthorizationGrant,
    query: octacity_server_application::GetIssueQuery,
  ) -> Result<octacity_server_application::IssueRevision, Self::Error> {
    self.started.notify_one();
    self.resume.notified().await;
    self.inner.execute_management_query(context, grant, query).await
  }
}
#[async_trait::async_trait]
impl octacity_server_application::ManagementQueryUseCase<octacity_server_application::ListProjectIssuesQuery>
  for PausedQueries
{
  type Error = octacity_server_application::ApplicationError;
  async fn execute_management_query(
    &self,
    context: &octacity_server_application::ManagementRequestContext,
    grant: &octacity_server_application::ManagementAuthorizationGrant,
    query: octacity_server_application::ListProjectIssuesQuery,
  ) -> Result<octacity_server_application::IssuePage, Self::Error> {
    self.started.notify_one();
    self.resume.notified().await;
    self.inner.execute_management_query(context, grant, query).await
  }
}

#[tokio::test]
async fn current_credential_project_visibility_and_expiry_are_rechecked_after_an_asynchronous_read() {
  let expiry = Timestamp::from_unix_millis(2_000_000_000_000).unwrap();
  for changed in [
    None,
    Some(
      IssueReadPrincipal::new(
        "finite-reader",
        PROJECT.parse().unwrap(),
        ManagementVisibility::none(),
        expiry,
      )
      .unwrap(),
    ),
    Some(
      IssueReadPrincipal::new(
        "finite-reader",
        "0199a6f4-d56c-7440-9aa2-6a320f862796".parse().unwrap(),
        ManagementVisibility::all(),
        expiry,
      )
      .unwrap(),
    ),
    Some(
      IssueReadPrincipal::new(
        "finite-reader",
        PROJECT.parse().unwrap(),
        ManagementVisibility::all(),
        Timestamp::from_unix_millis(1).unwrap(),
      )
      .unwrap(),
    ),
  ] {
    let project = "0199a6f4-d56c-7440-9aa2-6a320f862795".parse().unwrap();
    let store = manual_issue_store(project).await;
    let started = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    let queries = Arc::new(PausedQueries {
      inner: Arc::new(IssueHandlers::new(store, Arc::new(TrustedNetworkManagementPolicy))),
      started: started.clone(),
      resume: resume.clone(),
    });
    let principal = IssueReadPrincipal::new(
      "finite-reader",
      project,
      ManagementVisibility::all(),
      Timestamp::from_unix_millis(2_000_000_000_000).unwrap(),
    )
    .unwrap();
    let authority = Arc::new(CurrentAuthority(std::sync::RwLock::new(Some(principal))));
    let router = issue_mcp_router(IssueMcpApplication::new(authority.clone(), queries));
    let pending = tokio::spawn(
      router.oneshot(
        Request::post("/mcp/issues")
          .header("authorization", "Bearer fixture-mcp-read-only")
          .header("content-type", "application/json")
          .body(Body::from(
            json!({"jsonrpc":"2.0","id":"in-flight-read","method":"tools/call",
      "params":{"name":"search_issues","arguments":{"query":"timeout"}}})
            .to_string(),
          ))
          .unwrap(),
      ),
    );
    tokio::time::timeout(std::time::Duration::from_secs(2), started.notified())
      .await
      .unwrap();
    *authority.0.write().unwrap() = changed;
    resume.notify_one();
    let response = pending.await.unwrap().unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let bytes = axum::body::to_bytes(response.into_body(), 65_536).await.unwrap();
    assert!(
      bytes.is_empty(),
      "a revoked read must not release result or coverage metadata"
    );
  }
}

#[async_trait::async_trait]
impl IssueMcpAuthority for NoVerifiedIdentity {
  async fn authenticate(&self, _: &ProviderCredentialValue, _: Timestamp) -> Option<IssueReadPrincipal> {
    None
  }
}

struct FiniteReadAuthority(IssueReadPrincipal);
#[async_trait::async_trait]
impl IssueMcpAuthority for FiniteReadAuthority {
  async fn authenticate(&self, credential: &ProviderCredentialValue, _: Timestamp) -> Option<IssueReadPrincipal> {
    (credential.expose_for_installation() == b"fixture-mcp-read-only").then(|| self.0.clone())
  }
}

#[tokio::test]
async fn scoped_connection_discovers_only_bounded_issue_read_tools() {
  let project = "0199a6f4-d56c-7440-9aa2-6a320f862795".parse().unwrap();
  let store = manual_issue_store(project).await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let queries = Arc::new(IssueHandlers::new(store, policy));
  let principal = IssueReadPrincipal::new(
    "finite-reader",
    project,
    ManagementVisibility::all(),
    Timestamp::from_unix_millis(2_000_000_000_000).unwrap(),
  )
  .unwrap();
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(principal)),
    queries,
  ));
  let response = router
    .oneshot(
      Request::post("/mcp/issues")
        .header("authorization", "Bearer fixture-mcp-read-only")
        .header("content-type", "application/json")
        .body(Body::from(
          json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}).to_string(),
        ))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 16_384).await.unwrap();
  let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  let tools = response["result"]["tools"]
    .as_array()
    .expect("a successful native tool catalog");
  assert_eq!(
    tools
      .iter()
      .map(|tool| tool["name"].as_str().unwrap())
      .collect::<Vec<_>>(),
    ["read_issue", "search_issues"]
  );
  for tool in tools {
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    assert!(tool["inputSchema"]["properties"].get("project_id").is_none());
  }
}

#[tokio::test]
async fn scoped_connection_initializes_with_read_only_tools_capability() {
  let project = "0199a6f4-d56c-7440-9aa2-6a320f862795".parse().unwrap();
  let store = manual_issue_store(project).await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let queries = Arc::new(IssueHandlers::new(store, policy));
  let principal = IssueReadPrincipal::new(
    "finite-reader",
    project,
    ManagementVisibility::all(),
    Timestamp::from_unix_millis(2_000_000_000_000).unwrap(),
  )
  .unwrap();
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(principal)),
    queries,
  ));
  let response = router.oneshot(Request::post("/mcp/issues")
    .header("authorization", "Bearer fixture-mcp-read-only")
    .header("content-type", "application/json")
    .header("accept", "application/json, text/event-stream")
    .body(Body::from(json!({
      "jsonrpc":"2.0", "id":"initialize-connection", "method":"initialize",
      "params":{"protocolVersion":"2025-11-25", "capabilities":{}, "clientInfo":{"name":"finite-client","version":"1"}}
    }).to_string())).unwrap()).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = axum::body::to_bytes(response.into_body(), 16_384).await.unwrap();
  let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert_eq!(response["id"], "initialize-connection");
  assert_eq!(response["result"]["protocolVersion"], "2025-11-25");
  assert_eq!(response["result"]["capabilities"], json!({"tools":{}}));
  assert_eq!(response["result"]["serverInfo"]["name"], "octacity");
  assert!(!response.to_string().contains("fixture-mcp-read-only"));
}

#[tokio::test]
async fn mcp_never_inherits_trusted_network_or_unverified_identity_headers() {
  let project = "0199a6f4-d56c-7440-9aa2-6a320f862795".parse().unwrap();
  let store = manual_issue_store(project).await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let queries = Arc::new(IssueHandlers::new(store, policy));
  let router = issue_mcp_router(IssueMcpApplication::new(Arc::new(NoVerifiedIdentity), queries));
  for bearer in [None, Some("Bearer invalid-connection-token")] {
    let mut request = Request::post("/mcp/issues")
      .header("content-type", "application/json")
      .header("accept", "application/json, text/event-stream")
      .header("x-authenticated-subject", "local-administrator")
      .header("x-project-id", project.to_string());
    if let Some(bearer) = bearer {
      request = request.header("authorization", bearer);
    }
    let response = router.clone().oneshot(request.body(Body::from(json!({
      "jsonrpc":"2.0", "id":1, "method":"initialize",
      "params":{"protocolVersion":"2025-11-25", "capabilities":{}, "clientInfo":{"name":"finite-client","version":"1"}}
    }).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
  }
}

struct UnavailableQueries;
#[async_trait::async_trait]
impl octacity_server_application::ManagementQueryUseCase<octacity_server_application::ListProjectIssuesQuery>
  for UnavailableQueries
{
  type Error = octacity_server_application::ApplicationError;
  async fn execute_management_query(
    &self,
    _: &octacity_server_application::ManagementRequestContext,
    _: &octacity_server_application::ManagementAuthorizationGrant,
    _: octacity_server_application::ListProjectIssuesQuery,
  ) -> Result<octacity_server_application::IssuePage, Self::Error> {
    Err(Self::Error::capability_unavailable())
  }
}
#[async_trait::async_trait]
impl octacity_server_application::ManagementQueryUseCase<octacity_server_application::GetIssueQuery>
  for UnavailableQueries
{
  type Error = octacity_server_application::ApplicationError;
  async fn execute_management_query(
    &self,
    _: &octacity_server_application::ManagementRequestContext,
    _: &octacity_server_application::ManagementAuthorizationGrant,
    _: octacity_server_application::GetIssueQuery,
  ) -> Result<octacity_server_application::IssueRevision, Self::Error> {
    Err(Self::Error::capability_unavailable())
  }
}
fn read_principal() -> IssueReadPrincipal {
  IssueReadPrincipal::new(
    "finite-reader",
    PROJECT.parse().unwrap(),
    ManagementVisibility::all(),
    Timestamp::from_unix_millis(2_000_000_000_000).unwrap(),
  )
  .unwrap()
}
#[tokio::test]
async fn unavailable_search_is_a_tool_failure_instead_of_an_empty_successful_page() {
  let authority = Arc::new(FiniteReadAuthority(read_principal()));
  let empty = issue_mcp_router(IssueMcpApplication::new(authority.clone(), queries().await));
  let unavailable = issue_mcp_router(IssueMcpApplication::new(authority, Arc::new(UnavailableQueries)));
  let message = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
    "params":{"name":"search_issues","arguments":{"query":"duplicate"}}});
  let successful = json_response(rpc(empty, message.clone()).await).await;
  assert_eq!(successful["result"]["isError"], false);
  assert_eq!(successful["result"]["structuredContent"]["items"], json!([]));
  let failed = json_response(rpc(unavailable, message).await).await;
  assert_eq!(failed["result"]["isError"], true);
  assert_eq!(failed["result"]["structuredContent"]["status"], "unavailable");
  assert!(failed["result"]["structuredContent"].get("items").is_none());
  assert!(failed["result"]["structuredContent"].get("coverage").is_none());
}

#[tokio::test]
async fn mutation_tools_and_caller_project_overrides_are_refused_without_changes() {
  let queries = queries().await;
  let original = seed(&queries, "Preserved ticket", "Requirements").await;
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(read_principal())),
    queries.clone(),
  ));
  for (name, arguments) in [
    ("delete_issue", json!({"issue_id":original.id})),
    ("create_issue", json!({"title":"Injected task"})),
    (
      "read_issue",
      json!({"issue_id":original.id,"project_id":"0199a6f4-d56c-7440-9aa2-6a320f862796"}),
    ),
    ("search_issues", json!({"query":"","project_id":PROJECT})),
  ] {
    let response = json_response(
      rpc(
        router.clone(),
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
      "params":{"name":name,"arguments":arguments}}),
      )
      .await,
    )
    .await;
    assert_eq!(response["error"]["code"], -32602);
  }
  let response = json_response(
    rpc(
      router,
      json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
    "params":{"name":"read_issue","arguments":{"issue_id":original.id}}}),
    )
    .await,
  )
  .await;
  let issue = &response["result"]["structuredContent"]["issue"];
  assert_eq!(issue["id"], original.id.to_string());
  assert_eq!(issue["version"], 1);
  assert_eq!(
    issue["content"],
    json!({"title":"Preserved ticket","description":"Requirements","acceptance":null,"priority":null})
  );
}

#[tokio::test]
async fn cancellation_stops_an_outstanding_read_and_does_not_release_a_page() {
  let started = Arc::new(tokio::sync::Notify::new());
  let resume = Arc::new(tokio::sync::Notify::new());
  let handlers = Arc::new(PausedQueries {
    inner: queries().await,
    started: started.clone(),
    resume,
  });
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(read_principal())),
    handlers,
  ));
  let pending = tokio::spawn(rpc(
    router.clone(),
    json!({"jsonrpc":"2.0","id":"pending-read","method":"tools/call",
    "params":{"name":"search_issues","arguments":{"query":"duplicate"}}}),
  ));
  tokio::time::timeout(std::time::Duration::from_secs(2), started.notified())
    .await
    .unwrap();
  let cancelled = rpc(
    router.clone(),
    json!({"jsonrpc":"2.0","method":"notifications/cancelled",
    "params":{"requestId":"pending-read","reason":"Changed investigation"}}),
  )
  .await;
  assert_eq!(cancelled.status(), StatusCode::ACCEPTED);
  let response = json_response(
    tokio::time::timeout(std::time::Duration::from_secs(1), pending)
      .await
      .unwrap()
      .unwrap(),
  )
  .await;
  assert_eq!(response["result"]["isError"], true);
  assert_eq!(response["result"]["structuredContent"]["status"], "cancelled");
  assert!(response["result"]["structuredContent"].get("items").is_none());
  // Completing a cancelled request releases its ID for a subsequent invocation.
  let read = tokio::spawn(rpc(
    router.clone(),
    json!({"jsonrpc":"2.0","id":"pending-read","method":"tools/call",
    "params":{"name":"search_issues","arguments":{"query":"duplicate"}}}),
  ));
  tokio::time::timeout(std::time::Duration::from_secs(2), started.notified())
    .await
    .unwrap();
  rpc(
    router,
    json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"pending-read"}}),
  )
  .await;
  assert_eq!(read.await.unwrap().status(), StatusCode::OK);
}

#[tokio::test]
async fn stateless_http_accepts_initialized_notification_and_ping_but_refuses_browser_origins() {
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(read_principal())),
    queries().await,
  ));
  let initialized = rpc(
    router.clone(),
    json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
  )
  .await;
  assert_eq!(initialized.status(), StatusCode::ACCEPTED);
  assert!(
    axum::body::to_bytes(initialized.into_body(), 16)
      .await
      .unwrap()
      .is_empty()
  );
  assert_eq!(
    json_response(rpc(router.clone(), json!({"jsonrpc":"2.0","id":1,"method":"ping"})).await).await["result"],
    json!({})
  );
  let browser = router
    .oneshot(
      Request::post("/mcp/issues")
        .header("authorization", "Bearer fixture-mcp-read-only")
        .header("origin", "https://untrusted.example")
        .header("content-type", "application/json")
        .body(Body::from(
          json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string(),
        ))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(browser.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn malformed_envelopes_and_unbounded_tool_arguments_never_produce_a_read_result() {
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(read_principal())),
    queries().await,
  ));
  for message in [
    json!({"jsonrpc":"2.0","id":null,"method":"tools/list"}),
    json!({"jsonrpc":"2.0","id":{},"method":"tools/list"}),
    json!({"jsonrpc":"2.0","id":1.5,"method":"tools/list"}),
    json!({"jsonrpc":"2.0","id":"a".repeat(257),"method":"tools/list"}),
    json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"search_issues","arguments":{"query":""}}}),
  ] {
    let response = json_response(rpc(router.clone(), message).await).await;
    assert_eq!(response["error"]["code"], -32600);
    assert!(
      response["id"].is_null(),
      "an invalid request ID must not become an invalid response ID"
    );
    assert!(response.get("result").is_none());
  }
  for arguments in [
    json!({"query":"a".repeat(257)}),
    json!({"query":"\n"}),
    json!({"query":"","limit":0}),
    json!({"query":"","limit":51}),
    json!({"query":"","cursor":"a".repeat(1025)}),
  ] {
    let response = json_response(
      rpc(
        router.clone(),
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
      "params":{"name":"search_issues","arguments":arguments}}),
      )
      .await,
    )
    .await;
    assert_eq!(response["error"]["code"], -32602);
  }
}

#[tokio::test(start_paused = true)]
async fn stalled_reads_have_a_finite_deadline_and_cannot_occupy_request_ids_forever() {
  let started = Arc::new(tokio::sync::Notify::new());
  let resume = Arc::new(tokio::sync::Notify::new());
  let handlers = Arc::new(PausedQueries {
    inner: queries().await,
    started: started.clone(),
    resume,
  });
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(read_principal())),
    handlers,
  ));
  let pending = tokio::spawn(rpc(
    router.clone(),
    json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
    "params":{"name":"search_issues","arguments":{"query":"duplicate"}}}),
  ));
  started.notified().await;
  tokio::time::advance(std::time::Duration::from_secs(31)).await;
  tokio::task::yield_now().await;
  assert!(
    pending.is_finished(),
    "an unanswered store read must stop within thirty seconds"
  );
  let response = pending.await.unwrap();
  assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
  assert!(axum::body::to_bytes(response.into_body(), 16).await.unwrap().is_empty());
}

#[tokio::test]
async fn oversized_detail_returns_explicit_failure_without_silent_truncation() {
  let handlers = queries().await;
  let issue = seed(&handlers, "Large permitted document", &"\n".repeat(65_536)).await;
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(read_principal())),
    handlers,
  ));
  let response = rpc(
    router,
    json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
    "params":{"name":"read_issue","arguments":{"issue_id":issue.id}}}),
  )
  .await;
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
  assert!(
    bytes.len() <= 256 * 1024,
    "a valid document can still exceed the encoded MCP response limit"
  );
  let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(response["result"]["isError"], true);
  assert_eq!(response["result"]["structuredContent"]["status"], "too_large");
  assert!(response["result"]["structuredContent"].get("issue").is_none());
}

#[tokio::test]
async fn unavailable_detail_is_distinct_from_a_hidden_or_absent_candidate() {
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(read_principal())),
    Arc::new(UnavailableQueries),
  ));
  let response = json_response(
    rpc(
      router,
      json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
    "params":{"name":"read_issue","arguments":{"issue_id":"0199a6f4-d56c-7440-9aa2-6a320f862799"}}}),
    )
    .await,
  )
  .await;
  assert_eq!(response["result"]["structuredContent"]["status"], "unavailable");
}

#[tokio::test]
async fn cancellation_is_scoped_to_the_verified_connection_not_the_request_id_alone() {
  let started = Arc::new(tokio::sync::Notify::new());
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(TwoConnections),
    Arc::new(PausedQueries {
      inner: queries().await,
      started: started.clone(),
      resume: Arc::new(tokio::sync::Notify::new()),
    }),
  ));
  let pending = tokio::spawn(rpc(
    router.clone(),
    json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
    "params":{"name":"search_issues","arguments":{"query":""}}}),
  ));
  started.notified().await;
  let notification = json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}});
  let response = router
    .clone()
    .oneshot(
      Request::post("/mcp/issues")
        .header("authorization", "Bearer independent-mcp-reader")
        .header("content-type", "application/json")
        .body(Body::from(notification.to_string()))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::ACCEPTED);
  tokio::task::yield_now().await;
  assert!(
    !pending.is_finished(),
    "another connection cannot cancel this reader's call"
  );
  rpc(router, notification).await;
  assert_eq!(
    json_response(pending.await.unwrap()).await["result"]["structuredContent"]["status"],
    "cancelled"
  );
}

#[tokio::test(start_paused = true)]
async fn active_reads_are_bounded_while_cancellation_remains_available_at_capacity() {
  let started = Arc::new(tokio::sync::Notify::new());
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(read_principal())),
    Arc::new(PausedQueries {
      inner: queries().await,
      started: started.clone(),
      resume: Arc::new(tokio::sync::Notify::new()),
    }),
  ));
  let mut pending = Vec::new();
  for id in 0..64 {
    pending.push(tokio::spawn(rpc(
      router.clone(),
      json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
      "params":{"name":"search_issues","arguments":{"query":""}}}),
    )));
    tokio::time::timeout(std::time::Duration::from_secs(1), started.notified())
      .await
      .unwrap();
  }
  let over_capacity = rpc(
    router.clone(),
    json!({"jsonrpc":"2.0","id":64,"method":"tools/call",
    "params":{"name":"search_issues","arguments":{"query":""}}}),
  );
  // The rejected request must return immediately rather than join the stalled queue.
  let response = tokio::time::timeout(std::time::Duration::from_secs(1), over_capacity).await;
  assert!(
    response.is_ok(),
    "Issue MCP must bound active reads instead of building an unbounded queue"
  );
  assert_eq!(response.unwrap().status(), StatusCode::TOO_MANY_REQUESTS);
  for (id, call) in pending.into_iter().enumerate() {
    assert_eq!(
      rpc(
        router.clone(),
        json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":id}})
      )
      .await
      .status(),
      StatusCode::ACCEPTED
    );
    assert_eq!(
      json_response(call.await.unwrap()).await["result"]["structuredContent"]["status"],
      "cancelled"
    );
  }
}

#[tokio::test]
async fn native_http_enforces_content_type_protocol_version_and_non_cacheable_results() {
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(read_principal())),
    queries().await,
  ));
  let message = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"});
  for (content_type, version, status) in [
    ("text/plain", "2025-11-25", StatusCode::UNSUPPORTED_MEDIA_TYPE),
    ("application/json", "1900-01-01", StatusCode::BAD_REQUEST),
  ] {
    let response = router
      .clone()
      .oneshot(
        Request::post("/mcp/issues")
          .header("authorization", "Bearer fixture-mcp-read-only")
          .header("content-type", content_type)
          .header("mcp-protocol-version", version)
          .body(Body::from(message.to_string()))
          .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(response.status(), status);
  }
  let response = rpc(router, message).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
}

#[tokio::test]
async fn initialization_negotiates_the_supported_version_and_unknown_notifications_are_ignored() {
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(read_principal())),
    queries().await,
  ));
  let negotiated = json_response(
    rpc(
      router.clone(),
      json!({"jsonrpc":"2.0","id":1,"method":"initialize",
    "params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"older-client","version":"1"},"capabilities":{}}}),
    )
    .await,
  )
  .await;
  assert_eq!(negotiated["result"]["protocolVersion"], "2025-11-25");
  let malformed = json_response(
    rpc(
      router.clone(),
      json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    )
    .await,
  )
  .await;
  assert_eq!(malformed["error"]["code"], -32602);
  let ignored = rpc(
    router,
    json!({"jsonrpc":"2.0","method":"notifications/client_extension","params":{}}),
  )
  .await;
  assert_eq!(ignored.status(), StatusCode::ACCEPTED);
  assert!(axum::body::to_bytes(ignored.into_body(), 16).await.unwrap().is_empty());
}

#[tokio::test]
async fn independent_native_clients_can_use_the_same_request_id_without_cross_cancellation() {
  let started = Arc::new(tokio::sync::Notify::new());
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(read_principal())),
    Arc::new(PausedQueries {
      inner: queries().await,
      started: started.clone(),
      resume: Arc::new(tokio::sync::Notify::new()),
    }),
  ));
  let init = json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25"}});
  let first = rpc(router.clone(), init.clone()).await;
  let second = rpc(router.clone(), init).await;
  let first_session = first
    .headers()
    .get("mcp-session-id")
    .expect("native clients need independent request correlation")
    .clone();
  let second_session = second.headers().get("mcp-session-id").unwrap().clone();
  assert_ne!(first_session, second_session);
  let message =
    json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search_issues","arguments":{"query":""}}});
  let mut pending = Vec::new();
  for session in [&first_session, &second_session] {
    pending.push(tokio::spawn(
      router.clone().oneshot(
        Request::post("/mcp/issues")
          .header("authorization", "Bearer fixture-mcp-read-only")
          .header("content-type", "application/json")
          .header("mcp-session-id", session)
          .body(Body::from(message.to_string()))
          .unwrap(),
      ),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(1), started.notified())
      .await
      .unwrap();
  }
  for (index, session) in [&first_session, &second_session].into_iter().enumerate() {
    let cancelled = router
      .clone()
      .oneshot(
        Request::post("/mcp/issues")
          .header("authorization", "Bearer fixture-mcp-read-only")
          .header("content-type", "application/json")
          .header("mcp-session-id", session)
          .body(Body::from(
            json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}).to_string(),
          ))
          .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(cancelled.status(), StatusCode::ACCEPTED);
    tokio::task::yield_now().await;
    if index == 0 {
      assert!(
        !pending[1].is_finished(),
        "cancellation must not cross native client sessions"
      );
    }
  }
  for call in pending {
    assert_eq!(
      json_response(call.await.unwrap().unwrap()).await["result"]["structuredContent"]["status"],
      "cancelled"
    );
  }
}

#[tokio::test(start_paused = true)]
async fn protocol_sessions_are_bounded_in_time_and_never_replace_connection_authentication() {
  let authority = Arc::new(CurrentAuthority(std::sync::RwLock::new(Some(read_principal()))));
  let router = issue_mcp_router(IssueMcpApplication::new(authority.clone(), queries().await));
  let init = rpc(
    router.clone(),
    json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
  )
  .await;
  let session = init.headers().get("mcp-session-id").unwrap().clone();
  let call = |session: axum::http::HeaderValue| {
    Request::post("/mcp/issues")
      .header("authorization", "Bearer fixture-mcp-read-only")
      .header("content-type", "application/json")
      .header("mcp-session-id", session)
      .body(Body::from(
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string(),
      ))
      .unwrap()
  };
  assert_eq!(
    router.clone().oneshot(call(session.clone())).await.unwrap().status(),
    StatusCode::OK
  );
  *authority.0.write().unwrap() = Some(
    IssueReadPrincipal::new(
      "another-reader",
      PROJECT.parse().unwrap(),
      ManagementVisibility::all(),
      Timestamp::from_unix_millis(2_000_000_000_000).unwrap(),
    )
    .unwrap(),
  );
  assert_eq!(
    router.clone().oneshot(call(session.clone())).await.unwrap().status(),
    StatusCode::NOT_FOUND
  );
  *authority.0.write().unwrap() = None;
  assert_eq!(
    router.clone().oneshot(call(session.clone())).await.unwrap().status(),
    StatusCode::UNAUTHORIZED
  );
  *authority.0.write().unwrap() = Some(read_principal());
  tokio::time::advance(std::time::Duration::from_secs(30 * 60 + 1)).await;
  assert_eq!(
    router.oneshot(call(session)).await.unwrap().status(),
    StatusCode::NOT_FOUND
  );
}

#[tokio::test]
async fn detail_wire_contract_preserves_explicit_optional_planning_fields() {
  use octacity_server_application::{
    AuthorizedCommandHandler, CreateIssueCommand, CreateIssueIntent, IssueContent, IssuePlanning,
  };
  let queries = queries().await;
  let original = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), queries.clone())
    .handle_command(
      &context(),
      CreateIssueCommand {
        intent: CreateIssueIntent {
          project_id: PROJECT.parse().unwrap(),
          planning: IssuePlanning {
            due_date: Some("2026-10-28".into()),
            ..Default::default()
          },
          content: IssueContent {
            title: "Planned ticket".into(),
            description: "Requirements".into(),
            acceptance: Some("Reviewed criteria".into()),
            priority: Some(2),
          },
        },
        idempotency_key: "planned-ticket".parse().unwrap(),
        created_at: Timestamp::from_unix_millis(10).unwrap(),
      },
    )
    .await
    .unwrap()
    .issue;
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(read_principal())),
    queries,
  ));
  let response = json_response(
    rpc(
      router,
      json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
    "params":{"name":"read_issue","arguments":{"issue_id":original.id}}}),
    )
    .await,
  )
  .await;
  assert_eq!(
    response["result"]["structuredContent"]["issue"],
    json!({
      "id":original.id,"project_id":PROJECT,"number":1,"version":1,"applicability_version":1,
      "content_available":true,
      "content":{"title":"Planned ticket","description":"Requirements","acceptance":"Reviewed criteria","priority":2},
      "planning":{"definition_version":0,"assignee":null,"labels":[],"due_date":"2026-10-28","values":{},"parent_id":null},
      "workflow_id":original.workflow_id,"workflow_version":1,"status_id":original.status_id,"created_at":10,"updated_at":10,
    })
  );
}

async fn initialize_session(router: axum::Router) -> axum::http::HeaderValue {
  let response = rpc(
    router,
    json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
  )
  .await;
  assert_eq!(response.status(), StatusCode::OK);
  response.headers().get("mcp-session-id").unwrap().clone()
}
fn session_request(
  method: axum::http::Method,
  session: &axum::http::HeaderValue,
  message: Option<serde_json::Value>,
) -> Request<Body> {
  let mut request = Request::builder()
    .method(method)
    .uri("/mcp/issues")
    .header("authorization", "Bearer fixture-mcp-read-only")
    .header("mcp-session-id", session);
  if message.is_some() {
    request = request.header("content-type", "application/json");
  }
  request
    .body(Body::from(message.map(|value| value.to_string()).unwrap_or_default()))
    .unwrap()
}

#[tokio::test]
async fn session_termination_checks_current_owner_cancels_its_reads_and_preserves_other_sessions() {
  use axum::http::Method;
  let authority = Arc::new(CurrentAuthority(std::sync::RwLock::new(Some(read_principal()))));
  let started = Arc::new(tokio::sync::Notify::new());
  let router = issue_mcp_router(IssueMcpApplication::new(
    authority.clone(),
    Arc::new(PausedQueries {
      inner: queries().await,
      started: started.clone(),
      resume: Arc::new(tokio::sync::Notify::new()),
    }),
  ));
  let session = initialize_session(router.clone()).await;
  let other_session = initialize_session(router.clone()).await;
  *authority.0.write().unwrap() = Some(
    IssueReadPrincipal::new(
      "another-reader",
      PROJECT.parse().unwrap(),
      ManagementVisibility::all(),
      Timestamp::from_unix_millis(2_000_000_000_000).unwrap(),
    )
    .unwrap(),
  );
  assert_eq!(
    router
      .clone()
      .oneshot(session_request(Method::DELETE, &session, None))
      .await
      .unwrap()
      .status(),
    StatusCode::NOT_FOUND
  );
  *authority.0.write().unwrap() = None;
  assert_eq!(
    router
      .clone()
      .oneshot(session_request(Method::DELETE, &session, None))
      .await
      .unwrap()
      .status(),
    StatusCode::UNAUTHORIZED
  );
  *authority.0.write().unwrap() = Some(read_principal());
  let read = tokio::spawn(router.clone().oneshot(session_request(
    Method::POST,
    &session,
    Some(json!({
      "jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search_issues","arguments":{"query":""}}
    })),
  )));
  tokio::time::timeout(std::time::Duration::from_secs(2), started.notified())
    .await
    .unwrap();
  let closed = router
    .clone()
    .oneshot(session_request(Method::DELETE, &session, None))
    .await
    .unwrap();
  assert_eq!(closed.status(), StatusCode::NO_CONTENT);
  assert!(axum::body::to_bytes(closed.into_body(), 16).await.unwrap().is_empty());
  let cancelled = json_response(
    tokio::time::timeout(std::time::Duration::from_secs(1), read)
      .await
      .unwrap()
      .unwrap()
      .unwrap(),
  )
  .await;
  assert_eq!(cancelled["result"]["structuredContent"]["status"], "cancelled");
  assert!(cancelled["result"]["structuredContent"].get("items").is_none());
  let message = Some(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
  assert_eq!(
    router
      .clone()
      .oneshot(session_request(Method::POST, &session, message.clone()))
      .await
      .unwrap()
      .status(),
    StatusCode::NOT_FOUND
  );
  assert_eq!(
    router
      .oneshot(session_request(Method::POST, &other_session, message))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
}

struct TwoConnections;
#[async_trait::async_trait]
impl IssueMcpAuthority for TwoConnections {
  async fn authenticate(&self, credential: &ProviderCredentialValue, _: Timestamp) -> Option<IssueReadPrincipal> {
    match credential.expose_for_installation() {
      b"fixture-mcp-read-only" => Some(read_principal()),
      b"independent-mcp-reader" => Some(
        IssueReadPrincipal::new(
          "second-reader",
          PROJECT.parse().unwrap(),
          ManagementVisibility::all(),
          Timestamp::from_unix_millis(2_000_000_000_000).unwrap(),
        )
        .unwrap(),
      ),
      _ => None,
    }
  }
}

#[tokio::test]
async fn session_capacity_is_released_on_termination_and_one_connection_cannot_exhaust_other_readers() {
  use axum::http::Method;
  let router = issue_mcp_router(IssueMcpApplication::new(Arc::new(TwoConnections), queries().await));
  // More completed clients than the entire global ceiling must leave no occupied slots.
  for _ in 0..1025 {
    let session = initialize_session(router.clone()).await;
    assert_eq!(
      router
        .clone()
        .oneshot(session_request(Method::DELETE, &session, None))
        .await
        .unwrap()
        .status(),
      StatusCode::NO_CONTENT
    );
  }
  let mut sessions = Vec::new();
  for _ in 0..64 {
    sessions.push(initialize_session(router.clone()).await);
  }
  let init = json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25"}});
  assert_eq!(
    rpc(router.clone(), init.clone()).await.status(),
    StatusCode::TOO_MANY_REQUESTS
  );
  let independent = router
    .clone()
    .oneshot(
      Request::post("/mcp/issues")
        .header("authorization", "Bearer independent-mcp-reader")
        .header("content-type", "application/json")
        .body(Body::from(init.to_string()))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    independent.status(),
    StatusCode::OK,
    "one reader's full quota cannot prevent another connection initializing"
  );
  assert_eq!(
    router
      .clone()
      .oneshot(session_request(Method::DELETE, &sessions[0], None))
      .await
      .unwrap()
      .status(),
    StatusCode::NO_CONTENT
  );
  assert_eq!(
    rpc(router, init).await.status(),
    StatusCode::OK,
    "termination must immediately release the connection's slot"
  );
}

#[tokio::test]
async fn a_session_closed_while_a_request_body_is_pending_cannot_admit_a_read_afterwards() {
  use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _};
  let router = issue_mcp_router(IssueMcpApplication::new(
    Arc::new(FiniteReadAuthority(read_principal())),
    queries().await,
  ));
  let session = initialize_session(router.clone()).await;
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = listener.local_addr().unwrap();
  let server = tokio::spawn({
    let router = router.clone();
    async move {
      axum::serve(listener, router).await.unwrap();
    }
  });
  let mut socket = tokio::io::BufReader::new(tokio::net::TcpStream::connect(address).await.unwrap());
  let body =
    json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search_issues","arguments":{"query":""}}})
      .to_string();
  socket.get_mut().write_all(format!(
    "POST /mcp/issues HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer fixture-mcp-read-only\r\nMcp-Session-Id: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n",
    session.to_str().unwrap(),body.len()).as_bytes()).await.unwrap();
  // The interim response proves that authentication and session checking passed
  // and the real HTTP handler is polling the still-incomplete request body.
  tokio::time::timeout(std::time::Duration::from_secs(2), async {
    let mut line = String::new();
    assert!(
      socket.read_line(&mut line).await.unwrap() > 0,
      "HTTP stream closed before the body barrier"
    );
    assert!(line.starts_with("HTTP/1.1 100 "), "unexpected interim response: {line}");
    loop {
      line.clear();
      assert!(
        socket.read_line(&mut line).await.unwrap() > 0,
        "HTTP stream closed before the body barrier"
      );
      if line == "\r\n" {
        break;
      }
    }
  })
  .await
  .unwrap();
  assert_eq!(
    router
      .oneshot(session_request(axum::http::Method::DELETE, &session, None))
      .await
      .unwrap()
      .status(),
    StatusCode::NO_CONTENT
  );
  socket.get_mut().write_all(body.as_bytes()).await.unwrap();
  let mut response = String::new();
  tokio::time::timeout(std::time::Duration::from_secs(2), socket.read_to_string(&mut response))
    .await
    .unwrap()
    .unwrap();
  server.abort();
  assert!(
    response.starts_with("HTTP/1.1 404 "),
    "a closed session must not dispatch a delayed read: {response}"
  );
  assert!(!response.contains("\"items\""));
}
