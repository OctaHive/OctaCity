use crate::{
  ListAgentPoolsQuery, ListAgentsQuery, ListAuditFactsQuery, ListBuildArtifactsQuery, ListBuildCacheSessionsQuery,
  ListInternalTriggersQuery, ListProjectBuildConfigurationsQuery, ListProjectBuildsQuery, ListProjectPipelinesQuery,
  ListProjectRepositoriesQuery, ListProjectTriggerDefinitionsQuery, ListProjectsQuery, ManagementAuthorizationGrant,
  ManagementResource, ManagementResourceKind, ManagementVisibilityView, Query, ReadJobEventsQuery,
  SearchBuildLogsQuery,
};
use octacity_server_domain::{
  AgentId, ArtifactId, AuditFactId, BuildConfigurationId, BuildId, CacheSessionId, JobId, PipelineId, PoolId,
  ProjectId, RepositoryId, TriggerId,
};
use octacity_server_store::{
  AgentListVisibility, AgentPoolListVisibility, ArtifactListVisibility, AuditFactListVisibility,
  BuildConfigurationListVisibility, BuildListVisibility, BuildLogSearchVisibility, CacheSessionListVisibility,
  InternalTriggerListVisibility, JobEventReadVisibility, PipelineListVisibility, ProjectListVisibility,
  RepositoryListVisibility, TriggerDefinitionListVisibility,
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

macro_rules! instance_scoped_query_registry {
  ($apply:ident) => {
    $apply! {
      (ListProjectsQuery, ProjectListVisibility, ProjectId, ManagementResourceKind::Project, 1),
      (ListProjectPipelinesQuery, PipelineListVisibility, PipelineId, ManagementResourceKind::Pipeline, 2),
      (ListProjectRepositoriesQuery, RepositoryListVisibility, RepositoryId, ManagementResourceKind::Repository, 3),
      (
        ListProjectBuildConfigurationsQuery,
        BuildConfigurationListVisibility,
        BuildConfigurationId,
        ManagementResourceKind::BuildConfiguration,
        4
      ),
      (
        ListProjectTriggerDefinitionsQuery,
        TriggerDefinitionListVisibility,
        TriggerId,
        ManagementResourceKind::Trigger,
        5
      ),
      (ListProjectBuildsQuery, BuildListVisibility, BuildId, ManagementResourceKind::Build, 6),
      (ListAgentPoolsQuery, AgentPoolListVisibility, PoolId, ManagementResourceKind::AgentPool, 7),
      (ListAgentsQuery, AgentListVisibility, AgentId, ManagementResourceKind::Agent, 8),
      (
        ListInternalTriggersQuery,
        InternalTriggerListVisibility,
        TriggerId,
        ManagementResourceKind::Trigger,
        9
      ),
      (ReadJobEventsQuery, JobEventReadVisibility, JobId, ManagementResourceKind::Job, 10),
      (
        SearchBuildLogsQuery,
        BuildLogSearchVisibility,
        ProjectId,
        ManagementResourceKind::Project,
        11
      ),
      (
        ListAuditFactsQuery,
        AuditFactListVisibility,
        AuditFactId,
        ManagementResourceKind::AuditFact,
        12
      ),
      (
        ListBuildArtifactsQuery,
        ArtifactListVisibility,
        ArtifactId,
        ManagementResourceKind::Artifact,
        13
      ),
      (
        ListBuildCacheSessionsQuery,
        CacheSessionListVisibility,
        CacheSessionId,
        ManagementResourceKind::CacheSession,
        14
      )
    }
  };
}

macro_rules! implement_instance_scoped_queries {
  ($(($query:ty, $visibility:ty, $identity:ty, $kind:expr, $sample:literal)),+ $(,)?) => {
    $(
      impl sealed::VisibilityInput for $visibility {}
      impl ManagementVisibilityInput for $visibility {}

      impl ManagementVisibilityTarget for $query {
        type Visibility = $visibility;
        const RESTRICTED_RESOURCE_KIND: ManagementResourceKind = $kind;

        fn visibility_from(
          grant: &ManagementAuthorizationGrant,
        ) -> Result<Self::Visibility, ManagementVisibilityError> {
          translate_visibility(
            grant,
            |resource| instance_identity::<$identity>(resource, Self::RESTRICTED_RESOURCE_KIND),
            <$visibility>::all,
            <$visibility>::none,
            <$visibility>::restricted,
          )
        }
      }
    )+
  };
}

/// Declares the explicit backend-neutral visibility input for one paged management query.
pub trait ManagementVisibilityTarget: Query {
  /// Query-specific visibility type that adapters must apply before page derivation.
  type Visibility: ManagementVisibilityInput;

  /// Resource identity kind accepted by a restricted grant for this query.
  const RESTRICTED_RESOURCE_KIND: ManagementResourceKind;

  /// Translates the policy grant without widening unsupported resource scopes.
  fn visibility_from(grant: &ManagementAuthorizationGrant) -> Result<Self::Visibility, ManagementVisibilityError>;
}

instance_scoped_query_registry!(implement_instance_scoped_queries);

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

  macro_rules! registered_visibility_types {
    ($(($query:ty, $visibility:ty, $identity:ty, $kind:expr, $sample:literal)),+ $(,)?) => {
      [$(visibility_type::<$query>()),+]
    };
  }

  #[test]
  fn paged_management_queries_have_distinct_typed_visibility_inputs() {
    let visibility_types = instance_scoped_query_registry!(registered_visibility_types);
    assert_eq!(
      visibility_types.into_iter().collect::<BTreeSet<_>>().len(),
      visibility_types.len()
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
    macro_rules! assert_registered_mappings {
      ($(($query:ty, $visibility:ty, $identity:ty, $kind:expr, $sample:literal)),+ $(,)?) => {
        $(
          let identity = <$identity>::from_uuid(uuid::Uuid::from_u128($sample)).unwrap();
          assert_eq!(<$query>::RESTRICTED_RESOURCE_KIND, $kind);
          let grant = restricted(instance($kind, identity));
          assert!(grant.visibility_for::<$query>().unwrap().allows(&identity));
        )+
      };
    }

    instance_scoped_query_registry!(assert_registered_mappings);
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
