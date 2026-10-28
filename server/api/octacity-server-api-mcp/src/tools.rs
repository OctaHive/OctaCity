//! Closed read-only tool catalogue, argument contracts and safe result projections.
use super::{IssueMcpApplication, error, tool_failure};
use axum::{
  Json,
  http::StatusCode,
  response::{IntoResponse, Response},
};
use octacity_server_application::{
  ApplicationError, ApplicationFailure, FactoryExecutionClock, GetIssueQuery, IssueReadPrincipal, IssueRevision,
  IssueVersion, ListProjectIssuesQuery, MAX_ISSUE_TITLE_CURSOR_BYTES, MAX_ISSUE_TITLE_QUERY_BYTES,
  ManagementHandlerError, ManagementQueryUseCase, ProviderCredentialValue, SystemFactoryExecutionClock,
};
use serde_json::{Value, json};
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_PAGE_SIZE: u16 = 50;

pub(super) async fn call<H>(
  application: &IssueMcpApplication<H>,
  credential: &ProviderCredentialValue,
  principal: &IssueReadPrincipal,
  id: Value,
  params: &Value,
) -> Response
where
  H: ManagementQueryUseCase<ListProjectIssuesQuery, Error = ApplicationError>
    + ManagementQueryUseCase<GetIssueQuery, Error = ApplicationError>
    + 'static,
{
  match params["name"].as_str() {
    Some("read_issue") => {
      let Ok(arguments) = serde_json::from_value::<ReadArguments>(params["arguments"].clone()) else {
        return error(id, -32602, "Invalid tool arguments");
      };
      let Ok(issue_id) = arguments.issue_id.parse() else {
        return error(id, -32602, "Invalid tool arguments");
      };
      let outcome = principal
        .read_issue(application.queries.clone(), issue_id, arguments.version)
        .await;
      if let Err(status) = application.recheck(credential, principal).await {
        return status.into_response();
      }
      let issue = match outcome {
        Ok(issue) => issue,
        Err(ManagementHandlerError::Forbidden(_)) => return tool_failure(id, "not_available"),
        Err(ManagementHandlerError::Application(cause)) if cause.classification() == ApplicationFailure::NotFound => {
          return tool_failure(id, "not_available");
        }
        Err(_) => return tool_failure(id, "unavailable"),
      };
      tool_result(
        id,
        json!({"scope":{"project_id":principal.project_id(),"mode":"issue_revision"},"issue":detail(&issue)}),
      )
    }
    Some("search_issues") => {
      let Ok(arguments) = serde_json::from_value::<SearchArguments>(params["arguments"].clone()) else {
        return error(id, -32602, "Invalid tool arguments");
      };
      let limit = arguments.limit.unwrap_or(25);
      if limit == 0 || limit > MAX_PAGE_SIZE {
        return error(id, -32602, "Invalid tool arguments");
      }
      let outcome = principal
        .search_titles(
          application.queries.clone(),
          &arguments.query,
          arguments.cursor.as_deref(),
          limit,
        )
        .await;
      if let Err(status) = application.recheck(credential, principal).await {
        return status.into_response();
      }
      let page = match outcome {
        Ok(page) => page,
        Err(ManagementHandlerError::Application(error_value))
          if error_value.classification() == ApplicationFailure::Invalid =>
        {
          return error(id, -32602, "Invalid tool arguments");
        }
        Err(_) => return tool_failure(id, "unavailable"),
      };
      let has_more = page.next_cursor.is_some();
      let content = json!({"items":page.items, "next_cursor":page.next_cursor,
        "scope":{"project_id":principal.project_id(),"mode":"issue_titles"},
        "coverage":{"scanned_entries":page.scanned_entries,"has_more":has_more,"consistent_snapshot":false}});
      tool_result(id, content)
    }
    _ => error(id, -32602, "Tool unavailable"),
  }
}

// The public MCP shape is selected here; additions to the store revision do not
// automatically become protocol fields. Keep the retained revision's wire shape.
fn detail(issue: &IssueRevision) -> Value {
  let mut value = json!({
    "id":issue.id.to_string(),"project_id":issue.project_id.to_string(),"number":issue.number,
    "version":issue.version.get(),"applicability_version":issue.applicability_version.get(),
    "content_available":issue.content_available,
    "content":{"title":issue.content.title,"description":issue.content.description,
      "acceptance":issue.content.acceptance,"priority":issue.content.priority},
    "workflow_id":issue.workflow_id.to_string(),"workflow_version":issue.workflow_version.get(),
    "status_id":issue.status_id.to_string(),"created_at":issue.created_at.unix_millis(),"updated_at":issue.updated_at.unix_millis(),
  });
  if !issue.planning.is_empty() {
    value["planning"] = json!({
      "definition_version":issue.planning.definition_version,"assignee":issue.planning.assignee,
      "labels":issue.planning.labels,"due_date":issue.planning.due_date,
      "values":issue.planning.values,"parent_id":issue.planning.parent_id,
    });
  }
  value
}

pub(super) fn catalog() -> Value {
  json!([
    {
      "name":"read_issue", "description":"Read one visible Issue at its exact retained version, or its current revision.",
      "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false},
      "inputSchema":{"type":"object","additionalProperties":false,"required":["issue_id"],"properties":{
        "issue_id":{"type":"string","format":"uuid","maxLength":36},
        "version":{"type":"integer","minimum":1}
      }}
    },
    {
      "name":"search_issues", "description":"Search bounded pages of visible Issue titles in the connection's Project. Page coverage is not a global proof of absence.",
      "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false},
      "inputSchema":{"type":"object","additionalProperties":false,"required":["query"],"properties":{
        "query":{"type":"string","maxLength":MAX_ISSUE_TITLE_QUERY_BYTES},
        "limit":{"type":"integer","minimum":1,"maximum":MAX_PAGE_SIZE},
        "cursor":{"type":"string","maxLength":MAX_ISSUE_TITLE_CURSOR_BYTES}
      }}
    }
  ])
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArguments {
  query: String,
  limit: Option<u16>,
  cursor: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArguments {
  issue_id: String,
  version: Option<IssueVersion>,
}

fn tool_result(id: Value, mut content: Value) -> Response {
  let Ok(observed) = SystemFactoryExecutionClock.now() else {
    return StatusCode::SERVICE_UNAVAILABLE.into_response();
  };
  content["content_digest"] = json!(octacity_server_application::FactoryDigest::content_sha256(
    &serde_json::to_vec(&content).unwrap()
  ));
  content["observed_at_unix_ms"] = json!(observed);
  let response = json!({"jsonrpc":"2.0","id":id,"result":{
    "content":[{"type":"text","text":content.to_string()}],"structuredContent":content,"isError":false
  }});
  if serde_json::to_vec(&response).expect("JSON response").len() > MAX_RESPONSE_BYTES {
    return tool_failure(id, "too_large");
  }
  Json(response).into_response()
}
