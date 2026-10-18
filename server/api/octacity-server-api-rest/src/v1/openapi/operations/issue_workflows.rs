use super::{ManagementOperation, ParameterProfile};
use octacity_server_application::PublishIssueWorkflowCommand;
use octacity_server_application::{GetDefaultIssueWorkflowQuery, ValidateIssueWorkflowQuery};
pub(super) const OPERATIONS: &[ManagementOperation] = &[
  operation!(
    octacity_server_application::ListProjectIssueWorkflowsQuery,
    "GET",
    "/api/v1/projects/{project_id}/issue-workflows",
    "listProjectIssueWorkflows",
    "Issue Workflows",
    "Discover bounded visible current business policies",
    None,
    "IssueWorkflowSummaryPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::CurrentDefinitionList)
  .with_visibility::<octacity_server_application::ListProjectIssueWorkflowsQuery>(),
  operation!(
    octacity_server_application::SelectDefaultIssueWorkflowCommand,
    "POST",
    "/api/v1/projects/{project_id}/issue-workflow-default",
    "selectDefaultIssueWorkflow",
    "Issue Workflows",
    "Select exact business policy for new tasks",
    Some("SelectDefaultIssueWorkflowRequest"),
    "IssueWorkflowDefaultMutationResponse",
    "200",
    true,
    true
  ),
  operation!(
    octacity_server_application::GetIssueWorkflowQuery,
    "GET",
    "/api/v1/projects/{project_id}/issue-workflows/{workflow_id}",
    "getCurrentIssueWorkflow",
    "Issue Workflows",
    "Read current immutable workflow policy",
    None,
    "IssueWorkflowResource",
    "200",
    false,
    false
  ),
  operation!(
    octacity_server_application::GetIssueWorkflowQuery,
    "GET",
    "/api/v1/projects/{project_id}/issue-workflows/{workflow_id}/versions/{version}",
    "getIssueWorkflowVersion",
    "Issue Workflows",
    "Read a retained exact workflow version",
    None,
    "IssueWorkflowResource",
    "200",
    false,
    false
  ),
  operation!(
    PublishIssueWorkflowCommand,
    "POST",
    "/api/v1/projects/{project_id}/issue-workflows",
    "createIssueWorkflow",
    "Issue Workflows",
    "Create an immutable Project business policy",
    Some("CreateIssueWorkflowRequest"),
    "IssueWorkflowMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    PublishIssueWorkflowCommand,
    "POST",
    "/api/v1/projects/{project_id}/issue-workflows/{workflow_id}/versions",
    "publishIssueWorkflowVersion",
    "Issue Workflows",
    "Publish an immutable business policy replacement",
    Some("IssueWorkflowDefinitionRequest"),
    "IssueWorkflowMutationResponse",
    "200",
    true,
    true
  ),
  operation!(
    ValidateIssueWorkflowQuery,
    "POST",
    "/api/v1/projects/{project_id}/issue-workflows/validate",
    "validateIssueWorkflow",
    "Issue Workflows",
    "Validate bounded business policy without publication",
    Some("IssueWorkflowDefinitionRequest"),
    "IssueWorkflowDefinition",
    "200",
    false,
    false
  ),
  operation!(
    GetDefaultIssueWorkflowQuery,
    "GET",
    "/api/v1/projects/{project_id}/issue-workflow-default",
    "getDefaultIssueWorkflow",
    "Issue Workflows",
    "Read the exact default for new manual tasks",
    None,
    "IssueWorkflowDefault",
    "200",
    false,
    false
  ),
];
