use std::sync::Arc;

use octacity_server_api_rest::{
  JobEventNotificationHub,
  v1::{
    AgentManagementApplication, ArtifactManagementApplication, AuditManagementApplication,
    BuildLogSearchManagementApplication, BuildManagementApplication, BuildResultRetentionManagementApplication,
    CacheManagementApplication, CatalogManagementApplication, ConfigurationManagementApplication,
    DefinitionManagementApplication, ExecutionManagementApplication, FactoryManagementApplication,
    InternalTriggerManagementApplication, JobEventManagementApplication, ManagementApplication,
    ManagementApplicationHandlers, ManualTriggerManagementApplication, OperationalMetadataManagementApplication,
    OperatorAttentionManagementApplication, PipelineManagementApplication, ProjectManagementApplication,
    ResourceSearchManagementApplication, ScheduleManagementApplication,
  },
};
use octacity_server_application::{
  AgentEnrollmentHandler, AgentHandlers, AgentPoolHandlers, ArtifactHandlers, AuditQueries, AuthorizedCommandHandler,
  AuthorizedQueryHandler, BuildConfigurationHandlers, BuildHandlers, BuildLogSearch, BuildResultRetentionHandlers,
  CacheSessionHandlers, DefinitionHandlers, DurableManualTriggerService, DurableRetryPolicy, InternalTriggerHandlers,
  JobEventLongPoll, JobSpecToolchainPolicy, ManagementAuthorizationPolicy, ManagementOperationalMetadataProjection,
  ManualTriggerRetryWorker, ManualTriggerService, OperationalMetadataQueries, OperatorAttentionHandlers,
  PipelineHandlers, ProjectHandlers, ResourceSearchHandlers, RevisionResolver, ScheduleHandlers,
  StoreBackedEffectiveProjectPolicySource, StoreBackedManualTriggerContext, TrustedNetworkManagementPolicy,
  WebhookDeliveryVerifier, WebhookIngressService, WebhookManagementProvider, WebhookManagementService,
};
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresLogSearchIndex, PostgresStore};
use octacity_server_webhook::WebhookAdapterRegistry;
use tokio_util::sync::CancellationToken;

use super::{
  ServerRuntimeError,
  webhook::{HostedWebhookVerifier, UnavailableWebhookVerifier},
};

// HTTP wait only; the shared intake retains its own original leases/deadlines.
const ISSUE_EXECUTION_PUBLICATION_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

pub(super) struct ApplicationComponents {
  pub(super) management: ManagementApplication,
  pub(super) trigger_service: Arc<ManualTriggerService>,
  pub(super) manual_trigger_retries: ManualTriggerRetryWorker,
  pub(super) webhook_ingress: Arc<WebhookIngressService>,
  pub(super) webhook_verifier: Arc<dyn WebhookDeliveryVerifier>,
  pub(super) webhook_provider: Arc<dyn WebhookManagementProvider>,
}

type WebhookHostConfiguration = (
  Arc<WebhookAdapterRegistry>,
  octacity_server_application::WebhookCallbackOrigin,
  std::time::Duration,
  std::time::Duration,
);

pub(super) struct ApplicationAssembly {
  pub(super) issue_integrations: super::IssueRuntimeIntegrations,
  pub(super) store: Arc<PostgresStore>,
  pub(super) authoritative_store: Arc<PostgresAuthoritativeStore>,
  pub(super) toolchain: JobSpecToolchainPolicy,
  pub(super) supported_pipeline_capabilities: Vec<String>,
  pub(super) agent_enrollment_lifetime: std::time::Duration,
  pub(super) agent_enrollment_secret_key: octacity_server_application::AgentEnrollmentSecretKey,
  pub(super) webhook_configuration: Option<WebhookHostConfiguration>,
  pub(super) revision_resolver: Arc<dyn RevisionResolver>,
  pub(super) cancellation: CancellationToken,
  pub(super) vcs_retry_policy: DurableRetryPolicy,
  pub(super) trigger_claim_lifetime: std::time::Duration,
  pub(super) artifacts: Arc<ArtifactHandlers<PostgresStore, octacity_artifact_s3::S3ArtifactStore>>,
  pub(super) object_storage: Arc<octacity_artifact_s3::S3ArtifactStore>,
  pub(super) cache: Arc<CacheSessionHandlers<PostgresStore>>,
  pub(super) log_search_index: Arc<PostgresLogSearchIndex>,
  pub(super) operational_metadata: ManagementOperationalMetadataProjection,
  pub(super) integrations: Option<octacity_server_api_rest::integrations::IntegrationManagementApplication>,
}

pub(super) fn management_application(
  assembly: ApplicationAssembly,
) -> Result<ApplicationComponents, ServerRuntimeError> {
  let ApplicationAssembly {
    issue_integrations,
    store,
    authoritative_store,
    toolchain,
    supported_pipeline_capabilities,
    agent_enrollment_lifetime,
    agent_enrollment_secret_key,
    webhook_configuration,
    revision_resolver,
    cancellation,
    vcs_retry_policy,
    trigger_claim_lifetime,
    artifacts,
    object_storage,
    cache,
    log_search_index,
    operational_metadata,
    integrations,
  } = assembly;
  let projects = Arc::new(ProjectHandlers::new(store.clone()));
  let pipelines = Arc::new(PipelineHandlers::new(store.clone()));
  let configurations = Arc::new(BuildConfigurationHandlers::new(store.clone()));
  let pools = Arc::new(AgentPoolHandlers::new(store.clone()));
  let agents = Arc::new(AgentHandlers::new(store.clone()));
  let enrollments = Arc::new(AgentEnrollmentHandler::new(store.clone()));
  let builds = Arc::new(BuildHandlers::new(authoritative_store.clone()));
  let definitions = Arc::new(DefinitionHandlers::new(store.clone()));
  let schedules = Arc::new(ScheduleHandlers::new(store.clone()));
  let internal_triggers = Arc::new(InternalTriggerHandlers::new(store.clone()));
  let policy_source = Arc::new(StoreBackedEffectiveProjectPolicySource::new(store.clone()));
  let trigger_context = Arc::new(StoreBackedManualTriggerContext::new(
    store.clone(),
    policy_source,
    toolchain,
  ));
  let manual_triggers = Arc::new(ManualTriggerService::new(
    authoritative_store.clone(),
    trigger_context,
    revision_resolver.clone(),
  ));
  let durable_manual_triggers = Arc::new(DurableManualTriggerService::new(
    manual_triggers.clone(),
    store.clone(),
    vcs_retry_policy,
    trigger_claim_lifetime,
  ));
  let manual_trigger_retries = ManualTriggerRetryWorker::new(manual_triggers.clone(), store.clone(), vcs_retry_policy);
  let (verifier, provider, callback_base): (
    Arc<dyn WebhookDeliveryVerifier>,
    Arc<dyn WebhookManagementProvider>,
    Option<octacity_server_application::WebhookCallbackOrigin>,
  ) = match webhook_configuration {
    Some((registry, callback_base, operation_timeout, cancellation_grace)) => {
      let hosted = Arc::new(HostedWebhookVerifier {
        registry,
        operation_timeout,
        cancellation_grace,
        cancellation: cancellation.child_token(),
      });
      (hosted.clone(), hosted, Some(callback_base))
    }
    None => {
      let unavailable = Arc::new(UnavailableWebhookVerifier);
      (unavailable.clone(), unavailable, None)
    }
  };
  let webhook_management = Arc::new(WebhookManagementService::new(
    store.clone(),
    store.clone(),
    store.clone(),
    provider.clone(),
    callback_base,
  ));
  let webhook_ingress = Arc::new(WebhookIngressService::new(store.clone(), store.clone()));
  let job_events = Arc::new(JobEventLongPoll::new(
    store.clone(),
    Arc::new(JobEventNotificationHub::default()),
  ));
  let retention = Arc::new(BuildResultRetentionHandlers::new(store.clone()));
  let audit = Arc::new(AuditQueries::new(store.clone()));
  let resource_search = Arc::new(ResourceSearchHandlers::new(store.clone()));
  let operator_attention = Arc::new(OperatorAttentionHandlers::new(store.clone()));
  let management_policy: Arc<dyn ManagementAuthorizationPolicy> = Arc::new(TrustedNetworkManagementPolicy);
  let workflows = Arc::new(octacity_server_application::IssueWorkflowHandlers::new(store.clone()));
  let workflow_commands = authorized_commands(&management_policy, workflows.clone());
  let workflow_queries = authorized_queries(&management_policy, workflows);
  let factory = Arc::new(
    octacity_server_application::FactoryConfigurationHandlers::with_policy(store.clone(), management_policy.clone())
      .with_issue_status_actions(workflow_queries.clone()),
  );
  let log_search = Arc::new(BuildLogSearch::new(store.clone(), log_search_index));
  let operational_metadata = Arc::new(OperationalMetadataQueries::new(operational_metadata));
  let project_commands = authorized_commands(&management_policy, projects.clone());
  let pipeline_commands = authorized_commands(&management_policy, pipelines.clone());
  let configuration_commands = authorized_commands(&management_policy, configurations.clone());
  let pool_commands = authorized_commands(&management_policy, pools.clone());
  let agent_commands = authorized_commands(&management_policy, agents.clone());
  let enrollment_commands = authorized_commands(&management_policy, enrollments);
  let build_commands = authorized_commands(&management_policy, builds.clone());
  let definition_commands = authorized_commands(&management_policy, definitions.clone());
  let schedule_commands = authorized_commands(&management_policy, schedules.clone());
  let internal_trigger_commands = authorized_commands(&management_policy, internal_triggers.clone());
  let webhook_commands = authorized_commands(&management_policy, webhook_management);
  let manual_trigger_commands = authorized_commands(&management_policy, durable_manual_triggers);
  let retention_commands = authorized_commands(&management_policy, retention.clone());
  let project_queries = authorized_queries(&management_policy, projects);
  let pipeline_queries = authorized_queries(&management_policy, pipelines);
  let configuration_queries = authorized_queries(&management_policy, configurations);
  let definition_queries = authorized_queries(&management_policy, definitions);
  let pool_queries = authorized_queries(&management_policy, pools);
  let agent_queries = authorized_queries(&management_policy, agents);
  let build_queries = authorized_queries(&management_policy, builds);
  let schedule_queries = authorized_queries(&management_policy, schedules);
  let internal_trigger_queries = authorized_queries(&management_policy, internal_triggers);
  let job_event_queries = authorized_queries(&management_policy, job_events.clone());
  let artifact_queries = authorized_queries(&management_policy, artifacts);
  let cache_queries = authorized_queries(&management_policy, cache);
  let log_search_queries = authorized_queries(&management_policy, log_search);
  let retention_queries = authorized_queries(&management_policy, retention);
  let audit_queries = authorized_queries(&management_policy, audit);
  let resource_search_queries = authorized_queries(&management_policy, resource_search);
  let operator_attention_queries = authorized_queries(&management_policy, operator_attention);
  let operational_queries = authorized_queries(&management_policy, operational_metadata);
  let flow_handlers = Arc::new(
    octacity_server_application::FactoryFlowDefinitionHandlers::new(store.clone(), management_policy.clone())
      .with_issue_status_actions(workflow_queries.clone()),
  );
  let flow_commands = authorized_commands(&management_policy, flow_handlers.clone());
  let flow_queries = authorized_queries(&management_policy, flow_handlers);
  let factory_admission = authorized_commands(
    &management_policy,
    Arc::new(octacity_server_application::FactoryAdmissionHandlers::with_policy(
      store.clone(),
      revision_resolver.clone(),
      management_policy.clone(),
    )),
  );
  let factory_discovery = authorized_queries(
    &management_policy,
    Arc::new(octacity_server_application::FactoryDiscoveryHandlers::with_policy(
      store.clone(),
      management_policy.clone(),
    )),
  );
  let factory_commands = authorized_commands(&management_policy, factory.clone());
  let factory_control = authorized_commands(
    &management_policy,
    Arc::new(octacity_server_application::FactoryOperatorHandlers::new(
      store.clone(),
      management_policy.clone(),
    )),
  );
  let factory_queries = authorized_queries(&management_policy, factory);
  let factory_actions = authorized_queries(
    &management_policy,
    Arc::new(
      octacity_server_application::FactoryActionHandlers::new(
        store.clone(),
        management_policy.clone(),
        Arc::new(octacity_server_application::SystemFactoryExecutionClock),
      )
      .with_issue_associations(store.clone()),
    ),
  );
  let issues = Arc::new(octacity_server_application::IssueHandlers::new(
    store.clone(),
    management_policy.clone(),
  ));
  let issue_commands = authorized_commands(&management_policy, issues.clone());
  let issue_queries = authorized_queries(&management_policy, issues);
  let discussion = Arc::new(octacity_server_application::IssueDiscussionHandlers::new(
    store.clone(),
    management_policy.clone(),
    Arc::new(octacity_server_application::SystemFactoryExecutionClock),
  ));
  let executions = Arc::new(octacity_server_application::IssueExecutionHandlers::new(
    store.clone(),
    store.clone(),
    management_policy.clone(),
  ));
  let result_artifacts = Arc::new(octacity_server_application::IssueArtifactHandlers::new(
    executions.clone(),
    Arc::new(
      octacity_server_application::FactoryArtifactReader::new(
        store.clone(),
        authoritative_store,
        object_storage.clone(),
      )
      .with_project_inputs(store.clone())
      .with_source_inputs(store.clone()),
    ),
    Arc::new(octacity_server_application::SystemFactoryExecutionClock),
    management_policy.clone(),
  ));
  let execution_publication = Arc::new(octacity_server_application::IssueExecutionPublisher::new(
    store.clone(),
    store.clone(),
    object_storage,
    revision_resolver,
    management_policy.clone(),
    Arc::new(octacity_server_application::SystemFactoryExecutionClock),
  ));
  let super::IssueRuntimeIntegrations {
    sources: issue_sources,
    code_reviews,
  } = issue_integrations;
  let reviews = Arc::new(
    octacity_server_application::IssueCodeReviewHandlers::new(
      store.clone(),
      code_reviews.unwrap_or_else(|| Arc::new(UnavailableIssueCodeReviews)),
      management_policy.clone(),
      Arc::new(octacity_server_application::SystemFactoryExecutionClock),
    )
    .with_execution_reader(executions.clone()),
  );
  let (reader, mappings, refresh_policy) = match issue_sources {
    Some(super::IssueSourceRuntimeInstallation {
      reader,
      mappings,
      refresh_policy,
    }) => (reader, mappings, Some(refresh_policy)),
    None => (
      Arc::new(UnavailableIssueSource) as Arc<dyn octacity_server_application::WorkSourceObservations>,
      vec![],
      None,
    ),
  };
  let sources = octacity_server_application::IssueSourceHandlers::new(
    store.clone(),
    reader,
    management_policy.clone(),
    Arc::new(octacity_server_application::SystemFactoryExecutionClock),
  )
  .with_source_mappings(mappings)
  .map_err(|_| ServerRuntimeError::InvalidManagementPolicy)?;
  let sources = Arc::new(match refresh_policy {
    Some(policy) => sources
      .with_refresh_policy(policy)
      .map_err(|_| ServerRuntimeError::InvalidManagementPolicy)?,
    None => sources,
  });
  let application = ManagementApplication::new(
    supported_pipeline_capabilities,
    agent_enrollment_lifetime,
    agent_enrollment_secret_key,
    ManagementApplicationHandlers::new(
      OperationalMetadataManagementApplication::new(operational_queries),
      CatalogManagementApplication::new(
        ProjectManagementApplication::new(project_commands, project_queries),
        PipelineManagementApplication::new(pipeline_commands, pipeline_queries),
        ConfigurationManagementApplication::new(configuration_commands, configuration_queries),
        DefinitionManagementApplication::new(definition_commands, webhook_commands, definition_queries),
        ScheduleManagementApplication::new(schedule_commands, schedule_queries),
        InternalTriggerManagementApplication::new(internal_trigger_commands, internal_trigger_queries),
        FactoryManagementApplication::new(
          factory_commands,
          factory_queries,
          flow_commands,
          flow_queries,
          factory_admission,
          factory_discovery,
          factory_control,
        ),
      ),
      AgentManagementApplication::new(
        pool_commands,
        pool_queries,
        agent_commands,
        agent_queries,
        enrollment_commands,
      ),
      ExecutionManagementApplication::new(
        BuildManagementApplication::new(build_commands, build_queries),
        ManualTriggerManagementApplication::new(manual_trigger_commands),
        JobEventManagementApplication::new(job_event_queries),
        ArtifactManagementApplication::new(artifact_queries),
        CacheManagementApplication::new(cache_queries),
        BuildLogSearchManagementApplication::new(log_search_queries),
        BuildResultRetentionManagementApplication::new(retention_commands, retention_queries),
      ),
      AuditManagementApplication::new(audit_queries),
      ResourceSearchManagementApplication::new(resource_search_queries),
      OperatorAttentionManagementApplication::new(operator_attention_queries),
    ),
  )
  .map_err(|_| ServerRuntimeError::InvalidManagementPolicy)?
  .with_factory_actions(
    octacity_server_api_rest::v1::FactoryActionManagementApplication::new(factory_actions.clone())
      .with_observations(factory_actions),
  )
  .with_issue_workflows(octacity_server_api_rest::v1::IssueWorkflowManagementApplication::new(
    workflow_commands,
    workflow_queries,
  ))
  .with_issue_result_panels(
    octacity_server_api_rest::v1::IssueResultPanelManagementApplication::new(
      authorized_queries(&management_policy, executions.clone()),
      authorized_queries(&management_policy, result_artifacts),
    ),
  )
  .with_issue_activity(
    octacity_server_api_rest::v1::IssueActivityManagementApplication::new(authorized_queries(
      &management_policy,
      Arc::new(octacity_server_application::IssueActivityHandlers::new(
        store.clone(),
        Arc::new(octacity_server_application::SystemFactoryExecutionClock),
      )),
    ))
    .with_results(authorized_queries(&management_policy, executions.clone())),
  )
  .with_issue_discussion(octacity_server_api_rest::v1::IssueDiscussionManagementApplication::new(
    authorized_commands(&management_policy, discussion.clone()),
    authorized_queries(&management_policy, discussion),
  ))
  .with_issue_executions(
    octacity_server_api_rest::v1::IssueExecutionManagementApplication::new(authorized_queries(
      &management_policy,
      executions.clone(),
    ))
    .with_journal(
      authorized_queries(&management_policy, executions.clone()),
      authorized_queries(
        &management_policy,
        Arc::new(octacity_server_application::IssueExecutionJournalHandlers::new(
          executions.clone(),
          store.clone(),
          job_events,
        )),
      ),
    )
    .with_publication(
      authorized_commands(&management_policy, executions),
      authorized_commands(&management_policy, execution_publication),
      ISSUE_EXECUTION_PUBLICATION_WAIT,
    ),
  )
  .with_issue_code_reviews(
    octacity_server_api_rest::v1::IssueCodeReviewManagementApplication::new(
      authorized_commands(&management_policy, reviews.clone()),
      authorized_queries(&management_policy, reviews.clone()),
    )
    .with_checks(authorized_queries(&management_policy, reviews)),
  )
  .with_issue_sources(
    octacity_server_api_rest::v1::IssueSourceManagementApplication::new(authorized_queries(
      &management_policy,
      sources.clone(),
    ))
    .with_commands(authorized_commands(&management_policy, sources.clone()))
    .with_resolution(authorized_commands(&management_policy, sources))
    .with_projection_queries(authorized_queries(
      &management_policy,
      Arc::new(octacity_server_application::IssueSynchronizationHandlers::new(
        store.clone(),
        Arc::new(octacity_server_application::SystemFactoryExecutionClock),
      )),
    )),
  )
  .with_issues(octacity_server_api_rest::v1::IssueManagementApplication::new(
    issue_commands,
    issue_queries,
  ));
  let application = match integrations {
    Some(integrations) => application.with_integrations(integrations),
    None => application,
  };
  Ok(ApplicationComponents {
    management: application,
    trigger_service: manual_triggers,
    manual_trigger_retries,
    webhook_ingress,
    webhook_verifier: verifier,
    webhook_provider: provider,
  })
}

fn authorized_commands<H>(
  policy: &Arc<dyn ManagementAuthorizationPolicy>,
  handler: Arc<H>,
) -> Arc<AuthorizedCommandHandler<H>> {
  Arc::new(AuthorizedCommandHandler::new(policy.clone(), handler))
}

fn authorized_queries<H>(
  policy: &Arc<dyn ManagementAuthorizationPolicy>,
  handler: Arc<H>,
) -> Arc<AuthorizedQueryHandler<H>> {
  Arc::new(AuthorizedQueryHandler::new(policy.clone(), handler))
}

// Default task composition retains metadata and local pull control without installing a provider.
struct UnavailableIssueSource;
#[async_trait::async_trait]
impl octacity_server_application::WorkSourceObservations for UnavailableIssueSource {
  async fn read(
    &self,
    _: &octacity_server_application::ManagementRequestContext,
    _: octacity_server_application::ReadWorkSourceObservations,
  ) -> Result<octacity_server_application::WorkSourceObservationPage, octacity_server_application::ApplicationError> {
    Err(octacity_server_application::ApplicationError::capability_unavailable())
  }
}

struct UnavailableIssueCodeReviews;
#[async_trait::async_trait]
impl octacity_server_application::IssueCodeReviewObservations for UnavailableIssueCodeReviews {
  async fn visibility(
    &self,
    _: &octacity_server_application::ManagementRequestContext,
    _: octacity_server_domain::ProjectId,
  ) -> Result<
    octacity_server_application::IssueCodeReviewRepositoryVisibility,
    octacity_server_application::ApplicationError,
  > {
    Ok(octacity_server_application::IssueCodeReviewRepositoryVisibility::none())
  }
  async fn authorize(
    &self,
    _: &octacity_server_application::ManagementRequestContext,
    _: octacity_server_domain::ProjectId,
    _: &octacity_server_application::IssueCodeReviewSelection,
  ) -> Result<(), octacity_server_application::ApplicationError> {
    Err(octacity_server_application::ApplicationError::capability_unavailable())
  }
  async fn read(
    &self,
    _: &octacity_server_application::ManagementRequestContext,
    _: octacity_server_domain::ProjectId,
    _: &octacity_server_application::IssueCodeReviewSelection,
  ) -> Result<octacity_server_application::IssueCodeReviewObservation, octacity_server_application::ApplicationError>
  {
    Err(octacity_server_application::ApplicationError::capability_unavailable())
  }
}
