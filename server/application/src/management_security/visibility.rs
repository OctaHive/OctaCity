use crate::{
  ListAgentPoolsQuery, ListAgentsQuery, ListAuditFactsQuery, ListBuildArtifactsQuery, ListBuildCacheSessionsQuery,
  ListInternalTriggersQuery, ListProjectsQuery, ManagementAuthorizationGrant, ManagementResource,
  ManagementResourceKind, ManagementVisibilityView, Query, ReadJobEventsQuery, SearchBuildLogsQuery,
};
use octacity_server_domain::{AgentId, ArtifactId, AuditFactId, CacheSessionId, JobId, PoolId, ProjectId, TriggerId};
use octacity_server_store::{
  AgentListVisibility, AgentPoolListVisibility, ArtifactListVisibility, AuditFactListVisibility,
  BuildLogSearchVisibility, CacheSessionListVisibility, InternalTriggerListVisibility, JobEventReadVisibility,
  ProjectListVisibility,
};
use thiserror::Error;

mod sealed {
  pub trait VisibilityInput {}
}

/// Closed marker implemented by backend-neutral, query-specific visibility inputs.
pub trait ManagementVisibilityInput: sealed::VisibilityInput + Clone + Send + Sync + 'static {}

/// Failure to translate a policy-selected resource scope into one query's typed read scope.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ManagementVisibilityError {
  /// A restricted resource had a kind or shape unsupported by the target query.
  #[error("management visibility scope is unsupported for this query")]
  UnsupportedScope,
  /// A restricted resource identity was not a canonical identity for the target query.
  #[error("management visibility identity is invalid for this query")]
  InvalidIdentity,
  /// The translated identity set violated the store visibility bound.
  #[error("management visibility cannot be represented by the read port")]
  InvalidRestrictedScope,
}

macro_rules! visibility_input {
  ($($visibility:ty),+ $(,)?) => {
    $(
      impl sealed::VisibilityInput for $visibility {}
      impl ManagementVisibilityInput for $visibility {}
    )+
  };
}

visibility_input!(
  ProjectListVisibility,
  AgentPoolListVisibility,
  AgentListVisibility,
  InternalTriggerListVisibility,
  JobEventReadVisibility,
  BuildLogSearchVisibility,
  AuditFactListVisibility,
  ArtifactListVisibility,
  CacheSessionListVisibility,
);

/// Declares the explicit backend-neutral visibility input for one paged management query.
pub trait ManagementVisibilityTarget: Query {
  /// Query-specific visibility type that adapters must apply before page derivation.
  type Visibility: ManagementVisibilityInput;

  /// Resource identity kind accepted by a restricted grant for this query.
  const RESTRICTED_RESOURCE_KIND: ManagementResourceKind;

  /// Translates the policy grant without widening unsupported resource scopes.
  fn visibility_from(grant: &ManagementAuthorizationGrant) -> Result<Self::Visibility, ManagementVisibilityError>;
}

macro_rules! instance_scoped_query {
  ($query:ty => $visibility:ty, $identity:ty, $kind:expr) => {
    impl ManagementVisibilityTarget for $query {
      type Visibility = $visibility;
      const RESTRICTED_RESOURCE_KIND: ManagementResourceKind = $kind;

      fn visibility_from(grant: &ManagementAuthorizationGrant) -> Result<Self::Visibility, ManagementVisibilityError> {
        translate_visibility(
          grant,
          |resource| instance_identity::<$identity>(resource, Self::RESTRICTED_RESOURCE_KIND),
          <$visibility>::all,
          <$visibility>::none,
          <$visibility>::restricted,
        )
      }
    }
  };
}

instance_scoped_query!(
  ListProjectsQuery => ProjectListVisibility,
  ProjectId,
  ManagementResourceKind::Project
);
instance_scoped_query!(
  ListAgentPoolsQuery => AgentPoolListVisibility,
  PoolId,
  ManagementResourceKind::AgentPool
);
instance_scoped_query!(
  ListAgentsQuery => AgentListVisibility,
  AgentId,
  ManagementResourceKind::Agent
);
instance_scoped_query!(
  ListInternalTriggersQuery => InternalTriggerListVisibility,
  TriggerId,
  ManagementResourceKind::Trigger
);
instance_scoped_query!(
  ReadJobEventsQuery => JobEventReadVisibility,
  JobId,
  ManagementResourceKind::Job
);
instance_scoped_query!(
  SearchBuildLogsQuery => BuildLogSearchVisibility,
  ProjectId,
  ManagementResourceKind::Project
);
instance_scoped_query!(
  ListAuditFactsQuery => AuditFactListVisibility,
  AuditFactId,
  ManagementResourceKind::AuditFact
);
instance_scoped_query!(
  ListBuildArtifactsQuery => ArtifactListVisibility,
  ArtifactId,
  ManagementResourceKind::Artifact
);
instance_scoped_query!(
  ListBuildCacheSessionsQuery => CacheSessionListVisibility,
  CacheSessionId,
  ManagementResourceKind::CacheSession
);

impl ManagementAuthorizationGrant {
  /// Produces the query-specific read scope selected by this authorization grant.
  pub fn visibility_for<Q>(&self) -> Result<Q::Visibility, ManagementVisibilityError>
  where
    Q: ManagementVisibilityTarget,
  {
    Q::visibility_from(self)
  }
}

fn translate_visibility<I, V, E>(
  grant: &ManagementAuthorizationGrant,
  identity: impl Fn(&ManagementResource) -> Result<I, ManagementVisibilityError>,
  all: impl FnOnce() -> V,
  none: impl FnOnce() -> V,
  restricted: impl FnOnce(Vec<I>) -> Result<V, E>,
) -> Result<V, ManagementVisibilityError> {
  match grant.visibility().view() {
    ManagementVisibilityView::All => Ok(all()),
    ManagementVisibilityView::None => Ok(none()),
    ManagementVisibilityView::Restricted(resources) => restricted(
      resources
        .iter()
        .map(identity)
        .collect::<Result<Vec<_>, ManagementVisibilityError>>()?,
    )
    .map_err(|_| ManagementVisibilityError::InvalidRestrictedScope),
  }
}

fn instance_identity<I>(
  resource: &ManagementResource,
  expected_kind: ManagementResourceKind,
) -> Result<I, ManagementVisibilityError>
where
  I: std::str::FromStr,
{
  if resource.kind() != expected_kind || resource.owner().is_some() {
    return Err(ManagementVisibilityError::UnsupportedScope);
  }
  resource
    .identity()
    .ok_or(ManagementVisibilityError::UnsupportedScope)?
    .as_str()
    .parse()
    .map_err(|_| ManagementVisibilityError::InvalidIdentity)
}

#[cfg(test)]
mod tests {
  use std::any::TypeId;
  use std::collections::BTreeSet;

  use octacity_server_store::ReadVisibilityKind;

  use super::*;

  fn visibility_type<Q: ManagementVisibilityTarget + 'static>() -> TypeId {
    TypeId::of::<Q::Visibility>()
  }

  #[test]
  fn paged_management_queries_have_distinct_typed_visibility_inputs() {
    let visibility_types = [
      visibility_type::<ListProjectsQuery>(),
      visibility_type::<ListAgentPoolsQuery>(),
      visibility_type::<ListAgentsQuery>(),
      visibility_type::<ListInternalTriggersQuery>(),
      visibility_type::<ReadJobEventsQuery>(),
      visibility_type::<SearchBuildLogsQuery>(),
      visibility_type::<ListAuditFactsQuery>(),
      visibility_type::<ListBuildArtifactsQuery>(),
      visibility_type::<ListBuildCacheSessionsQuery>(),
    ];
    assert_eq!(
      visibility_types.into_iter().collect::<BTreeSet<_>>().len(),
      visibility_types.len()
    );
  }

  #[test]
  fn scoped_queries_declare_the_identity_kind_accepted_from_restricted_grants() {
    assert_eq!(
      ListProjectsQuery::RESTRICTED_RESOURCE_KIND,
      ManagementResourceKind::Project
    );
    assert_eq!(
      ListAgentPoolsQuery::RESTRICTED_RESOURCE_KIND,
      ManagementResourceKind::AgentPool
    );
    assert_eq!(ListAgentsQuery::RESTRICTED_RESOURCE_KIND, ManagementResourceKind::Agent);
    assert_eq!(
      ListInternalTriggersQuery::RESTRICTED_RESOURCE_KIND,
      ManagementResourceKind::Trigger
    );
    assert_eq!(
      ReadJobEventsQuery::RESTRICTED_RESOURCE_KIND,
      ManagementResourceKind::Job
    );
    assert_eq!(
      SearchBuildLogsQuery::RESTRICTED_RESOURCE_KIND,
      ManagementResourceKind::Project
    );
    assert_eq!(
      ListAuditFactsQuery::RESTRICTED_RESOURCE_KIND,
      ManagementResourceKind::AuditFact
    );
    assert_eq!(
      ListBuildArtifactsQuery::RESTRICTED_RESOURCE_KIND,
      ManagementResourceKind::Artifact
    );
    assert_eq!(
      ListBuildCacheSessionsQuery::RESTRICTED_RESOURCE_KIND,
      ManagementResourceKind::CacheSession
    );
  }

  #[test]
  fn unrestricted_and_empty_grants_translate_without_changing_semantics() {
    let all = ManagementAuthorizationGrant::new(crate::ManagementVisibility::all())
      .visibility_for::<ListProjectsQuery>()
      .unwrap();
    let none = ManagementAuthorizationGrant::new(crate::ManagementVisibility::none())
      .visibility_for::<ListProjectsQuery>()
      .unwrap();

    assert_eq!(all.kind(), ReadVisibilityKind::All);
    assert_eq!(none.kind(), ReadVisibilityKind::None);
  }

  #[test]
  fn every_scoped_query_translates_its_supported_restricted_resource_shape() {
    let project_id = ProjectId::from_uuid(uuid::Uuid::from_u128(1)).unwrap();
    let pool_id = PoolId::from_uuid(uuid::Uuid::from_u128(2)).unwrap();
    let agent_id = AgentId::from_uuid(uuid::Uuid::from_u128(3)).unwrap();
    let trigger_id = TriggerId::from_uuid(uuid::Uuid::from_u128(4)).unwrap();
    let job_id = JobId::from_uuid(uuid::Uuid::from_u128(5)).unwrap();
    let log_project_id = ProjectId::from_uuid(uuid::Uuid::from_u128(6)).unwrap();
    let audit_fact_id = AuditFactId::from_uuid(uuid::Uuid::from_u128(7)).unwrap();
    let artifact_id = ArtifactId::from_uuid(uuid::Uuid::from_u128(8)).unwrap();
    let cache_session_id = CacheSessionId::from_uuid(uuid::Uuid::from_u128(9)).unwrap();

    let grant = restricted(instance(ManagementResourceKind::Project, project_id));
    assert!(grant.visibility_for::<ListProjectsQuery>().unwrap().allows(&project_id));

    let grant = restricted(instance(ManagementResourceKind::AgentPool, pool_id));
    assert!(grant.visibility_for::<ListAgentPoolsQuery>().unwrap().allows(&pool_id));

    let grant = restricted(instance(ManagementResourceKind::Agent, agent_id));
    assert!(grant.visibility_for::<ListAgentsQuery>().unwrap().allows(&agent_id));

    let grant = restricted(instance(ManagementResourceKind::Trigger, trigger_id));
    assert!(
      grant
        .visibility_for::<ListInternalTriggersQuery>()
        .unwrap()
        .allows(&trigger_id)
    );

    let grant = restricted(instance(ManagementResourceKind::Job, job_id));
    assert!(grant.visibility_for::<ReadJobEventsQuery>().unwrap().allows(&job_id));

    let grant = restricted(instance(ManagementResourceKind::Project, log_project_id));
    assert!(
      grant
        .visibility_for::<SearchBuildLogsQuery>()
        .unwrap()
        .allows(&log_project_id)
    );

    let grant = restricted(instance(ManagementResourceKind::AuditFact, audit_fact_id));
    assert!(
      grant
        .visibility_for::<ListAuditFactsQuery>()
        .unwrap()
        .allows(&audit_fact_id)
    );

    let grant = restricted(instance(ManagementResourceKind::Artifact, artifact_id));
    assert!(
      grant
        .visibility_for::<ListBuildArtifactsQuery>()
        .unwrap()
        .allows(&artifact_id)
    );

    let grant = restricted(instance(ManagementResourceKind::CacheSession, cache_session_id));
    assert!(
      grant
        .visibility_for::<ListBuildCacheSessionsQuery>()
        .unwrap()
        .allows(&cache_session_id)
    );
  }

  #[test]
  fn unsupported_or_malformed_restricted_scopes_are_rejected_instead_of_widened() {
    let wrong_shape = restricted(instance(ManagementResourceKind::Agent, AgentId::generate()));
    assert_eq!(
      wrong_shape.visibility_for::<SearchBuildLogsQuery>(),
      Err(ManagementVisibilityError::UnsupportedScope)
    );

    let malformed = restricted(
      ManagementResource::instance(
        ManagementResourceKind::Project,
        crate::ManagementResourceIdentity::new("not-a-project-id").unwrap(),
      )
      .unwrap(),
    );
    assert_eq!(
      malformed.visibility_for::<ListProjectsQuery>(),
      Err(ManagementVisibilityError::InvalidIdentity)
    );
  }

  fn restricted(resource: ManagementResource) -> ManagementAuthorizationGrant {
    ManagementAuthorizationGrant::new(crate::ManagementVisibility::restricted([resource]).unwrap())
  }

  fn instance(kind: ManagementResourceKind, identity: impl ToString) -> ManagementResource {
    ManagementResource::instance(
      kind,
      crate::ManagementResourceIdentity::new(identity.to_string()).unwrap(),
    )
    .unwrap()
  }
}
