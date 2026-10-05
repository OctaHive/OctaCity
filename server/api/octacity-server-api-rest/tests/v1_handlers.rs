use std::{
  collections::BTreeSet,
  sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
  },
};

use async_trait::async_trait;
use axum::{
  body::{Body, to_bytes},
  http::{Method, Request, StatusCode},
};
use octacity_server_api_rest::{
  management_router_with_application,
  v1::{ErrorCode, MANAGEMENT_OPERATIONS, management_authorization_operations, openapi_document},
};
use octacity_server_application::{
  AcceptManualTriggerCommand, AgentPageProjection, AgentPoolProjection, AgentProjection, ApplicationError,
  AttemptState, AuditActorKind, AuditActorProjection, AuditFactPageProjection, AuditFactProjection, AuditOutcome,
  AuthorizeArtifactDownloadQuery, BuildConfigurationPageProjection, BuildConfigurationSummaryProjection,
  BuildLogSearchCursorProjection, BuildLogSearchError, BuildLogSearchFreshnessProjection, BuildLogSearchHitProjection,
  BuildLogSearchPageProjection, BuildLogStream, BuildPageCursor, BuildPageProjection, BuildState,
  BuildSummaryProjection, CancelBuildCommand, Command, CreateAgentPoolCommand, CreateBuildConfigurationCommand,
  CreateInternalTriggerCommand, CreateManagedWebhookCommand, CreatePipelineCommand, CreateProjectCommand,
  CreateRepositoryCommand, CreateScheduleCommand, CreateTriggerDefinitionCommand, CreateUnmanagedWebhookCommand,
  DeleteAgentPoolCommand, DeleteManagedWebhookRegistrationCommand, DeleteProjectCommand, DrainAgentCommand,
  GetAgentPoolQuery, GetAgentQuery, GetArtifactQuery, GetAttemptQuery, GetBuildConfigurationQuery, GetBuildQuery,
  GetBuildResultRetentionQuery, GetCacheSessionQuery, GetInternalTriggerQuery, GetJobQuery,
  GetManualTriggerDefinitionQuery, GetOperationalMetadataQuery, GetPipelineQuery, GetProjectQuery, GetRepositoryQuery,
  GetScheduleQuery, IssueAgentEnrollmentCommand, ListAgentPoolsQuery, ListAgentsQuery, ListAuditFactsQuery,
  ListBuildArtifactsQuery, ListBuildCacheSessionsQuery, ListInternalTriggersQuery, ListOperatorAttentionQuery,
  ListProjectBuildConfigurationsQuery, ListProjectBuildsQuery, ListProjectPipelinesQuery, ListProjectRepositoriesQuery,
  ListProjectTriggerDefinitionsQuery, ListProjectsQuery, LogSearchError, ManagementAction,
  ManagementAuthorizationDenial, ManagementAuthorizationGrant, ManagementAuthorizationPolicy,
  ManagementAuthorizationTarget, ManagementCommandUseCase, ManagementOperationalMetadataProjection,
  ManagementQueryUseCase, ManagementRequestContext, ManagementResource, ManualTriggerDefinitionProjection,
  ManualTriggerError, MoveProjectCommand, ObserveManagedWebhookRegistrationCommand, OperatorAttentionCategory,
  OperatorAttentionId, OperatorAttentionItemProjection, OperatorAttentionPageProjection, OperatorAttentionSeverity,
  PipelinePageProjection, PipelineSummaryProjection, PlaceBuildResultHoldCommand, PublishAgentPoolVersionCommand,
  PublishBuildConfigurationVersionCommand, PublishInternalTriggerVersionCommand, PublishPipelineVersionCommand,
  PublishProjectPolicyCommand, PublishRepositoryVersionCommand, Query, ReadJobEventsQuery, ReassignAgentPoolCommand,
  ReleaseBuildResultHoldCommand, RenameProjectCommand, RepositoryPageProjection, RepositorySummaryProjection,
  ResourceSearchCursor, ResourceSearchIdentityProjection, ResourceSearchPageProjection,
  ResourceSearchSummaryProjection, RetryBuildCommand, RotateManagedWebhookRegistrationCommand, SearchBuildLogsQuery,
  SearchResourcesQuery, Timestamp, TriggerCauseProjection, TriggerDefinitionKindProjection,
  TriggerDefinitionPageProjection, TriggerDefinitionSummaryProjection,
};
use tower::ServiceExt as _;

#[path = "v1_handlers/agent_capacity.rs"]
mod agent_capacity;
#[path = "v1_handlers/audit.rs"]
mod audit;
#[path = "v1_handlers/build_discovery.rs"]
mod build_discovery;
#[path = "v1_handlers/definition_discovery.rs"]
mod definition_discovery;
#[path = "v1_handlers/log_search.rs"]
mod log_search;
#[path = "v1_handlers/openapi_drift.rs"]
mod openapi_drift;
#[path = "v1_handlers/operator_attention.rs"]
mod operator_attention;
#[path = "v1_handlers/resource_search.rs"]
mod resource_search;
#[path = "v1_handlers/retention.rs"]
mod retention;
mod support;
#[path = "v1_handlers/webhook.rs"]
mod webhook;

use support::{
  JobEventApplication, agent_pool_create_body, agent_pool_publish_body, assert_component_exists,
  assert_json_matches_component, assert_required_header, concrete_path, configuration_version_body,
  documented_http_requests, empty_request, json_request, recording_management_application,
  recording_management_application_with_policy, recording_management_application_with_retention, repository_body,
  repository_version_body, request_examples,
};

const DOCUMENTED_SECTION_FOUR_WORKFLOW: &str = include_str!("../../../../docs/reference/management-rest-v1.md");

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ProtectedSideEffectCounts {
  handler: usize,
  transaction: usize,
  audit: usize,
  outbox: usize,
  idempotency: usize,
  transfer_capability: usize,
}

#[derive(Default)]
struct RecordingApplication {
  calls: Mutex<Vec<&'static str>>,
  protected_side_effects: Mutex<ProtectedSideEffectCounts>,
  log_search_queries: Mutex<Vec<SearchBuildLogsQuery>>,
  build_list_queries: Mutex<Vec<ListProjectBuildsQuery>>,
  resource_search_queries: Mutex<Vec<SearchResourcesQuery>>,
  operator_attention_queries: Mutex<Vec<ListOperatorAttentionQuery>>,
  agent_queries: Mutex<Vec<GetAgentQuery>>,
  agent_list_queries: Mutex<Vec<ListAgentsQuery>>,
  agent_pool_queries: Mutex<Vec<GetAgentPoolQuery>>,
  build_summary_states: Option<(BuildState, AttemptState)>,
  successful_workflow: bool,
  capability_unavailable_for_managed: bool,
}

impl RecordingApplication {
  fn successful_workflow() -> Self {
    Self {
      calls: Mutex::default(),
      protected_side_effects: Mutex::default(),
      log_search_queries: Mutex::default(),
      build_list_queries: Mutex::default(),
      resource_search_queries: Mutex::default(),
      operator_attention_queries: Mutex::default(),
      agent_queries: Mutex::default(),
      agent_list_queries: Mutex::default(),
      agent_pool_queries: Mutex::default(),
      build_summary_states: Some((BuildState::Succeeded, AttemptState::Succeeded)),
      successful_workflow: true,
      capability_unavailable_for_managed: false,
    }
  }

  fn queued_build_workflow() -> Self {
    Self {
      build_summary_states: Some((BuildState::Queued, AttemptState::Created)),
      ..Self::successful_workflow()
    }
  }

  fn record(&self, operation: &'static str) {
    self.calls.lock().unwrap().push(operation);
    // Arm every protected downstream boundary so one accidental dispatch makes
    // the deny contract fail for handlers and all forbidden side effects.
    let mut effects = self.protected_side_effects.lock().unwrap();
    effects.handler += 1;
    effects.transaction += 1;
    effects.audit += 1;
    effects.outbox += 1;
    effects.idempotency += 1;
    effects.transfer_capability += 1;
  }
}

#[derive(Default)]
struct DenyAllManagementPolicy {
  decisions: AtomicUsize,
}

#[async_trait]
impl ManagementAuthorizationPolicy for DenyAllManagementPolicy {
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    _action: ManagementAction,
    _resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    self.decisions.fetch_add(1, Ordering::SeqCst);
    Err(ManagementAuthorizationDenial::forbidden())
  }
}

macro_rules! unavailable_command {
  ($command:ty, $operation:literal) => {
    #[async_trait]
    impl ManagementCommandUseCase<$command> for RecordingApplication {
      type Error = ApplicationError;

      async fn execute_management_command(
        &self,
        _context: &ManagementRequestContext,
        _grant: &ManagementAuthorizationGrant,
        _command: $command,
      ) -> Result<<$command as Command>::Outcome, Self::Error> {
        self.record($operation);
        Err(ApplicationError::unavailable())
      }
    }
  };
}

macro_rules! unavailable_query {
  ($query:ty, $operation:literal) => {
    #[async_trait]
    impl ManagementQueryUseCase<$query> for RecordingApplication {
      type Error = ApplicationError;

      async fn execute_management_query(
        &self,
        _context: &ManagementRequestContext,
        _grant: &ManagementAuthorizationGrant,
        _query: $query,
      ) -> Result<<$query as Query>::Outcome, Self::Error> {
        self.record($operation);
        Err(ApplicationError::unavailable())
      }
    }
  };
}

#[async_trait]
impl ManagementQueryUseCase<GetOperationalMetadataQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    _query: GetOperationalMetadataQuery,
  ) -> Result<ManagementOperationalMetadataProjection, Self::Error> {
    self.record("get_operational_metadata");
    Ok(ManagementOperationalMetadataProjection {
      management_externally_reachable: false,
      external_access_acknowledged: false,
      agent_ingress_enabled: true,
      webhook_ingress_enabled: false,
    })
  }
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
unavailable_command!(CreateScheduleCommand, "create_schedule");
unavailable_command!(CreateUnmanagedWebhookCommand, "create_unmanaged_webhook");
unavailable_query!(GetScheduleQuery, "get_schedule");
unavailable_command!(CreateInternalTriggerCommand, "create_internal_trigger");
unavailable_command!(PublishInternalTriggerVersionCommand, "publish_internal_trigger");
unavailable_query!(GetInternalTriggerQuery, "get_internal_trigger");
unavailable_query!(ListInternalTriggersQuery, "list_internal_triggers");
unavailable_query!(GetBuildConfigurationQuery, "get_configuration");
unavailable_query!(ReadJobEventsQuery, "read_job_events");
unavailable_command!(CreateAgentPoolCommand, "create_agent_pool");
unavailable_command!(PublishAgentPoolVersionCommand, "publish_agent_pool");
unavailable_command!(DeleteAgentPoolCommand, "delete_agent_pool");
unavailable_query!(ListAgentPoolsQuery, "list_agent_pools");
unavailable_command!(ReassignAgentPoolCommand, "reassign_agent_pool");
unavailable_command!(DrainAgentCommand, "drain_agent");
unavailable_command!(IssueAgentEnrollmentCommand, "issue_agent_enrollment");
unavailable_query!(GetBuildQuery, "get_build");
unavailable_query!(GetAttemptQuery, "get_attempt");
unavailable_query!(GetJobQuery, "get_job");
unavailable_command!(CancelBuildCommand, "cancel_build");
unavailable_command!(RetryBuildCommand, "retry_build");
unavailable_query!(GetArtifactQuery, "get_artifact");
unavailable_query!(ListBuildArtifactsQuery, "list_build_artifacts");
unavailable_query!(AuthorizeArtifactDownloadQuery, "authorize_artifact_download");
unavailable_query!(GetCacheSessionQuery, "get_cache_session");
unavailable_query!(ListBuildCacheSessionsQuery, "list_build_cache_sessions");
unavailable_query!(GetBuildResultRetentionQuery, "get_build_result_retention");
unavailable_command!(PlaceBuildResultHoldCommand, "place_build_result_hold");
unavailable_command!(ReleaseBuildResultHoldCommand, "release_build_result_hold");

#[async_trait]
impl ManagementQueryUseCase<GetAgentQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: GetAgentQuery,
  ) -> Result<AgentProjection, Self::Error> {
    self.record("get_agent");
    self.agent_queries.lock().unwrap().push(query);
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(agent_projection(query.agent_id))
  }
}

#[async_trait]
impl ManagementQueryUseCase<ListAgentsQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: ListAgentsQuery,
  ) -> Result<AgentPageProjection, Self::Error> {
    self.record("list_agents");
    self.agent_list_queries.lock().unwrap().push(query);
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(AgentPageProjection {
      agents: vec![agent_projection(
        query
          .after
          .unwrap_or_else(|| "77777777-7777-4777-8777-777777777777".parse().unwrap()),
      )],
      next_cursor: None,
    })
  }
}

#[async_trait]
impl ManagementQueryUseCase<GetAgentPoolQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: GetAgentPoolQuery,
  ) -> Result<AgentPoolProjection, Self::Error> {
    self.record("get_agent_pool");
    self.agent_pool_queries.lock().unwrap().push(query);
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(
      serde_json::from_value(serde_json::json!({
        "id": query.pool_id,
        "name": "linux-native",
      "version": query.version.map_or(2, |version| version.get()),
        "enabled": true,
        "drain_state": "accepting",
        "admission_policy": {"mode": "any"},
        "concurrency_limit": 2,
        "fairness_policy": "priority_fifo",
        "static_capacity_limit": 4,
        "published_at": 1_700_000_000_000_i64
      }))
      .expect("static Agent Pool projection must be valid"),
    )
  }
}

fn agent_projection(agent_id: impl serde::Serialize) -> AgentProjection {
  serde_json::from_value(serde_json::json!({
    "id": agent_id,
    "name": "linux-builder-1",
    "version": 3,
    "pool_id": "66666666-6666-4666-8666-666666666666",
    "pool_version": 2,
    "inventory": {
      "agent_version": "0.1.0",
      "coordinator_protocols": [1],
      "labels": {"region": "local"},
      "host_platform": {"os": "linux", "architecture": "amd64"},
      "runtimes": [],
      "octa": {
        "version": "0.1.0",
        "runner_sha256": "1111111111111111111111111111111111111111111111111111111111111111",
        "build_commit": null,
        "runner_protocols": [1],
        "event_schemas": [1],
        "plugin_protocols": [1],
        "octafile_versions": [1],
        "features": [],
        "plugins": []
      },
      "source_plugins": [],
      "cache": null
    },
    "capacity": {
      "logical_cpu_count": 8,
      "total_memory_bytes": 17179869184_u64,
      "work_disk_total_bytes": 107374182400_u64,
      "state_disk_total_bytes": 53687091200_u64,
      "virtualization_available": true
    },
    "status": "online",
    "last_seen_at": 1_789_750_800_000_i64,
    "current_execution": {
      "lease_id": "88888888-8888-4888-8888-888888888888",
      "build_id": "99999999-9999-4999-8999-999999999999",
      "attempt_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
      "job_id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
      "lease_state": "active"
    }
  }))
  .expect("static Agent projection must be valid")
}

#[async_trait]
impl ManagementQueryUseCase<GetManualTriggerDefinitionQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: GetManualTriggerDefinitionQuery,
  ) -> Result<ManualTriggerDefinitionProjection, Self::Error> {
    self.record("get_manual_trigger_definition");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(ManualTriggerDefinitionProjection {
      trigger_id: query.trigger_id,
      version: query.version,
      configuration_id: "44444444-4444-4444-8444-444444444444".parse().unwrap(),
      configuration_version: serde_json::from_value(serde_json::json!(1)).unwrap(),
      enabled: true,
      definition: serde_json::json!({"reason": "operator"}),
      created_at: Timestamp::from_unix_millis(1_700_000_000_400).unwrap(),
    })
  }
}

#[async_trait]
impl ManagementQueryUseCase<ListProjectPipelinesQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: ListProjectPipelinesQuery,
  ) -> Result<PipelinePageProjection, Self::Error> {
    self.record("list_project_pipelines");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    let id = serde_json::from_value(serde_json::json!("22222222-2222-4222-8222-222222222222")).unwrap();
    let first_page = query.page().after().is_none();
    Ok(PipelinePageProjection {
      items: first_page
        .then(|| PipelineSummaryProjection {
          id,
          project_id: query.page().project_id(),
          name: serde_json::from_value(serde_json::json!("main")).unwrap(),
          version: serde_json::from_value(serde_json::json!(1)).unwrap(),
          published_at: Timestamp::from_unix_millis(1_700_000_000_000).unwrap(),
        })
        .into_iter()
        .collect(),
      next_cursor: first_page.then_some(id),
    })
  }
}

#[async_trait]
impl ManagementQueryUseCase<ListProjectRepositoriesQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: ListProjectRepositoriesQuery,
  ) -> Result<RepositoryPageProjection, Self::Error> {
    self.record("list_project_repositories");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    let id = serde_json::from_value(serde_json::json!("33333333-3333-4333-8333-333333333333")).unwrap();
    let first_page = query.page().after().is_none();
    Ok(RepositoryPageProjection {
      items: first_page
        .then(|| RepositorySummaryProjection {
          id,
          project_id: query.page().project_id(),
          name: serde_json::from_value(serde_json::json!("source")).unwrap(),
          version: serde_json::from_value(serde_json::json!(1)).unwrap(),
          published_at: Timestamp::from_unix_millis(1_700_000_000_100).unwrap(),
        })
        .into_iter()
        .collect(),
      next_cursor: first_page.then_some(id),
    })
  }
}

#[async_trait]
impl ManagementQueryUseCase<ListProjectBuildConfigurationsQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: ListProjectBuildConfigurationsQuery,
  ) -> Result<BuildConfigurationPageProjection, Self::Error> {
    self.record("list_project_build_configurations");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    let id = serde_json::from_value(serde_json::json!("44444444-4444-4444-8444-444444444444")).unwrap();
    let first_page = query.page().after().is_none();
    Ok(BuildConfigurationPageProjection {
      items: first_page
        .then(|| BuildConfigurationSummaryProjection {
          id,
          project_id: query.page().project_id(),
          name: serde_json::from_value(serde_json::json!("release")).unwrap(),
          version: serde_json::from_value(serde_json::json!(1)).unwrap(),
          enabled: true,
          published_at: Timestamp::from_unix_millis(1_700_000_000_200).unwrap(),
        })
        .into_iter()
        .collect(),
      next_cursor: first_page.then_some(id),
    })
  }
}

#[async_trait]
impl ManagementQueryUseCase<ListProjectTriggerDefinitionsQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: ListProjectTriggerDefinitionsQuery,
  ) -> Result<TriggerDefinitionPageProjection, Self::Error> {
    self.record("list_project_trigger_definitions");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    let id = serde_json::from_value(serde_json::json!("88888888-8888-4888-8888-888888888888")).unwrap();
    let first_page = query.page().after().is_none();
    Ok(TriggerDefinitionPageProjection {
      items: first_page
        .then(|| TriggerDefinitionSummaryProjection {
          id,
          project_id: query.page().project_id(),
          configuration_id: "44444444-4444-4444-8444-444444444444".parse().unwrap(),
          configuration_version: serde_json::from_value(serde_json::json!(1)).unwrap(),
          version: serde_json::from_value(serde_json::json!(1)).unwrap(),
          kind: TriggerDefinitionKindProjection::Scheduled,
          enabled: true,
          published_at: Timestamp::from_unix_millis(1_700_000_000_300).unwrap(),
        })
        .into_iter()
        .collect(),
      next_cursor: first_page.then_some(id),
    })
  }
}

#[async_trait]
impl ManagementQueryUseCase<ListProjectBuildsQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: ListProjectBuildsQuery,
  ) -> Result<BuildPageProjection, Self::Error> {
    self.record("list_project_builds");
    self.build_list_queries.lock().unwrap().push(query.clone());
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    let id = "99999999-9999-4999-8999-999999999999".parse().unwrap();
    let created_at = Timestamp::from_unix_millis(1_700_000_000_500).unwrap();
    let first_page = query.after().is_none();
    let (state, current_attempt_state) = self
      .build_summary_states
      .expect("successful fixture must define states");
    Ok(BuildPageProjection {
      items: first_page
        .then(|| BuildSummaryProjection {
          id,
          project_id: query.project_id(),
          configuration_id: "44444444-4444-4444-8444-444444444444".parse().unwrap(),
          configuration_version: serde_json::from_value(serde_json::json!(3)).unwrap(),
          cause: TriggerCauseProjection::Manual,
          state,
          created_at,
          current_attempt_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".parse().unwrap(),
          current_attempt_number: serde_json::from_value(serde_json::json!(2)).unwrap(),
          current_attempt_state,
          terminal_at: Some(Timestamp::from_unix_millis(1_700_000_001_500).unwrap()),
        })
        .into_iter()
        .collect(),
      next_cursor: first_page.then_some(BuildPageCursor::new(created_at, id)),
    })
  }
}

#[async_trait]
impl ManagementQueryUseCase<SearchResourcesQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: SearchResourcesQuery,
  ) -> Result<ResourceSearchPageProjection, Self::Error> {
    self.record("search_resources");
    let has_cursor = query.after().is_some();
    let kinds = query.kinds().clone();
    let limit = usize::from(query.limit());
    self.resource_search_queries.lock().unwrap().push(query);
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(ResourceSearchPageProjection {
      items: if has_cursor {
        Vec::new()
      } else {
        vec![
          ResourceSearchSummaryProjection {
            resource: ResourceSearchIdentityProjection::Project(
              "11111111-1111-4111-8111-111111111111".parse().unwrap(),
            ),
            label: "Alpha".to_owned(),
            context: None,
          },
          ResourceSearchSummaryProjection {
            resource: ResourceSearchIdentityProjection::Build(
              "99999999-9999-4999-8999-999999999999".parse().unwrap(),
            ),
            label: "Alpha Release".to_owned(),
            context: Some("Root Project".to_owned()),
          },
        ]
        .into_iter()
        .filter(|item| kinds.as_set().contains(&match item.resource {
          ResourceSearchIdentityProjection::Project(_) => octacity_server_application::ResourceSearchKind::Project,
          ResourceSearchIdentityProjection::Build(_) => octacity_server_application::ResourceSearchKind::Build,
          ResourceSearchIdentityProjection::Agent(_) => octacity_server_application::ResourceSearchKind::Agent,
          ResourceSearchIdentityProjection::AgentPool(_) => {
            octacity_server_application::ResourceSearchKind::AgentPool
          }
        }))
        .take(limit)
        .collect()
      },
      next_cursor: (!has_cursor && kinds == octacity_server_application::ResourceSearchKinds::all() && limit >= 2)
        .then(|| {
        ResourceSearchCursor::decode(
          "eyJ2ZXJzaW9uIjoxLCJxdWVyeSI6ImFscGhhIiwia2luZHMiOlsicHJvamVjdCIsImJ1aWxkIiwiYWdlbnQiLCJhZ2VudF9wb29sIl0sInBvc2l0aW9uIjp7InJhbmsiOiJuYW1lX3ByZWZpeCIsImtpbmQiOiJidWlsZCIsIm5vcm1hbGl6ZWRfbGFiZWwiOiJhbHBoYSByZWxlYXNlIiwicmVzb3VyY2UiOnsia2luZCI6ImJ1aWxkIiwiaWQiOiI5OTk5OTk5OS05OTk5LTQ5OTktODk5OS05OTk5OTk5OTk5OTkifX19",
        )
        .expect("fixture cursor is canonical")
      }),
    })
  }
}

#[async_trait]
impl ManagementQueryUseCase<ListOperatorAttentionQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: ListOperatorAttentionQuery,
  ) -> Result<OperatorAttentionPageProjection, Self::Error> {
    self.record("list_operator_attention");
    let has_cursor = query.after().is_some();
    self.operator_attention_queries.lock().unwrap().push(query);
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(OperatorAttentionPageProjection {
      items: if has_cursor {
        Vec::new()
      } else {
        vec![OperatorAttentionItemProjection {
          id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
            .parse::<OperatorAttentionId>()
            .unwrap(),
          category: OperatorAttentionCategory::CriticalSystem,
          severity: OperatorAttentionSeverity::Critical,
          code: "storage_pressure".to_owned(),
          summary: "Artifact storage is approaching its configured limit".to_owned(),
          occurred_at_unix_ms: 1_700_000_001_000,
          resolved_at_unix_ms: None,
          target: None,
        }]
      },
      next_cursor: None,
    })
  }
}

#[async_trait]
impl ManagementQueryUseCase<SearchBuildLogsQuery> for RecordingApplication {
  type Error = BuildLogSearchError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: SearchBuildLogsQuery,
  ) -> Result<<SearchBuildLogsQuery as Query>::Outcome, Self::Error> {
    self.record("search_build_logs");
    self.log_search_queries.lock().unwrap().push(query.clone());
    if !self.successful_workflow {
      return Err(BuildLogSearchError::SearchIndex(LogSearchError::Unavailable));
    }
    let has_cursor = query.search.after.is_some();
    Ok(BuildLogSearchPageProjection {
      items: if has_cursor {
        Vec::new()
      } else {
        vec![BuildLogSearchHitProjection {
          chunk_id: "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee".parse().unwrap(),
          build_id: "99999999-9999-4999-8999-999999999999".parse().unwrap(),
          attempt_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".parse().unwrap(),
          job_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".parse().unwrap(),
          stream: BuildLogStream::Stderr,
          first_sequence: 41,
          last_sequence: 44,
          occurred_at_unix_ms: 1_700_000_000_000,
          snippet: "error[E0425]: cannot find value `answer` in this scope".to_owned(),
        }]
      },
      next_cursor: (!has_cursor).then(|| BuildLogSearchCursorProjection {
        occurred_at_unix_ms: 1_700_000_000_000,
        chunk_id: "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee".parse().unwrap(),
      }),
      freshness: BuildLogSearchFreshnessProjection {
        indexed_through: Some(17),
        committed_through: Some(19),
        caught_up: false,
      },
    })
  }
}

#[async_trait]
impl ManagementQueryUseCase<ListAuditFactsQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    _query: ListAuditFactsQuery,
  ) -> Result<<ListAuditFactsQuery as Query>::Outcome, Self::Error> {
    self.record("list_audit_facts");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(AuditFactPageProjection {
      items: vec![AuditFactProjection {
        id: "dddddddd-dddd-4ddd-8ddd-dddddddddddd".parse().unwrap(),
        actor: AuditActorProjection {
          kind: AuditActorKind::UnauthenticatedManagement,
          identity: None,
        },
        operation: "cancel-build".to_owned(),
        target_kind: "build".to_owned(),
        target_identity: "99999999-9999-4999-8999-999999999999".to_owned(),
        request_identity: Some("cancel-build:cancel-17".to_owned()),
        idempotency_key: Some("cancel-17".to_owned()),
        outcome: AuditOutcome::Accepted,
        metadata: serde_json::json!({"cancelled_job_count": 2}),
        occurred_at_unix_ms: 1_700_000_000_000,
      }],
      next_cursor: None,
    })
  }
}

#[async_trait]
impl ManagementCommandUseCase<CreateProjectCommand> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
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
impl ManagementCommandUseCase<CreatePipelineCommand> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
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
impl ManagementCommandUseCase<CreateRepositoryCommand> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
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
impl ManagementCommandUseCase<CreateBuildConfigurationCommand> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
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
impl ManagementCommandUseCase<AcceptManualTriggerCommand> for RecordingApplication {
  type Error = ManualTriggerError;

  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
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

fn management_contract_requests() -> Vec<Request<Body>> {
  let project_id = "11111111-1111-4111-8111-111111111111";
  let pipeline_id = "22222222-2222-4222-8222-222222222222";
  let repository_id = "33333333-3333-4333-8333-333333333333";
  let configuration_id = "44444444-4444-4444-8444-444444444444";
  let job_id = "55555555-5555-4555-8555-555555555555";
  let pool_id = "66666666-6666-4666-8666-666666666666";
  let agent_id = "77777777-7777-4777-8777-777777777777";
  let build_id = "99999999-9999-4999-8999-999999999999";
  let attempt_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
  let schedule_id = "88888888-8888-4888-8888-888888888888";
  let integration_id = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
  let artifact_id = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
  let cache_session_id = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";

  vec![
    empty_request("GET", "/api/v1/operations/metadata", None),
    empty_request("GET", "/api/v1/audit-facts?limit=10", None),
    empty_request("GET", "/api/v1/search?query=alpha&limit=10", None),
    empty_request(
      "GET",
      &format!("/api/v1/operator-attention?build_ids={build_id}&limit=10"),
      None,
    ),
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
    empty_request(
      "GET",
      &format!("/api/v1/projects/{project_id}/pipelines?limit=10"),
      None,
    ),
    empty_request(
      "GET",
      &format!("/api/v1/projects/{project_id}/repositories?limit=10"),
      None,
    ),
    empty_request(
      "GET",
      &format!("/api/v1/projects/{project_id}/build-configurations?limit=10"),
      None,
    ),
    empty_request(
      "GET",
      &format!("/api/v1/projects/{project_id}/trigger-definitions?limit=10"),
      None,
    ),
    empty_request("GET", &format!("/api/v1/projects/{project_id}/builds?limit=10"), None),
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
    empty_request(
      "GET",
      &format!("/api/v1/trigger-definitions/manual/{schedule_id}/versions/1"),
      None,
    ),
    json_request(
      "POST",
      "/api/v1/trigger-definitions/scheduled",
      "create-schedule",
      None,
      &format!(
        r#"{{"configuration_id":"{configuration_id}","configuration_version":1,"enabled":true,"schedule":{{"expression":"0 * * * * * *","timezone":"UTC","missed_run_policy":{{"kind":"run_once"}}}},"build":{{"source":{{"kind":"exact_revision","value":"0123456789abcdef"}},"parameters":{{}},"priority":0}}}}"#
      ),
    ),
    json_request(
      "POST",
      "/api/v1/trigger-definitions/internal",
      "create-internal-trigger",
      None,
      include_str!("../fixtures/v1/create-internal-trigger-request.json"),
    ),
    empty_request("GET", "/api/v1/trigger-definitions/internal?limit=10", None),
    json_request(
      "POST",
      &format!("/api/v1/trigger-definitions/internal/{schedule_id}/versions"),
      "publish-internal-trigger",
      Some("\"1\""),
      include_str!("../fixtures/v1/create-internal-trigger-request.json"),
    ),
    empty_request(
      "GET",
      &format!("/api/v1/trigger-definitions/internal/{schedule_id}/versions/1"),
      None,
    ),
    json_request(
      "POST",
      "/api/v1/webhook-integrations/unmanaged",
      "create-unmanaged-webhook",
      None,
      &format!(
        r#"{{"configuration_id":"{configuration_id}","configuration_version":1,"enabled":true,"adapter_id":"github","adapter_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","verification_material_handle":"secret:webhook","verification_headers":["x-hub-signature-256"],"repository_id":"{repository_id}","event_kind":"push","parameters":{{}},"priority":0}}"#
      ),
    ),
    json_request(
      "POST",
      "/api/v1/webhook-integrations/managed",
      "create-managed-webhook",
      None,
      &format!(
        r#"{{"configuration_id":"{configuration_id}","configuration_version":1,"enabled":true,"adapter_id":"github","adapter_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","verification_material_handle":"secret:webhook","verification_headers":["x-hub-signature-256"],"administration_credential_handle":"secret:github-admin","repository_id":"{repository_id}","event_kind":"push","parameters":{{}},"priority":0}}"#
      ),
    ),
    empty_request(
      "POST",
      &format!("/api/v1/webhook-integrations/managed/{integration_id}/observe"),
      Some(("observe-managed-webhook", "\"1\"")),
    ),
    empty_request(
      "POST",
      &format!("/api/v1/webhook-integrations/managed/{integration_id}/rotate"),
      Some(("rotate-managed-webhook", "\"1\"")),
    ),
    empty_request(
      "DELETE",
      &format!("/api/v1/webhook-integrations/managed/{integration_id}"),
      Some(("delete-managed-webhook", "\"1\"")),
    ),
    empty_request("GET", &format!("/api/v1/schedules/{schedule_id}/versions/1"), None),
    json_request(
      "POST",
      "/api/v1/triggers/manual",
      "release-main-2026-09-18",
      None,
      include_str!("../fixtures/v1/accept-manual-trigger-request.json"),
    ),
    empty_request("GET", &format!("/api/v1/builds/{build_id}"), None),
    empty_request("GET", &format!("/api/v1/builds/{build_id}/retention"), None),
    json_request(
      "POST",
      &format!("/api/v1/builds/{build_id}/retention/hold"),
      "place-retention-hold",
      None,
      r#"{"reason":"incident investigation","expires_at_unix_ms":null}"#,
    ),
    empty_request(
      "POST",
      &format!("/api/v1/builds/{build_id}/retention/hold/release"),
      Some(("release-retention-hold", "\"1\"")),
    ),
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
    empty_request(
      "GET",
      &format!("/api/v1/projects/{project_id}/build-logs/search?query=error&mode=full_text"),
      None,
    ),
    empty_request("GET", &format!("/api/v1/artifacts/{artifact_id}"), None),
    empty_request("GET", &format!("/api/v1/builds/{build_id}/artifacts?limit=10"), None),
    empty_request("POST", &format!("/api/v1/artifacts/{artifact_id}/download"), None),
    empty_request("GET", &format!("/api/v1/cache-sessions/{cache_session_id}"), None),
    empty_request(
      "GET",
      &format!("/api/v1/builds/{build_id}/cache-sessions?limit=10"),
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
    empty_request("GET", &format!("/api/v1/agent-pools/{pool_id}"), None),
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
  ]
}

#[tokio::test]
async fn every_management_operation_is_allowed_before_application_dispatch() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );
  let requests = management_contract_requests();
  assert_eq!(requests.len(), MANAGEMENT_OPERATIONS.len());

  for (index, request) in requests.into_iter().enumerate() {
    let expected = if request.uri().path() == "/api/v1/operations/metadata" {
      StatusCode::OK
    } else {
      StatusCode::SERVICE_UNAVAILABLE
    };
    let response = routes.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), expected, "request {index}");
  }

  assert_eq!(
    *application.calls.lock().unwrap(),
    [
      "get_operational_metadata",
      "list_audit_facts",
      "search_resources",
      "list_operator_attention",
      "create_project",
      "rename_project",
      "move_project",
      "publish_project_policy",
      "delete_project",
      "get_project",
      "list_projects",
      "list_project_pipelines",
      "list_project_repositories",
      "list_project_build_configurations",
      "list_project_trigger_definitions",
      "list_project_builds",
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
      "get_manual_trigger_definition",
      "create_schedule",
      "create_internal_trigger",
      "list_internal_triggers",
      "publish_internal_trigger",
      "get_internal_trigger",
      "create_unmanaged_webhook",
      "create_managed_webhook",
      "observe_managed_webhook",
      "rotate_managed_webhook",
      "delete_managed_webhook",
      "get_schedule",
      "accept_manual_trigger",
      "get_build",
      "get_build_result_retention",
      "place_build_result_hold",
      "release_build_result_hold",
      "cancel_build",
      "retry_build",
      "get_attempt",
      "get_job",
      "read_job_events",
      "search_build_logs",
      "get_artifact",
      "list_build_artifacts",
      "authorize_artifact_download",
      "get_cache_session",
      "list_build_cache_sessions",
      "issue_agent_enrollment",
      "create_agent_pool",
      "publish_agent_pool",
      "get_agent_pool",
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
async fn every_management_operation_denies_before_application_and_protected_side_effects() {
  let application = Arc::new(RecordingApplication::default());
  let policy = Arc::new(DenyAllManagementPolicy::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application_with_policy(Arc::clone(&application), Arc::clone(&application), policy.clone()),
  );
  let requests = management_contract_requests();
  assert_eq!(requests.len(), MANAGEMENT_OPERATIONS.len());

  for (index, request) in requests.into_iter().enumerate() {
    let response = routes.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "request {index}");
    let request_id = response.headers()["x-request-id"].to_str().unwrap().to_owned();
    let body: serde_json::Value =
      serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(
      body,
      serde_json::json!({
        "code": "forbidden",
        "message": "management operation is forbidden",
        "request_id": request_id,
      }),
      "request {index}"
    );
  }

  assert_eq!(policy.decisions.load(Ordering::SeqCst), MANAGEMENT_OPERATIONS.len());
  assert!(application.calls.lock().unwrap().is_empty());
  assert_eq!(
    *application.protected_side_effects.lock().unwrap(),
    ProtectedSideEffectCounts::default(),
    "denial must precede handlers, transactions, audit, outbox, idempotency, and transfer capabilities"
  );
}

#[tokio::test]
async fn unsupported_managed_registration_capability_has_a_stable_rest_error() {
  let application = Arc::new(RecordingApplication {
    capability_unavailable_for_managed: true,
    ..RecordingApplication::default()
  });
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );
  let response = routes
    .clone()
    .oneshot(json_request(
      "POST",
      "/api/v1/webhook-integrations/managed",
      "unsupported-managed-webhook",
      None,
      include_str!("../fixtures/v1/create-managed-webhook-request.json"),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["code"], serde_json::json!(ErrorCode::CapabilityUnavailable));
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
    .clone()
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

  let oversized_reason = "x".repeat(513);
  let response = routes
    .clone()
    .oneshot(json_request(
      "POST",
      "/api/v1/builds/99999999-9999-4999-8999-999999999999/retention/hold",
      "oversized-retention-reason",
      None,
      &serde_json::json!({"reason": oversized_reason}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);

  let oversized_utf8_reason = "я".repeat(257);
  let response = routes
    .oneshot(json_request(
      "POST",
      "/api/v1/builds/99999999-9999-4999-8999-999999999999/retention/hold",
      "oversized-utf8-retention-reason",
      None,
      &serde_json::json!({"reason": oversized_utf8_reason}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
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

fn normalize_documented_success_response(target: &str, body: &mut serde_json::Value) {
  if target == "/api/v1/triggers/manual" {
    return;
  }
  let resource = body["resource"].as_object_mut().unwrap();
  let id = resource["id"].as_str().unwrap();
  uuid::Uuid::parse_str(id).expect("created resource identity must remain a UUID");
  resource.insert("id".to_owned(), serde_json::json!("<generated UUID>"));
  let timestamp_fields: &[&str] = if target == "/api/v1/projects" {
    &["created_at_unix_ms", "updated_at_unix_ms"]
  } else {
    &["published_at_unix_ms"]
  };
  for field in timestamp_fields {
    assert!(resource[*field].as_i64().is_some_and(|value| value > 0));
    resource.insert((*field).to_owned(), serde_json::json!("<generated timestamp>"));
  }
}

fn documented_success_response(target: &str, request: &serde_json::Value) -> serde_json::Value {
  match target {
    "/api/v1/projects" => serde_json::json!({
      "disposition": "applied",
      "resource": {
        "id": "<generated UUID>",
        "parent_id": request["parent_id"],
        "name": request["name"],
        "version": 1,
        "created_at_unix_ms": "<generated timestamp>",
        "updated_at_unix_ms": "<generated timestamp>",
      }
    }),
    "/api/v1/pipelines" => serde_json::json!({
      "disposition": "applied",
      "resource": {
        "id": "<generated UUID>",
        "project_id": request["project_id"],
        "name": request["name"],
        "version": 1,
        "dag": request["dag"],
        "published_at_unix_ms": "<generated timestamp>",
      }
    }),
    "/api/v1/repositories" => serde_json::json!({
      "disposition": "applied",
      "resource": {
        "id": "<generated UUID>",
        "project_id": request["project_id"],
        "name": request["name"],
        "version": 1,
        "definition": request["definition"],
        "published_at_unix_ms": "<generated timestamp>",
      }
    }),
    "/api/v1/build-configurations" => {
      let mut definition = request["definition"].clone();
      definition["secrets_profile"] = serde_json::Value::Null;
      serde_json::json!({
        "disposition": "applied",
        "resource": {
          "id": "<generated UUID>",
          "project_id": request["project_id"],
          "name": request["name"],
          "version": 1,
          "definition": definition,
          "published_at_unix_ms": "<generated timestamp>",
        }
      })
    }
    "/api/v1/triggers/manual" => serde_json::json!({
      "outcome": "accepted",
      "disposition": "applied",
      "trigger_occurrence_id": "88888888-8888-4888-8888-888888888888",
      "build_id": "99999999-9999-4999-8999-999999999999",
      "attempt_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
      "ready_job_ids": ["bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"],
    }),
    _ => panic!("unexpected documented workflow target {target}"),
  }
}

#[tokio::test]
async fn documented_successful_requests_retain_their_status_codes_and_bodies() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );
  let requests = documented_http_requests(DOCUMENTED_SECTION_FOUR_WORKFLOW);
  assert_eq!(requests.len(), 5, "every workflow step must be executable HTTP");
  for (request, expected_status) in requests.iter().zip([201, 201, 201, 201, 200]) {
    let request_body = request.json_body();
    let response = routes.clone().oneshot(request.to_request()).await.unwrap();
    assert_eq!(
      response.status().as_u16(),
      expected_status,
      "{} {} must return its documented success status",
      request.method,
      request.target
    );
    let mut body: serde_json::Value =
      serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    normalize_documented_success_response(&request.target, &mut body);
    assert_eq!(body, documented_success_response(&request.target, &request_body));
  }
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

#[test]
fn documented_http_requests_support_windows_line_endings() {
  let unix_document = DOCUMENTED_SECTION_FOUR_WORKFLOW.replace("\r\n", "\n");
  let document = unix_document.replace('\n', "\r\n");

  assert_eq!(documented_http_requests(&document).len(), 5);
}
