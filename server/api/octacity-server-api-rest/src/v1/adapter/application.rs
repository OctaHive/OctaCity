use std::sync::Arc;

use octacity_server_application::{
  AcceptManualTriggerCommand, ApplicationError, CancelBuildCommand, CommandHandler, CreateAgentPoolCommand,
  CreateBuildConfigurationCommand, CreateManagedWebhookCommand, CreatePipelineCommand, CreateProjectCommand,
  CreateRepositoryCommand, CreateScheduleCommand, CreateTriggerDefinitionCommand, CreateUnmanagedWebhookCommand,
  DeleteAgentPoolCommand, DeleteProjectCommand, DrainAgentCommand, GetAgentPoolQuery, GetAgentQuery, GetAttemptQuery,
  GetBuildConfigurationQuery, GetBuildQuery, GetJobQuery, GetPipelineQuery, GetProjectQuery, GetRepositoryQuery,
  GetScheduleQuery, IssueAgentEnrollmentCommand, ListAgentPoolsQuery, ListAgentsQuery, ListProjectsQuery,
  ManageWebhookRegistrationCommand, ManualTriggerError, MoveProjectCommand, PublishAgentPoolVersionCommand,
  PublishBuildConfigurationVersionCommand, PublishPipelineVersionCommand, PublishProjectPolicyCommand,
  PublishRepositoryVersionCommand, QueryHandler, ReadJobEventsQuery, ReassignAgentPoolCommand, RenameProjectCommand,
  RetryBuildCommand,
};

type ProjectCreate = dyn CommandHandler<CreateProjectCommand, Error = ApplicationError>;
type ProjectRename = dyn CommandHandler<RenameProjectCommand, Error = ApplicationError>;
type ProjectMove = dyn CommandHandler<MoveProjectCommand, Error = ApplicationError>;
type ProjectDelete = dyn CommandHandler<DeleteProjectCommand, Error = ApplicationError>;
type ProjectGet = dyn QueryHandler<GetProjectQuery, Error = ApplicationError>;
type ProjectList = dyn QueryHandler<ListProjectsQuery, Error = ApplicationError>;
type PipelineCreate = dyn CommandHandler<CreatePipelineCommand, Error = ApplicationError>;
type PipelinePublish = dyn CommandHandler<PublishPipelineVersionCommand, Error = ApplicationError>;
type PipelineGet = dyn QueryHandler<GetPipelineQuery, Error = ApplicationError>;
type RepositoryCreate = dyn CommandHandler<CreateRepositoryCommand, Error = ApplicationError>;
type RepositoryPublish = dyn CommandHandler<PublishRepositoryVersionCommand, Error = ApplicationError>;
type RepositoryGet = dyn QueryHandler<GetRepositoryQuery, Error = ApplicationError>;
type ConfigurationCreate = dyn CommandHandler<CreateBuildConfigurationCommand, Error = ApplicationError>;
type ConfigurationPublish = dyn CommandHandler<PublishBuildConfigurationVersionCommand, Error = ApplicationError>;
type ConfigurationGet = dyn QueryHandler<GetBuildConfigurationQuery, Error = ApplicationError>;
type AgentPoolCreate = dyn CommandHandler<CreateAgentPoolCommand, Error = ApplicationError>;
type AgentPoolPublish = dyn CommandHandler<PublishAgentPoolVersionCommand, Error = ApplicationError>;
type AgentPoolDelete = dyn CommandHandler<DeleteAgentPoolCommand, Error = ApplicationError>;
type AgentPoolGet = dyn QueryHandler<GetAgentPoolQuery, Error = ApplicationError>;
type AgentPoolList = dyn QueryHandler<ListAgentPoolsQuery, Error = ApplicationError>;
type AgentGet = dyn QueryHandler<GetAgentQuery, Error = ApplicationError>;
type AgentList = dyn QueryHandler<ListAgentsQuery, Error = ApplicationError>;
type AgentReassign = dyn CommandHandler<ReassignAgentPoolCommand, Error = ApplicationError>;
type AgentDrain = dyn CommandHandler<DrainAgentCommand, Error = ApplicationError>;
type AgentEnrollmentIssue = dyn CommandHandler<IssueAgentEnrollmentCommand, Error = ApplicationError>;
type BuildGet = dyn QueryHandler<GetBuildQuery, Error = ApplicationError>;
type AttemptGet = dyn QueryHandler<GetAttemptQuery, Error = ApplicationError>;
type JobGet = dyn QueryHandler<GetJobQuery, Error = ApplicationError>;
type BuildCancel = dyn CommandHandler<CancelBuildCommand, Error = ApplicationError>;
type BuildRetry = dyn CommandHandler<RetryBuildCommand, Error = ApplicationError>;
type ProjectPolicyPublish = dyn CommandHandler<PublishProjectPolicyCommand, Error = ApplicationError>;
type TriggerDefinitionCreate = dyn CommandHandler<CreateTriggerDefinitionCommand, Error = ApplicationError>;
type UnmanagedWebhookCreate = dyn CommandHandler<CreateUnmanagedWebhookCommand, Error = ApplicationError>;
type ManagedWebhookCreate = dyn CommandHandler<CreateManagedWebhookCommand, Error = ApplicationError>;
type ManagedWebhookManage = dyn CommandHandler<ManageWebhookRegistrationCommand, Error = ApplicationError>;
type ManualTriggerAccept = dyn CommandHandler<AcceptManualTriggerCommand, Error = ManualTriggerError>;
type JobEventsRead = dyn QueryHandler<ReadJobEventsQuery, Error = ApplicationError>;
type ScheduleCreate = dyn CommandHandler<CreateScheduleCommand, Error = ApplicationError>;
type ScheduleGet = dyn QueryHandler<GetScheduleQuery, Error = ApplicationError>;

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
  pub fn new<P>(service: Arc<P>) -> Self
  where
    P: CommandHandler<CreateProjectCommand, Error = ApplicationError>
      + CommandHandler<RenameProjectCommand, Error = ApplicationError>
      + CommandHandler<MoveProjectCommand, Error = ApplicationError>
      + CommandHandler<DeleteProjectCommand, Error = ApplicationError>
      + QueryHandler<GetProjectQuery, Error = ApplicationError>
      + QueryHandler<ListProjectsQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      create: service.clone(),
      rename: service.clone(),
      move_project: service.clone(),
      delete: service.clone(),
      get: service.clone(),
      list: service,
    }
  }
}

/// Type-erased Pipeline handlers consumed by REST.
pub struct PipelineManagementApplication {
  pub(super) create: Arc<PipelineCreate>,
  pub(super) publish: Arc<PipelinePublish>,
  pub(super) get: Arc<PipelineGet>,
}

impl PipelineManagementApplication {
  /// Erases one Pipeline service behind its endpoint capabilities.
  pub fn new<P>(service: Arc<P>) -> Self
  where
    P: CommandHandler<CreatePipelineCommand, Error = ApplicationError>
      + CommandHandler<PublishPipelineVersionCommand, Error = ApplicationError>
      + QueryHandler<GetPipelineQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      create: service.clone(),
      publish: service.clone(),
      get: service,
    }
  }
}

/// Type-erased Repository and Build Configuration handlers consumed by REST.
pub struct ConfigurationManagementApplication {
  pub(super) create_repository: Arc<RepositoryCreate>,
  pub(super) publish_repository: Arc<RepositoryPublish>,
  pub(super) get_repository: Arc<RepositoryGet>,
  pub(super) create_configuration: Arc<ConfigurationCreate>,
  pub(super) publish_configuration: Arc<ConfigurationPublish>,
  pub(super) get_configuration: Arc<ConfigurationGet>,
}

impl ConfigurationManagementApplication {
  /// Erases one configuration service behind its endpoint capabilities.
  pub fn new<C>(service: Arc<C>) -> Self
  where
    C: CommandHandler<CreateRepositoryCommand, Error = ApplicationError>
      + CommandHandler<PublishRepositoryVersionCommand, Error = ApplicationError>
      + QueryHandler<GetRepositoryQuery, Error = ApplicationError>
      + CommandHandler<CreateBuildConfigurationCommand, Error = ApplicationError>
      + CommandHandler<PublishBuildConfigurationVersionCommand, Error = ApplicationError>
      + QueryHandler<GetBuildConfigurationQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      create_repository: service.clone(),
      publish_repository: service.clone(),
      get_repository: service.clone(),
      create_configuration: service.clone(),
      publish_configuration: service.clone(),
      get_configuration: service,
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
  pub fn new<P, A, E>(pools: Arc<P>, agents: Arc<A>, enrollments: Arc<E>) -> Self
  where
    P: CommandHandler<CreateAgentPoolCommand, Error = ApplicationError>
      + CommandHandler<PublishAgentPoolVersionCommand, Error = ApplicationError>
      + CommandHandler<DeleteAgentPoolCommand, Error = ApplicationError>
      + QueryHandler<GetAgentPoolQuery, Error = ApplicationError>
      + QueryHandler<ListAgentPoolsQuery, Error = ApplicationError>
      + 'static,
    A: CommandHandler<ReassignAgentPoolCommand, Error = ApplicationError>
      + CommandHandler<DrainAgentCommand, Error = ApplicationError>
      + QueryHandler<GetAgentQuery, Error = ApplicationError>
      + QueryHandler<ListAgentsQuery, Error = ApplicationError>
      + 'static,
    E: CommandHandler<IssueAgentEnrollmentCommand, Error = ApplicationError> + 'static,
  {
    Self {
      pools: AgentPoolManagementApplication {
        create: pools.clone(),
        publish: pools.clone(),
        delete: pools.clone(),
        get: pools.clone(),
        list: pools,
      },
      agents: AgentEndpoints {
        issue_enrollment: enrollments,
        get: agents.clone(),
        list: agents.clone(),
        reassign: agents.clone(),
        drain: agents,
      },
    }
  }
}

/// Type-erased Build execution handlers consumed by REST.
pub struct BuildManagementApplication {
  pub(super) get_build: Arc<BuildGet>,
  pub(super) get_attempt: Arc<AttemptGet>,
  pub(super) get_job: Arc<JobGet>,
  pub(super) cancel: Arc<BuildCancel>,
  pub(super) retry: Arc<BuildRetry>,
}

impl BuildManagementApplication {
  /// Erases one Build service behind its endpoint capabilities.
  pub fn new<B>(service: Arc<B>) -> Self
  where
    B: QueryHandler<GetBuildQuery, Error = ApplicationError>
      + QueryHandler<GetAttemptQuery, Error = ApplicationError>
      + QueryHandler<GetJobQuery, Error = ApplicationError>
      + CommandHandler<CancelBuildCommand, Error = ApplicationError>
      + CommandHandler<RetryBuildCommand, Error = ApplicationError>
      + 'static,
  {
    Self {
      get_build: service.clone(),
      get_attempt: service.clone(),
      get_job: service.clone(),
      cancel: service.clone(),
      retry: service,
    }
  }
}

/// Type-erased policy and Trigger-definition handlers consumed by REST.
pub struct DefinitionManagementApplication {
  pub(super) publish_project_policy: Arc<ProjectPolicyPublish>,
  pub(super) create_trigger: Arc<TriggerDefinitionCreate>,
  pub(super) create_unmanaged_webhook: Arc<UnmanagedWebhookCreate>,
  pub(super) create_managed_webhook: Arc<ManagedWebhookCreate>,
  pub(super) manage_webhook_registration: Arc<ManagedWebhookManage>,
}

/// Type-erased durable schedule management handlers consumed by REST.
pub struct ScheduleManagementApplication {
  pub(super) create: Arc<ScheduleCreate>,
  pub(super) get: Arc<ScheduleGet>,
}

impl ScheduleManagementApplication {
  /// Erases one schedule service behind its command and query capabilities.
  pub fn new<S>(service: Arc<S>) -> Self
  where
    S: CommandHandler<CreateScheduleCommand, Error = ApplicationError>
      + QueryHandler<GetScheduleQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      create: service.clone(),
      get: service,
    }
  }
}

impl DefinitionManagementApplication {
  /// Erases one definition service behind its endpoint capabilities.
  pub fn new<D, W>(service: Arc<D>, webhooks: Arc<W>) -> Self
  where
    D: CommandHandler<PublishProjectPolicyCommand, Error = ApplicationError>
      + CommandHandler<CreateTriggerDefinitionCommand, Error = ApplicationError>
      + 'static,
    W: CommandHandler<CreateUnmanagedWebhookCommand, Error = ApplicationError>
      + CommandHandler<CreateManagedWebhookCommand, Error = ApplicationError>
      + CommandHandler<ManageWebhookRegistrationCommand, Error = ApplicationError>
      + 'static,
  {
    Self {
      publish_project_policy: service.clone(),
      create_trigger: service,
      create_unmanaged_webhook: webhooks.clone(),
      create_managed_webhook: webhooks.clone(),
      manage_webhook_registration: webhooks,
    }
  }
}

/// Type-erased manual Trigger acceptance handler consumed by REST.
pub struct ManualTriggerManagementApplication(pub(super) Arc<ManualTriggerAccept>);

impl ManualTriggerManagementApplication {
  /// Erases one manual Trigger service behind its command capability.
  pub fn new<T>(service: Arc<T>) -> Self
  where
    T: CommandHandler<AcceptManualTriggerCommand, Error = ManualTriggerError> + 'static,
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
    E: QueryHandler<ReadJobEventsQuery, Error = ApplicationError> + 'static,
  {
    Self(service)
  }
}

/// Catalog-oriented management handlers.
pub struct CatalogManagementApplication {
  pub(super) projects: ProjectManagementApplication,
  pub(super) pipelines: PipelineManagementApplication,
  pub(super) configurations: ConfigurationManagementApplication,
  pub(super) definitions: DefinitionManagementApplication,
  pub(super) schedules: ScheduleManagementApplication,
}

impl CatalogManagementApplication {
  /// Groups definition and catalog resources that establish immutable build input.
  pub fn new(
    projects: ProjectManagementApplication,
    pipelines: PipelineManagementApplication,
    configurations: ConfigurationManagementApplication,
    definitions: DefinitionManagementApplication,
    schedules: ScheduleManagementApplication,
  ) -> Self {
    Self {
      projects,
      pipelines,
      configurations,
      definitions,
      schedules,
    }
  }
}

/// Build execution and diagnostic management handlers.
pub struct ExecutionManagementApplication {
  pub(super) builds: BuildManagementApplication,
  pub(super) manual_triggers: ManualTriggerManagementApplication,
  pub(super) job_events: JobEventManagementApplication,
}

impl ExecutionManagementApplication {
  /// Groups handlers used after immutable build input has been defined.
  pub fn new(
    builds: BuildManagementApplication,
    manual_triggers: ManualTriggerManagementApplication,
    job_events: JobEventManagementApplication,
  ) -> Self {
    Self {
      builds,
      manual_triggers,
      job_events,
    }
  }
}

/// Complete non-generic handler groups consumed by the management transport.
pub struct ManagementApplicationHandlers {
  pub(super) catalog: CatalogManagementApplication,
  pub(super) agents: AgentManagementApplication,
  pub(super) execution: ExecutionManagementApplication,
}

impl ManagementApplicationHandlers {
  /// Groups independently type-erased feature boundaries.
  pub fn new(
    catalog: CatalogManagementApplication,
    agents: AgentManagementApplication,
    execution: ExecutionManagementApplication,
  ) -> Self {
    Self {
      catalog,
      agents,
      execution,
    }
  }
}
