use std::sync::Arc;

use octacity_server_application::{
  AcceptManualTriggerCommand, ApplicationError, AuthorizeArtifactDownloadQuery, AuthorizedManagementCommandHandler,
  AuthorizedManagementQueryHandler, BuildLogSearchError, CancelBuildCommand, CreateAgentPoolCommand,
  CreateBuildConfigurationCommand, CreateInternalTriggerCommand, CreateManagedWebhookCommand, CreatePipelineCommand,
  CreateProjectCommand, CreateRepositoryCommand, CreateScheduleCommand, CreateTriggerDefinitionCommand,
  CreateUnmanagedWebhookCommand, DeleteAgentPoolCommand, DeleteManagedWebhookRegistrationCommand, DeleteProjectCommand,
  DrainAgentCommand, GetAgentPoolQuery, GetAgentQuery, GetArtifactQuery, GetAttemptQuery, GetBuildConfigurationQuery,
  GetBuildQuery, GetBuildResultRetentionQuery, GetCacheSessionQuery, GetInternalTriggerQuery, GetJobQuery,
  GetManualTriggerDefinitionQuery, GetOperationalMetadataQuery, GetPipelineQuery, GetProjectQuery, GetRepositoryQuery,
  GetScheduleQuery, IssueAgentEnrollmentCommand, ListAgentPoolsQuery, ListAgentsQuery, ListAuditFactsQuery,
  ListBuildArtifactsQuery, ListBuildCacheSessionsQuery, ListInternalTriggersQuery, ListProjectBuildConfigurationsQuery,
  ListProjectBuildsQuery, ListProjectPipelinesQuery, ListProjectRepositoriesQuery, ListProjectTriggerDefinitionsQuery,
  ListProjectsQuery, ManualTriggerError, MoveProjectCommand, ObserveManagedWebhookRegistrationCommand,
  PlaceBuildResultHoldCommand, PublishAgentPoolVersionCommand, PublishBuildConfigurationVersionCommand,
  PublishInternalTriggerVersionCommand, PublishPipelineVersionCommand, PublishProjectPolicyCommand,
  PublishRepositoryVersionCommand, ReadJobEventsQuery, ReassignAgentPoolCommand, ReleaseBuildResultHoldCommand,
  RenameProjectCommand, RetryBuildCommand, RotateManagedWebhookRegistrationCommand, SearchBuildLogsQuery,
};

type ProjectCreate = dyn AuthorizedManagementCommandHandler<CreateProjectCommand, Error = ApplicationError>;
type ProjectRename = dyn AuthorizedManagementCommandHandler<RenameProjectCommand, Error = ApplicationError>;
type ProjectMove = dyn AuthorizedManagementCommandHandler<MoveProjectCommand, Error = ApplicationError>;
type ProjectDelete = dyn AuthorizedManagementCommandHandler<DeleteProjectCommand, Error = ApplicationError>;
type ProjectGet = dyn AuthorizedManagementQueryHandler<GetProjectQuery, Error = ApplicationError>;
type ProjectList = dyn AuthorizedManagementQueryHandler<ListProjectsQuery, Error = ApplicationError>;
type PipelineCreate = dyn AuthorizedManagementCommandHandler<CreatePipelineCommand, Error = ApplicationError>;
type PipelinePublish = dyn AuthorizedManagementCommandHandler<PublishPipelineVersionCommand, Error = ApplicationError>;
type PipelineGet = dyn AuthorizedManagementQueryHandler<GetPipelineQuery, Error = ApplicationError>;
type PipelineList = dyn AuthorizedManagementQueryHandler<ListProjectPipelinesQuery, Error = ApplicationError>;
type RepositoryCreate = dyn AuthorizedManagementCommandHandler<CreateRepositoryCommand, Error = ApplicationError>;
type RepositoryPublish =
  dyn AuthorizedManagementCommandHandler<PublishRepositoryVersionCommand, Error = ApplicationError>;
type RepositoryGet = dyn AuthorizedManagementQueryHandler<GetRepositoryQuery, Error = ApplicationError>;
type RepositoryList = dyn AuthorizedManagementQueryHandler<ListProjectRepositoriesQuery, Error = ApplicationError>;
type ConfigurationCreate =
  dyn AuthorizedManagementCommandHandler<CreateBuildConfigurationCommand, Error = ApplicationError>;
type ConfigurationPublish =
  dyn AuthorizedManagementCommandHandler<PublishBuildConfigurationVersionCommand, Error = ApplicationError>;
type ConfigurationGet = dyn AuthorizedManagementQueryHandler<GetBuildConfigurationQuery, Error = ApplicationError>;
type ConfigurationList =
  dyn AuthorizedManagementQueryHandler<ListProjectBuildConfigurationsQuery, Error = ApplicationError>;
type AgentPoolCreate = dyn AuthorizedManagementCommandHandler<CreateAgentPoolCommand, Error = ApplicationError>;
type AgentPoolPublish =
  dyn AuthorizedManagementCommandHandler<PublishAgentPoolVersionCommand, Error = ApplicationError>;
type AgentPoolDelete = dyn AuthorizedManagementCommandHandler<DeleteAgentPoolCommand, Error = ApplicationError>;
type AgentPoolGet = dyn AuthorizedManagementQueryHandler<GetAgentPoolQuery, Error = ApplicationError>;
type AgentPoolList = dyn AuthorizedManagementQueryHandler<ListAgentPoolsQuery, Error = ApplicationError>;
type AgentGet = dyn AuthorizedManagementQueryHandler<GetAgentQuery, Error = ApplicationError>;
type AgentList = dyn AuthorizedManagementQueryHandler<ListAgentsQuery, Error = ApplicationError>;
type AgentReassign = dyn AuthorizedManagementCommandHandler<ReassignAgentPoolCommand, Error = ApplicationError>;
type AgentDrain = dyn AuthorizedManagementCommandHandler<DrainAgentCommand, Error = ApplicationError>;
type AgentEnrollmentIssue =
  dyn AuthorizedManagementCommandHandler<IssueAgentEnrollmentCommand, Error = ApplicationError>;
type BuildGet = dyn AuthorizedManagementQueryHandler<GetBuildQuery, Error = ApplicationError>;
type BuildList = dyn AuthorizedManagementQueryHandler<ListProjectBuildsQuery, Error = ApplicationError>;
type AttemptGet = dyn AuthorizedManagementQueryHandler<GetAttemptQuery, Error = ApplicationError>;
type JobGet = dyn AuthorizedManagementQueryHandler<GetJobQuery, Error = ApplicationError>;
type BuildCancel = dyn AuthorizedManagementCommandHandler<CancelBuildCommand, Error = ApplicationError>;
type BuildRetry = dyn AuthorizedManagementCommandHandler<RetryBuildCommand, Error = ApplicationError>;
type ProjectPolicyPublish =
  dyn AuthorizedManagementCommandHandler<PublishProjectPolicyCommand, Error = ApplicationError>;
type TriggerDefinitionCreate =
  dyn AuthorizedManagementCommandHandler<CreateTriggerDefinitionCommand, Error = ApplicationError>;
type TriggerDefinitionList =
  dyn AuthorizedManagementQueryHandler<ListProjectTriggerDefinitionsQuery, Error = ApplicationError>;
type ManualTriggerDefinitionGet =
  dyn AuthorizedManagementQueryHandler<GetManualTriggerDefinitionQuery, Error = ApplicationError>;
type UnmanagedWebhookCreate =
  dyn AuthorizedManagementCommandHandler<CreateUnmanagedWebhookCommand, Error = ApplicationError>;
type ManagedWebhookCreate =
  dyn AuthorizedManagementCommandHandler<CreateManagedWebhookCommand, Error = ApplicationError>;
type ManagedWebhookObserve =
  dyn AuthorizedManagementCommandHandler<ObserveManagedWebhookRegistrationCommand, Error = ApplicationError>;
type ManagedWebhookRotate =
  dyn AuthorizedManagementCommandHandler<RotateManagedWebhookRegistrationCommand, Error = ApplicationError>;
type ManagedWebhookDelete =
  dyn AuthorizedManagementCommandHandler<DeleteManagedWebhookRegistrationCommand, Error = ApplicationError>;
type ManualTriggerAccept =
  dyn AuthorizedManagementCommandHandler<AcceptManualTriggerCommand, Error = ManualTriggerError>;
type JobEventsRead = dyn AuthorizedManagementQueryHandler<ReadJobEventsQuery, Error = ApplicationError>;
type ScheduleCreate = dyn AuthorizedManagementCommandHandler<CreateScheduleCommand, Error = ApplicationError>;
type ScheduleGet = dyn AuthorizedManagementQueryHandler<GetScheduleQuery, Error = ApplicationError>;
type InternalTriggerCreate =
  dyn AuthorizedManagementCommandHandler<CreateInternalTriggerCommand, Error = ApplicationError>;
type InternalTriggerPublish =
  dyn AuthorizedManagementCommandHandler<PublishInternalTriggerVersionCommand, Error = ApplicationError>;
type InternalTriggerGet = dyn AuthorizedManagementQueryHandler<GetInternalTriggerQuery, Error = ApplicationError>;
type InternalTriggerList = dyn AuthorizedManagementQueryHandler<ListInternalTriggersQuery, Error = ApplicationError>;
type ArtifactGet = dyn AuthorizedManagementQueryHandler<GetArtifactQuery, Error = ApplicationError>;
type ArtifactList = dyn AuthorizedManagementQueryHandler<ListBuildArtifactsQuery, Error = ApplicationError>;
type ArtifactDownload = dyn AuthorizedManagementQueryHandler<AuthorizeArtifactDownloadQuery, Error = ApplicationError>;
type CacheSessionGet = dyn AuthorizedManagementQueryHandler<GetCacheSessionQuery, Error = ApplicationError>;
type CacheSessionList = dyn AuthorizedManagementQueryHandler<ListBuildCacheSessionsQuery, Error = ApplicationError>;
type BuildLogsSearch = dyn AuthorizedManagementQueryHandler<SearchBuildLogsQuery, Error = BuildLogSearchError>;
type BuildRetentionGet = dyn AuthorizedManagementQueryHandler<GetBuildResultRetentionQuery, Error = ApplicationError>;
type BuildRetentionPlace =
  dyn AuthorizedManagementCommandHandler<PlaceBuildResultHoldCommand, Error = ApplicationError>;
type BuildRetentionRelease =
  dyn AuthorizedManagementCommandHandler<ReleaseBuildResultHoldCommand, Error = ApplicationError>;
type AuditFactList = dyn AuthorizedManagementQueryHandler<ListAuditFactsQuery, Error = ApplicationError>;
type OperationalMetadataGet =
  dyn AuthorizedManagementQueryHandler<GetOperationalMetadataQuery, Error = ApplicationError>;

/// Type-erased operational metadata query consumed by REST.
pub struct OperationalMetadataManagementApplication(pub(super) Arc<OperationalMetadataGet>);

impl OperationalMetadataManagementApplication {
  /// Erases the composition-owned operational metadata query service.
  pub fn new<Q>(queries: Arc<Q>) -> Self
  where
    Q: AuthorizedManagementQueryHandler<GetOperationalMetadataQuery, Error = ApplicationError> + 'static,
  {
    Self(queries)
  }
}

/// Type-erased Project handlers consumed by REST.
pub struct ProjectManagementApplication {
  pub(super) create: Arc<ProjectCreate>,
  pub(super) rename: Arc<ProjectRename>,
  pub(super) move_project: Arc<ProjectMove>,
  pub(super) delete: Arc<ProjectDelete>,
  pub(super) get: Arc<ProjectGet>,
  pub(super) list: Arc<ProjectList>,
}

impl ProjectManagementApplication {
  /// Erases one Project service behind its endpoint capabilities.
  pub fn new<C, Q>(commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<CreateProjectCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<RenameProjectCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<MoveProjectCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<DeleteProjectCommand, Error = ApplicationError>
      + 'static,
    Q: AuthorizedManagementQueryHandler<GetProjectQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListProjectsQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      create: commands.clone(),
      rename: commands.clone(),
      move_project: commands.clone(),
      delete: commands,
      get: queries.clone(),
      list: queries,
    }
  }
}

/// Type-erased Pipeline handlers consumed by REST.
pub struct PipelineManagementApplication {
  pub(super) create: Arc<PipelineCreate>,
  pub(super) publish: Arc<PipelinePublish>,
  pub(super) get: Arc<PipelineGet>,
  pub(super) list: Arc<PipelineList>,
}

impl PipelineManagementApplication {
  /// Erases one Pipeline service behind its endpoint capabilities.
  pub fn new<C, Q>(commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<CreatePipelineCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<PublishPipelineVersionCommand, Error = ApplicationError>
      + 'static,
    Q: AuthorizedManagementQueryHandler<GetPipelineQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListProjectPipelinesQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      create: commands.clone(),
      publish: commands,
      get: queries.clone(),
      list: queries,
    }
  }
}

/// Type-erased Repository and Build Configuration handlers consumed by REST.
pub struct ConfigurationManagementApplication {
  pub(super) create_repository: Arc<RepositoryCreate>,
  pub(super) publish_repository: Arc<RepositoryPublish>,
  pub(super) get_repository: Arc<RepositoryGet>,
  pub(super) list_repositories: Arc<RepositoryList>,
  pub(super) create_configuration: Arc<ConfigurationCreate>,
  pub(super) publish_configuration: Arc<ConfigurationPublish>,
  pub(super) get_configuration: Arc<ConfigurationGet>,
  pub(super) list_configurations: Arc<ConfigurationList>,
}

impl ConfigurationManagementApplication {
  /// Erases one configuration service behind its endpoint capabilities.
  pub fn new<C, Q>(commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<CreateRepositoryCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<PublishRepositoryVersionCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<CreateBuildConfigurationCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<PublishBuildConfigurationVersionCommand, Error = ApplicationError>
      + 'static,
    Q: AuthorizedManagementQueryHandler<GetRepositoryQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetBuildConfigurationQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListProjectRepositoriesQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListProjectBuildConfigurationsQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      create_repository: commands.clone(),
      publish_repository: commands.clone(),
      get_repository: queries.clone(),
      list_repositories: queries.clone(),
      create_configuration: commands.clone(),
      publish_configuration: commands,
      get_configuration: queries.clone(),
      list_configurations: queries,
    }
  }
}

/// Type-erased Agent and Agent Pool handlers consumed by REST.
pub struct AgentManagementApplication {
  pub(super) pools: AgentPoolManagementApplication,
  pub(super) agents: AgentEndpoints,
}

pub(super) struct AgentPoolManagementApplication {
  pub(super) create: Arc<AgentPoolCreate>,
  pub(super) publish: Arc<AgentPoolPublish>,
  pub(super) delete: Arc<AgentPoolDelete>,
  pub(super) get: Arc<AgentPoolGet>,
  pub(super) list: Arc<AgentPoolList>,
}

pub(super) struct AgentEndpoints {
  pub(super) issue_enrollment: Arc<AgentEnrollmentIssue>,
  pub(super) get: Arc<AgentGet>,
  pub(super) list: Arc<AgentList>,
  pub(super) reassign: Arc<AgentReassign>,
  pub(super) drain: Arc<AgentDrain>,
}

impl AgentManagementApplication {
  /// Erases Agent feature services behind their endpoint capabilities.
  pub fn new<PC, PQ, AC, AQ, E>(
    pool_commands: Arc<PC>,
    pool_queries: Arc<PQ>,
    agent_commands: Arc<AC>,
    agent_queries: Arc<AQ>,
    enrollments: Arc<E>,
  ) -> Self
  where
    PC: AuthorizedManagementCommandHandler<CreateAgentPoolCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<PublishAgentPoolVersionCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<DeleteAgentPoolCommand, Error = ApplicationError>
      + 'static,
    PQ: AuthorizedManagementQueryHandler<GetAgentPoolQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListAgentPoolsQuery, Error = ApplicationError>
      + 'static,
    AC: AuthorizedManagementCommandHandler<ReassignAgentPoolCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<DrainAgentCommand, Error = ApplicationError>
      + 'static,
    AQ: AuthorizedManagementQueryHandler<GetAgentQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListAgentsQuery, Error = ApplicationError>
      + 'static,
    E: AuthorizedManagementCommandHandler<IssueAgentEnrollmentCommand, Error = ApplicationError> + 'static,
  {
    Self {
      pools: AgentPoolManagementApplication {
        create: pool_commands.clone(),
        publish: pool_commands.clone(),
        delete: pool_commands,
        get: pool_queries.clone(),
        list: pool_queries,
      },
      agents: AgentEndpoints {
        issue_enrollment: enrollments,
        get: agent_queries.clone(),
        list: agent_queries,
        reassign: agent_commands.clone(),
        drain: agent_commands,
      },
    }
  }
}

/// Type-erased Build execution handlers consumed by REST.
pub struct BuildManagementApplication {
  pub(super) list: Arc<BuildList>,
  pub(super) get_build: Arc<BuildGet>,
  pub(super) get_attempt: Arc<AttemptGet>,
  pub(super) get_job: Arc<JobGet>,
  pub(super) cancel: Arc<BuildCancel>,
  pub(super) retry: Arc<BuildRetry>,
}

impl BuildManagementApplication {
  /// Erases one Build service behind its endpoint capabilities.
  pub fn new<C, Q>(commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<CancelBuildCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<RetryBuildCommand, Error = ApplicationError>
      + 'static,
    Q: AuthorizedManagementQueryHandler<GetBuildQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListProjectBuildsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetAttemptQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetJobQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      list: queries.clone(),
      get_build: queries.clone(),
      get_attempt: queries.clone(),
      get_job: queries,
      cancel: commands.clone(),
      retry: commands,
    }
  }
}

/// Type-erased policy and Trigger-definition handlers consumed by REST.
pub struct DefinitionManagementApplication {
  pub(super) publish_project_policy: Arc<ProjectPolicyPublish>,
  pub(super) create_trigger: Arc<TriggerDefinitionCreate>,
  pub(super) get_manual_trigger: Arc<ManualTriggerDefinitionGet>,
  pub(super) list_triggers: Arc<TriggerDefinitionList>,
  pub(super) create_unmanaged_webhook: Arc<UnmanagedWebhookCreate>,
  pub(super) create_managed_webhook: Arc<ManagedWebhookCreate>,
  pub(super) observe_managed_webhook: Arc<ManagedWebhookObserve>,
  pub(super) rotate_managed_webhook: Arc<ManagedWebhookRotate>,
  pub(super) delete_managed_webhook: Arc<ManagedWebhookDelete>,
}

/// Type-erased durable schedule management handlers consumed by REST.
pub struct ScheduleManagementApplication {
  pub(super) create: Arc<ScheduleCreate>,
  pub(super) get: Arc<ScheduleGet>,
}

/// Type-erased internal Trigger management handlers consumed by REST.
pub struct InternalTriggerManagementApplication {
  pub(super) create: Arc<InternalTriggerCreate>,
  pub(super) publish: Arc<InternalTriggerPublish>,
  pub(super) get: Arc<InternalTriggerGet>,
  pub(super) list: Arc<InternalTriggerList>,
}

impl InternalTriggerManagementApplication {
  /// Erases one internal Trigger service behind its command and query capabilities.
  pub fn new<C, Q>(commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<CreateInternalTriggerCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<PublishInternalTriggerVersionCommand, Error = ApplicationError>
      + 'static,
    Q: AuthorizedManagementQueryHandler<GetInternalTriggerQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListInternalTriggersQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      create: commands.clone(),
      publish: commands,
      get: queries.clone(),
      list: queries,
    }
  }
}

impl ScheduleManagementApplication {
  /// Erases one schedule service behind its command and query capabilities.
  pub fn new<C, Q>(commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<CreateScheduleCommand, Error = ApplicationError> + 'static,
    Q: AuthorizedManagementQueryHandler<GetScheduleQuery, Error = ApplicationError> + 'static,
  {
    Self {
      create: commands,
      get: queries,
    }
  }
}

impl DefinitionManagementApplication {
  /// Erases one definition service behind its endpoint capabilities.
  pub fn new<D, W, Q>(definitions: Arc<D>, webhooks: Arc<W>, queries: Arc<Q>) -> Self
  where
    D: AuthorizedManagementCommandHandler<PublishProjectPolicyCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<CreateTriggerDefinitionCommand, Error = ApplicationError>
      + 'static,
    W: AuthorizedManagementCommandHandler<CreateUnmanagedWebhookCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<CreateManagedWebhookCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<ObserveManagedWebhookRegistrationCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<RotateManagedWebhookRegistrationCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<DeleteManagedWebhookRegistrationCommand, Error = ApplicationError>
      + 'static,
    Q: AuthorizedManagementQueryHandler<ListProjectTriggerDefinitionsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetManualTriggerDefinitionQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      publish_project_policy: definitions.clone(),
      create_trigger: definitions,
      get_manual_trigger: queries.clone(),
      list_triggers: queries,
      create_unmanaged_webhook: webhooks.clone(),
      create_managed_webhook: webhooks.clone(),
      observe_managed_webhook: webhooks.clone(),
      rotate_managed_webhook: webhooks.clone(),
      delete_managed_webhook: webhooks,
    }
  }
}

/// Type-erased manual Trigger acceptance handler consumed by REST.
pub struct ManualTriggerManagementApplication(pub(super) Arc<ManualTriggerAccept>);

impl ManualTriggerManagementApplication {
  /// Erases one manual Trigger service behind its command capability.
  pub fn new<T>(service: Arc<T>) -> Self
  where
    T: AuthorizedManagementCommandHandler<AcceptManualTriggerCommand, Error = ManualTriggerError> + 'static,
  {
    Self(service)
  }
}

/// Type-erased Job-event query handler consumed by REST.
pub struct JobEventManagementApplication(pub(super) Arc<JobEventsRead>);

impl JobEventManagementApplication {
  /// Erases one Job-event query service behind its query capability.
  pub fn new<E>(service: Arc<E>) -> Self
  where
    E: AuthorizedManagementQueryHandler<ReadJobEventsQuery, Error = ApplicationError> + 'static,
  {
    Self(service)
  }
}

/// Type-erased logical Artifact query handlers consumed by REST.
pub struct ArtifactManagementApplication {
  pub(super) get: Arc<ArtifactGet>,
  pub(super) list: Arc<ArtifactList>,
  pub(super) download: Arc<ArtifactDownload>,
}

impl ArtifactManagementApplication {
  /// Erases one Artifact service behind its management query capabilities.
  pub fn new<A>(service: Arc<A>) -> Self
  where
    A: AuthorizedManagementQueryHandler<GetArtifactQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListBuildArtifactsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<AuthorizeArtifactDownloadQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      get: service.clone(),
      list: service.clone(),
      download: service,
    }
  }
}

/// Type-erased secret-free cache-session diagnostic queries consumed by REST.
pub struct CacheManagementApplication {
  pub(super) get: Arc<CacheSessionGet>,
  pub(super) list: Arc<CacheSessionList>,
}

/// Type-erased Build-log search query consumed by REST.
pub struct BuildLogSearchManagementApplication(pub(super) Arc<BuildLogsSearch>);

impl BuildLogSearchManagementApplication {
  /// Erases one backend-neutral Build-log search service behind its query capability.
  pub fn new<S>(service: Arc<S>) -> Self
  where
    S: AuthorizedManagementQueryHandler<SearchBuildLogsQuery, Error = BuildLogSearchError> + 'static,
  {
    Self(service)
  }
}

/// Type-erased read-only audit query consumed by REST.
pub struct AuditManagementApplication(pub(super) Arc<AuditFactList>);

impl AuditManagementApplication {
  /// Erases one audit service behind its read-only query capability.
  pub fn new<A>(service: Arc<A>) -> Self
  where
    A: AuthorizedManagementQueryHandler<ListAuditFactsQuery, Error = ApplicationError> + 'static,
  {
    Self(service)
  }
}

/// Type-erased whole-Build-Result retention query and command handlers.
pub struct BuildResultRetentionManagementApplication {
  pub(super) get: Arc<BuildRetentionGet>,
  pub(super) place: Arc<BuildRetentionPlace>,
  pub(super) release: Arc<BuildRetentionRelease>,
}

impl BuildResultRetentionManagementApplication {
  /// Erases one retention-hold service behind its complete management capabilities.
  pub fn new<C, Q>(commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<PlaceBuildResultHoldCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<ReleaseBuildResultHoldCommand, Error = ApplicationError>
      + 'static,
    Q: AuthorizedManagementQueryHandler<GetBuildResultRetentionQuery, Error = ApplicationError> + 'static,
  {
    Self {
      get: queries,
      place: commands.clone(),
      release: commands,
    }
  }
}

impl CacheManagementApplication {
  /// Erases one cache-session service behind management query capabilities.
  pub fn new<C>(service: Arc<C>) -> Self
  where
    C: AuthorizedManagementQueryHandler<GetCacheSessionQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListBuildCacheSessionsQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      get: service.clone(),
      list: service,
    }
  }
}

/// Catalog-oriented management handlers.
pub struct CatalogManagementApplication {
  pub(super) projects: ProjectManagementApplication,
  pub(super) pipelines: PipelineManagementApplication,
  pub(super) configurations: ConfigurationManagementApplication,
  pub(super) definitions: DefinitionManagementApplication,
  pub(super) schedules: ScheduleManagementApplication,
  pub(super) internal_triggers: InternalTriggerManagementApplication,
}

impl CatalogManagementApplication {
  /// Groups definition and catalog resources that establish immutable build input.
  pub fn new(
    projects: ProjectManagementApplication,
    pipelines: PipelineManagementApplication,
    configurations: ConfigurationManagementApplication,
    definitions: DefinitionManagementApplication,
    schedules: ScheduleManagementApplication,
    internal_triggers: InternalTriggerManagementApplication,
  ) -> Self {
    Self {
      projects,
      pipelines,
      configurations,
      definitions,
      schedules,
      internal_triggers,
    }
  }
}

/// Build execution and diagnostic management handlers.
pub struct ExecutionManagementApplication {
  pub(super) builds: BuildManagementApplication,
  pub(super) manual_triggers: ManualTriggerManagementApplication,
  pub(super) job_events: JobEventManagementApplication,
  pub(super) artifacts: ArtifactManagementApplication,
  pub(super) cache: CacheManagementApplication,
  pub(super) log_search: BuildLogSearchManagementApplication,
  pub(super) retention: BuildResultRetentionManagementApplication,
}

impl ExecutionManagementApplication {
  /// Groups handlers used after immutable build input has been defined.
  pub fn new(
    builds: BuildManagementApplication,
    manual_triggers: ManualTriggerManagementApplication,
    job_events: JobEventManagementApplication,
    artifacts: ArtifactManagementApplication,
    cache: CacheManagementApplication,
    log_search: BuildLogSearchManagementApplication,
    retention: BuildResultRetentionManagementApplication,
  ) -> Self {
    Self {
      builds,
      manual_triggers,
      job_events,
      artifacts,
      cache,
      log_search,
      retention,
    }
  }
}

/// Complete non-generic handler groups consumed by the management transport.
pub struct ManagementApplicationHandlers {
  pub(super) operational: OperationalMetadataManagementApplication,
  pub(super) catalog: CatalogManagementApplication,
  pub(super) agents: AgentManagementApplication,
  pub(super) execution: ExecutionManagementApplication,
  pub(super) audit: AuditManagementApplication,
}

impl ManagementApplicationHandlers {
  /// Groups independently type-erased feature boundaries.
  pub fn new(
    operational: OperationalMetadataManagementApplication,
    catalog: CatalogManagementApplication,
    agents: AgentManagementApplication,
    execution: ExecutionManagementApplication,
    audit: AuditManagementApplication,
  ) -> Self {
    Self {
      operational,
      catalog,
      agents,
      execution,
      audit,
    }
  }
}
