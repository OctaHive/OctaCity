use octacity_server_application::{
  CreateAgentPoolCommand, DeleteAgentPoolCommand, DrainAgentCommand, GetAgentPoolQuery, GetAgentQuery,
  IssueAgentEnrollmentCommand, ListAgentPoolsQuery, ListAgentsQuery, PublishAgentPoolVersionCommand,
  ReassignAgentPoolCommand,
};

use super::{ManagementOperation, ParameterProfile};

pub(super) const OPERATIONS: &[ManagementOperation] = &[
  operation!(
    CreateAgentPoolCommand,
    "POST",
    "/api/v1/agent-pools",
    "createAgentPool",
    "Agent Pools",
    "Create a static Agent Pool",
    Some("CreateAgentPoolRequest"),
    "AgentPoolMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    ListAgentPoolsQuery,
    "GET",
    "/api/v1/agent-pools",
    "listAgentPools",
    "Agent Pools",
    "List current Agent Pool versions",
    None,
    "AgentPoolPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::AgentPoolList),
  operation!(
    PublishAgentPoolVersionCommand,
    "POST",
    "/api/v1/agent-pools/{pool_id}/versions",
    "publishAgentPoolVersion",
    "Agent Pools",
    "Publish an Agent Pool version",
    Some("PublishAgentPoolVersionRequest"),
    "AgentPoolMutationResponse",
    "200",
    true,
    true
  ),
  operation!(
    GetAgentPoolQuery,
    "GET",
    "/api/v1/agent-pools/{pool_id}/versions/{version}",
    "getAgentPoolVersion",
    "Agent Pools",
    "Get an Agent Pool version",
    None,
    "AgentPoolResource",
    "200",
    false,
    false
  ),
  operation!(
    DeleteAgentPoolCommand,
    "DELETE",
    "/api/v1/agent-pools/{pool_id}",
    "deleteAgentPool",
    "Agent Pools",
    "Delete an unreferenced Agent Pool",
    None,
    "DeleteAgentPoolResponse",
    "200",
    true,
    true
  ),
  operation!(
    IssueAgentEnrollmentCommand,
    "POST",
    "/api/v1/agent-enrollments",
    "issueAgentEnrollment",
    "Agents",
    "Issue a single-use Agent enrollment credential",
    Some("IssueAgentEnrollmentRequest"),
    "IssueAgentEnrollmentResponse",
    "201",
    true,
    false
  ),
  operation!(
    ListAgentsQuery,
    "GET",
    "/api/v1/agents",
    "listAgents",
    "Agents",
    "List enrolled Agents",
    None,
    "AgentPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::AgentList),
  operation!(
    GetAgentQuery,
    "GET",
    "/api/v1/agents/{agent_id}",
    "getAgent",
    "Agents",
    "Get an enrolled Agent",
    None,
    "AgentResource",
    "200",
    false,
    false
  ),
  operation!(
    ReassignAgentPoolCommand,
    "POST",
    "/api/v1/agents/{agent_id}/pool",
    "reassignAgentPool",
    "Agents",
    "Move an idle Agent to another Pool",
    Some("ReassignAgentPoolRequest"),
    "AgentMutationResponse",
    "200",
    true,
    true
  ),
  operation!(
    DrainAgentCommand,
    "POST",
    "/api/v1/agents/{agent_id}/drain",
    "drainAgent",
    "Agents",
    "Drain an Agent and its current Lease",
    Some("DrainAgentRequest"),
    "AgentMutationResponse",
    "200",
    true,
    true
  ),
];
