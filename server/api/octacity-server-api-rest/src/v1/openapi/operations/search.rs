use octacity_server_application::{ListOperatorAttentionQuery, SearchResourcesQuery};

use super::{ManagementOperation, ParameterProfile};

pub(super) const OPERATIONS: &[ManagementOperation] = &[
  operation!(
    SearchResourcesQuery,
    "GET",
    "/api/v1/search",
    "searchResources",
    "Discovery",
    "Search Projects, Builds, Agents, and Agent Pools",
    None,
    "ResourceSearchPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::ResourceSearch)
  .with_visibility::<SearchResourcesQuery>(),
  operation!(
    ListOperatorAttentionQuery,
    "GET",
    "/api/v1/operator-attention",
    "listOperatorAttention",
    "Operations",
    "List scoped operator attention",
    None,
    "OperatorAttentionPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::OperatorAttention)
  .with_visibility::<ListOperatorAttentionQuery>(),
];
