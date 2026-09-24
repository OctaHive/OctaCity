use std::sync::Arc;

use octacity_server_api_agent::{AgentApiConfig, AgentRouterDependencies, ReadyJobNotificationHub, agent_router};
use octacity_server_api_cache::{CacheDataPlaneApplication, cache_router};
use octacity_server_api_webhook::{WebhookApplication, webhook_router};
use octacity_server_application::{
  AgentExecutionService, AgentHeartbeatService, AgentLeaseService, AgentRegistrationService, ArtifactHandlers,
  CacheDataPlaneService, CacheSessionHandlers, DurableRetryPolicy, InternalTriggerWorker, LeaseExpiryWorker,
  ManagedWebhookRegistrationWorker, RevisionResolver, ScheduleWorker, WebhookDeliveryWorker,
};
use octacity_server_store::WorkerOwner;
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
use octacity_server_vcs::VcsAdapterRegistry;
use octacity_server_webhook::WebhookAdapterRegistry;
use tokio_util::sync::CancellationToken;

use super::{
  ServerRuntime, ServerRuntimeError,
  application::{ApplicationAssembly, ApplicationComponents, management_application},
  assembly::RuntimeResources,
  listeners::RuntimeComponents,
  vcs::HostedRevisionResolver,
  workers::{
    DurableWorkers, EXPIRY_WORKER_NAME, INTERNAL_TRIGGER_WORKER_NAME, MANAGED_WEBHOOK_WORKER_NAME,
    MANUAL_TRIGGER_RETRY_WORKER_NAME, SCHEDULE_WORKER_NAME, WEBHOOK_DELIVERY_WORKER_NAME,
  },
};
use crate::{ServerConfig, readiness::WorkerHealth};

impl ServerRuntime {
  /// Binds the listener and starts supervised work from validated configuration.
  pub async fn start(config: ServerConfig) -> Result<Self, ServerRuntimeError> {
    let cancellation = CancellationToken::new();
    let mut resources = RuntimeResources::from_config(&config)
      .await
      .map_err(ServerRuntimeError::Assembly)?;
    let registration_store = Arc::new(PostgresStore::new(resources.postgres.clone()));
    let notification_pool = resources.postgres.clone();
    let registration_service = Arc::new(
      AgentRegistrationService::new(registration_store.clone(), config.agent_registration_lifetime())
        .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
    );
    let artifact_service = Arc::new(
      ArtifactHandlers::new(
        registration_service.clone(),
        registration_store.clone(),
        resources.object_storage.clone(),
        config.object_storage().upload_capability_lifetime(),
        config.object_storage().download_capability_lifetime(),
      )
      .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
    );
    let cache_service = Arc::new(
      CacheSessionHandlers::new(
        registration_service.clone(),
        registration_store.clone(),
        resources.cache_credential_key,
        config.cache().endpoint.clone(),
        config.cache().session_lifetime(),
      )
      .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
    );
    let cache_data_plane = Arc::new(
      CacheDataPlaneService::new(
        registration_store.clone(),
        resources.object_storage.clone(),
        config.cache().max_blob_bytes(),
      )
      .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
    );
    let placement_store = Arc::new(PostgresAuthoritativeStore::new(
      resources.postgres,
      resources.job_spec_signer,
    ));
    let webhook_configuration = match (config.webhook_adapter_registry(), config.webhook_callback_origin()) {
      (Some(registry), Some(callback_origin)) => Some((
        Arc::new(WebhookAdapterRegistry::discover(registry).map_err(ServerRuntimeError::WebhookRegistry)?),
        callback_origin,
        config.webhook_operation_timeout(),
        config.webhook_cancellation_grace(),
      )),
      (None, None) => None,
      _ => unreachable!("validated webhook configuration is all-or-none"),
    };
    let revision_resolver: Arc<dyn RevisionResolver> = match config.vcs_adapter_registry() {
      Some(registry) => Arc::new(HostedRevisionResolver::new(
        Arc::new(VcsAdapterRegistry::discover(registry).map_err(ServerRuntimeError::VcsRegistry)?),
        config.vcs_integrations(),
        config.vcs_operation_timeout(),
        config.vcs_cancellation_grace(),
        cancellation.child_token(),
      )),
      None => Arc::new(octacity_server_application::ExactRevisionResolver),
    };
    let webhook_retry_policy = DurableRetryPolicy::new(
      config.webhook_worker_max_attempts(),
      config.webhook_worker_initial_retry_milliseconds(),
      config.webhook_worker_maximum_retry_milliseconds(),
    )
    .map_err(|_| ServerRuntimeError::InvalidManagementPolicy)?;
    let vcs_retry_policy = DurableRetryPolicy::new(
      config.vcs_retry_max_attempts(),
      config.vcs_retry_initial_milliseconds(),
      config.vcs_retry_maximum_milliseconds(),
    )
    .map_err(|_| ServerRuntimeError::InvalidManagementPolicy)?;
    let ApplicationComponents {
      management: management_application,
      trigger_service,
      manual_trigger_retries,
      webhook_ingress,
      webhook_verifier,
      webhook_provider,
    } = management_application(ApplicationAssembly {
      store: registration_store.clone(),
      authoritative_store: placement_store.clone(),
      toolchain: resources.job_spec_toolchain,
      supported_pipeline_capabilities: config.supported_pipeline_capabilities().to_vec(),
      agent_enrollment_lifetime: config.agent_enrollment_lifetime(),
      agent_enrollment_secret_key: resources.agent_enrollment_secret_key,
      webhook_configuration,
      revision_resolver,
      cancellation: cancellation.child_token(),
      vcs_retry_policy,
      trigger_claim_lifetime: config.trigger_evaluation_claim_lifetime(),
      artifacts: artifact_service.clone(),
      cache: cache_service.clone(),
    })?;
    let worker_health = Arc::new(WorkerHealth::new(
      EXPIRY_WORKER_NAME,
      config
        .lease_expiry_poll_interval()
        .saturating_add(config.lease_expiry_claim_lifetime())
        .saturating_add(config.readiness_check_interval()),
    ));
    resources.readiness.push(worker_health.clone());
    let schedule_health = Arc::new(WorkerHealth::new(
      SCHEDULE_WORKER_NAME,
      config
        .schedule_poll_interval()
        .saturating_add(config.schedule_claim_lifetime())
        .saturating_add(config.readiness_check_interval()),
    ));
    resources.readiness.push(schedule_health.clone());
    let internal_trigger_health = Arc::new(WorkerHealth::new(
      INTERNAL_TRIGGER_WORKER_NAME,
      config
        .internal_trigger_poll_interval()
        .saturating_add(config.internal_trigger_claim_lifetime())
        .saturating_add(config.readiness_check_interval()),
    ));
    resources.readiness.push(internal_trigger_health.clone());
    let manual_trigger_retry_health = Arc::new(WorkerHealth::new(
      MANUAL_TRIGGER_RETRY_WORKER_NAME,
      config
        .trigger_evaluation_poll_interval()
        .saturating_add(config.trigger_evaluation_claim_lifetime())
        .saturating_add(config.readiness_check_interval()),
    ));
    resources.readiness.push(manual_trigger_retry_health.clone());
    let expiry_worker = LeaseExpiryWorker::new(
      placement_store.clone(),
      WorkerOwner::new(format!("server:{}", uuid::Uuid::new_v4()))
        .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
      config.lease_expiry_batch_size(),
    )
    .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?;
    let ready_jobs = Arc::new(ReadyJobNotificationHub::default());
    let lease_service = AgentLeaseService::new(
      registration_service.clone(),
      placement_store.clone(),
      ready_jobs.clone(),
      config.agent_lease_lifetime(),
    )
    .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?;
    let execution_service = AgentExecutionService::with_log_archive(
      registration_service.clone(),
      placement_store,
      resources.object_storage.clone(),
      resources.log_redactor.clone(),
    );
    let heartbeat_service = AgentHeartbeatService::new(
      registration_service.clone(),
      registration_store.clone(),
      config.agent_lease_lifetime(),
    )
    .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?;
    let api_config =
      AgentApiConfig::new(config.agent_max_retry_delay_milliseconds()).ok_or(ServerRuntimeError::InvalidAgentPolicy)?;
    let webhook_deliveries = if config.webhook_bind().is_some() {
      let health = Arc::new(WorkerHealth::new(
        WEBHOOK_DELIVERY_WORKER_NAME,
        config
          .webhook_worker_poll_interval()
          .saturating_add(config.webhook_worker_claim_lifetime())
          .saturating_add(config.readiness_check_interval()),
      ));
      resources.readiness.push(health.clone());
      Some((
        WebhookDeliveryWorker::new(
          registration_store.clone(),
          registration_store.clone(),
          trigger_service.clone(),
          webhook_verifier,
          webhook_retry_policy,
        ),
        WorkerOwner::new(format!("webhook-delivery:{}", uuid::Uuid::new_v4()))
          .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
        health,
      ))
    } else {
      None
    };
    let managed_webhooks = config
      .webhook_callback_origin()
      .map(|callback_origin| {
        let health = Arc::new(WorkerHealth::new(
          MANAGED_WEBHOOK_WORKER_NAME,
          config
            .webhook_worker_poll_interval()
            .saturating_add(config.webhook_worker_claim_lifetime())
            .saturating_add(config.readiness_check_interval()),
        ));
        resources.readiness.push(health.clone());
        Ok::<_, ServerRuntimeError>((
          ManagedWebhookRegistrationWorker::new(
            registration_store.clone(),
            webhook_provider,
            callback_origin,
            webhook_retry_policy,
          ),
          WorkerOwner::new(format!("managed-webhook:{}", uuid::Uuid::new_v4()))
            .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
          health,
        ))
      })
      .transpose()?;
    let webhook_router = config
      .webhook_bind()
      .map(|_| webhook_router(WebhookApplication::new(webhook_ingress)));
    let cache_router = config.cache_bind().map(|_| {
      cache_router(
        CacheDataPlaneApplication::new(cache_data_plane),
        config.cache().max_blob_bytes() as usize,
      )
    });
    Self::start_with_components(
      config,
      resources.readiness,
      RuntimeComponents {
        agent_router: agent_router(
          AgentRouterDependencies::new(
            registration_service,
            Arc::new(lease_service),
            Arc::new(heartbeat_service),
            Arc::new(execution_service),
            artifact_service,
            cache_service,
          ),
          api_config,
        ),
        cache_router,
        management_application: Some(management_application),
        webhook_router,
        workers: Some(DurableWorkers {
          expiry: expiry_worker,
          expiry_health: worker_health,
          schedules: ScheduleWorker::new(registration_store.clone(), trigger_service.clone()),
          schedule_owner: WorkerOwner::new(format!("schedule:{}", uuid::Uuid::new_v4()))
            .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
          schedule_health,
          internal_triggers: InternalTriggerWorker::new(registration_store.clone(), trigger_service),
          internal_trigger_owner: WorkerOwner::new(format!("internal-trigger:{}", uuid::Uuid::new_v4()))
            .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
          internal_trigger_health,
          manual_trigger_retries,
          manual_trigger_retry_owner: WorkerOwner::new(format!("manual-trigger:worker:{}", uuid::Uuid::new_v4()))
            .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
          manual_trigger_retry_health,
          webhook_deliveries,
          managed_webhooks,
        }),
        ready_job_notifications: Some((notification_pool, ready_jobs)),
        cancellation,
      },
    )
    .await
  }
}
