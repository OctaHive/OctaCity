use super::*;

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
  pub(super) cache: Arc<CacheSessionHandlers<PostgresStore>>,
}

pub(super) fn management_application(
  assembly: ApplicationAssembly,
) -> Result<ApplicationComponents, ServerRuntimeError> {
  let ApplicationAssembly {
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
    cache,
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
  let policy_source = Arc::new(StoreBackedEffectiveProjectPolicySource::new(store.clone()));
  let trigger_context = Arc::new(StoreBackedManualTriggerContext::new(
    store.clone(),
    policy_source,
    toolchain,
  ));
  let manual_triggers = Arc::new(ManualTriggerService::new(
    authoritative_store,
    trigger_context,
    revision_resolver,
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
    store,
    Arc::new(JobEventNotificationHub::default()),
  ));
  let application = ManagementApplication::new(
    supported_pipeline_capabilities,
    agent_enrollment_lifetime,
    agent_enrollment_secret_key,
    ManagementApplicationHandlers::new(
      CatalogManagementApplication::new(
        ProjectManagementApplication::new(projects),
        PipelineManagementApplication::new(pipelines),
        ConfigurationManagementApplication::new(configurations),
        DefinitionManagementApplication::new(definitions, webhook_management),
        ScheduleManagementApplication::new(schedules),
      ),
      AgentManagementApplication::new(pools, agents, enrollments),
      ExecutionManagementApplication::new(
        BuildManagementApplication::new(builds),
        ManualTriggerManagementApplication::new(durable_manual_triggers),
        JobEventManagementApplication::new(job_events),
        ArtifactManagementApplication::new(artifacts),
        CacheManagementApplication::new(cache),
      ),
    ),
  )
  .map_err(|_| ServerRuntimeError::InvalidManagementPolicy)?;
  Ok(ApplicationComponents {
    management: application,
    trigger_service: manual_triggers,
    manual_trigger_retries,
    webhook_ingress,
    webhook_verifier: verifier,
    webhook_provider: provider,
  })
}
