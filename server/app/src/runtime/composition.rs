use std::sync::Arc;

use octacity_server_api_agent::{
  AgentApiConfig, AgentRouterDependencies, AgentTelemetryIngress, AgentTelemetryPolicy, ReadyJobNotificationHub,
  agent_router,
};
use octacity_server_api_cache::{CacheDataPlaneApplication, cache_router};
use octacity_server_api_webhook::{WebhookApplication, webhook_router};
use octacity_server_application::{
  AgentExecutionService, AgentHeartbeatService, AgentLeaseService, AgentRegistrationService, AgentTelemetryService,
  ArtifactHandlers, BuildRetentionWorker, CacheDataPlaneService, CacheSessionHandlers, DurableRetryPolicy,
  InternalTriggerWorker, LeaseExpiryWorker, LogIndexingWorker, ManagedWebhookRegistrationWorker,
  ManagementOperationalMetadataProjection, RevisionResolver, ScheduleWorker, WebhookDeliveryWorker,
};
use octacity_server_store::WorkerOwner;
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresLogSearchIndex};
use octacity_server_vcs::VcsAdapterRegistry;
use octacity_server_webhook::WebhookAdapterRegistry;
use tokio_util::sync::CancellationToken;

use super::{
  ServerRuntime, ServerRuntimeError,
  application::{ApplicationAssembly, ApplicationComponents, management_application},
  assembly::RuntimeResources,
  listeners::RuntimeComponents,
  telemetry::{UnavailableAgentTelemetryExporter, install_server_metrics},
  vcs::HostedRevisionResolver,
  workers::{
    DurableWorkers, EXPIRY_WORKER_NAME, INTERNAL_TRIGGER_WORKER_NAME, LOG_INDEX_WORKER_NAME,
    MANAGED_WEBHOOK_WORKER_NAME, MANUAL_TRIGGER_RETRY_WORKER_NAME, RETENTION_WORKER_NAME, SCHEDULE_WORKER_NAME,
    WEBHOOK_DELIVERY_WORKER_NAME,
  },
};
use crate::{ServerConfig, readiness::WorkerHealth};

impl ServerRuntime {
  /// Binds the listener and starts supervised work from validated configuration.
  pub async fn start(config: ServerConfig) -> Result<Self, ServerRuntimeError> {
    let installer = configured_factory_installer(&config).await?;
    Self::start_configured(config, installer, Default::default()).await
  }

  /// Starts the optional common Flow owner using explicitly installed capability bindings.
  pub async fn start_with_factory(
    config: ServerConfig,
    installer: Arc<dyn super::FactoryRuntimeInstaller>,
  ) -> Result<Self, ServerRuntimeError> {
    Self::start_configured(config, Some(installer), Default::default()).await
  }

  /// Installs task Source reads and mapping choices independently from optional Factory admission.
  pub async fn start_with_issue_sources(
    config: ServerConfig,
    sources: super::IssueSourceRuntimeInstallation,
  ) -> Result<Self, ServerRuntimeError> {
    Self::start_with_issue_integrations(
      config,
      super::IssueRuntimeIntegrations {
        sources: Some(sources),
        ..Default::default()
      },
    )
    .await
  }

  /// Installs independent optional Source/review readers through the existing application composition.
  /// Installing review capability schedules no provider read or additional poller.
  pub async fn start_with_issue_integrations(
    config: ServerConfig,
    integrations: super::IssueRuntimeIntegrations,
  ) -> Result<Self, ServerRuntimeError> {
    let installer = configured_factory_installer(&config).await?;
    Self::start_configured(config, installer, integrations).await
  }

  async fn start_configured(
    config: ServerConfig,
    installer: Option<Arc<dyn super::FactoryRuntimeInstaller>>,
    issue_integrations: super::IssueRuntimeIntegrations,
  ) -> Result<Self, ServerRuntimeError> {
    if config.factory().is_some() && installer.is_none() {
      return Err(ServerRuntimeError::MissingFactoryInstallation);
    }
    let metrics = install_server_metrics();
    let cancellation = CancellationToken::new();
    let lease_expiry_policy = config.lease_expiry_worker();
    let schedule_policy = config.schedule_worker();
    let internal_trigger_policy = config.internal_trigger_worker();
    let log_index_policy = config.log_index_worker();
    let retention_policy = config.retention_worker();
    let trigger_evaluation_policy = config.trigger_evaluation_worker();
    let webhook_policy = config.webhook_worker();
    let webhook_delivery_policy = webhook_policy.delivery();
    let mut resources = RuntimeResources::from_config(&config)
      .await
      .map_err(ServerRuntimeError::Assembly)?;
    let placement_store = Arc::new(PostgresAuthoritativeStore::new(
      resources.postgres.clone(),
      resources.job_spec_signer.clone(),
    ));
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
    let (registration_store, mut factory) = super::factory::assemble(
      &config,
      &mut resources,
      placement_store.clone(),
      installer,
      revision_resolver.clone(),
    )
    .await?;
    let log_search_index = Arc::new(PostgresLogSearchIndex::new(resources.postgres.clone()));
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
    let webhook_retry_policy = DurableRetryPolicy::new(
      webhook_delivery_policy.max_attempts(),
      webhook_delivery_policy.initial_retry_milliseconds(),
      webhook_delivery_policy.maximum_retry_milliseconds(),
    )
    .map_err(|_| ServerRuntimeError::InvalidManagementPolicy)?;
    let vcs_retry_policy = DurableRetryPolicy::new(
      trigger_evaluation_policy.max_attempts(),
      trigger_evaluation_policy.initial_retry_milliseconds(),
      trigger_evaluation_policy.maximum_retry_milliseconds(),
    )
    .map_err(|_| ServerRuntimeError::InvalidManagementPolicy)?;
    let log_index_retry_policy = DurableRetryPolicy::new(
      log_index_policy.max_attempts(),
      log_index_policy.initial_retry_milliseconds(),
      log_index_policy.maximum_retry_milliseconds(),
    )
    .map_err(|_| ServerRuntimeError::InvalidManagementPolicy)?;
    let retention_retry_policy = DurableRetryPolicy::new(
      retention_policy.retrying().max_attempts(),
      retention_policy.retrying().initial_retry_milliseconds(),
      retention_policy.retrying().maximum_retry_milliseconds(),
    )
    .map_err(|_| ServerRuntimeError::InvalidManagementPolicy)?;
    let installed_integrations = super::integration_management::install(
      &config,
      registration_store.clone(),
      cancellation.child_token(),
      &mut resources.readiness,
    )
    .await?;
    let (integrations, source_reads, managed_issue_sources, source_management) = installed_integrations
      .map_or((None, None, None, None), |i| {
        (Some(i.application), i.source_reads, i.issues, i.source_management)
      });
    let source_reporting = if let Some(sources) = &source_management {
      let mut empty = vec![];
      let explicit = factory.as_mut().map_or(&mut empty, |factory| &mut factory.outbox);
      match sources.reporting_dispatcher(explicit) {
        Ok(worker) => worker,
        Err(octacity_server_application::ApplicationError::CapabilityUnavailable) => {
          resources
            .readiness
            .push_factory(Arc::new(super::source_reporting::UnavailableSourceReporting));
          tracing::warn!("shared Source Reporter overlaps an explicitly installed reporting owner");
          None
        }
        Err(_) => return Err(ServerRuntimeError::InvalidManagementPolicy),
      }
    } else {
      None
    };
    let managed_factory_source = source_management
      .map(|sources| {
        let principal = &config
          .integration_management()
          .expect("installed local Source authority")
          .principal;
        super::managed_factory_source::ManagedFactorySource::worker(
          sources,
          resources.object_storage.clone(),
          revision_resolver.clone(),
          principal,
        )
      })
      .transpose()
      .map_err(|_| ServerRuntimeError::InvalidManagementPolicy)?;
    let ApplicationComponents {
      management: management_application,
      trigger_service,
      manual_trigger_retries,
      webhook_ingress,
      webhook_verifier,
      webhook_provider,
    } = management_application(ApplicationAssembly {
      issue_integrations,
      managed_issue_sources,
      store: registration_store.clone(),
      integrations,
      authoritative_store: placement_store.clone(),
      toolchain: resources.job_spec_toolchain,
      supported_pipeline_capabilities: config.supported_pipeline_capabilities().to_vec(),
      agent_enrollment_lifetime: config.agent_enrollment_lifetime(),
      agent_enrollment_secret_key: resources.agent_enrollment_secret_key,
      webhook_configuration,
      revision_resolver,
      cancellation: cancellation.child_token(),
      vcs_retry_policy,
      trigger_claim_lifetime: trigger_evaluation_policy.worker().claim_lifetime(),
      artifacts: artifact_service.clone(),
      object_storage: resources.object_storage.clone(),
      cache: cache_service.clone(),
      log_search_index: log_search_index.clone(),
      operational_metadata: ManagementOperationalMetadataProjection {
        management_externally_reachable: config.management_externally_reachable(),
        external_access_acknowledged: config.unauthenticated_management_acknowledged(),
        agent_ingress_enabled: config.agent_bind().is_some(),
        webhook_ingress_enabled: config.webhook_bind().is_some(),
      },
    })?;
    let worker_health = Arc::new(WorkerHealth::new(
      EXPIRY_WORKER_NAME,
      lease_expiry_policy
        .poll_interval()
        .saturating_add(lease_expiry_policy.claim_lifetime())
        .saturating_add(config.readiness_check_interval()),
    ));
    resources.readiness.push(worker_health.clone());
    let schedule_health = Arc::new(WorkerHealth::new(
      SCHEDULE_WORKER_NAME,
      schedule_policy
        .poll_interval()
        .saturating_add(schedule_policy.claim_lifetime())
        .saturating_add(config.readiness_check_interval()),
    ));
    resources.readiness.push(schedule_health.clone());
    let internal_trigger_health = Arc::new(WorkerHealth::new(
      INTERNAL_TRIGGER_WORKER_NAME,
      internal_trigger_policy
        .poll_interval()
        .saturating_add(internal_trigger_policy.claim_lifetime())
        .saturating_add(config.readiness_check_interval()),
    ));
    resources.readiness.push(internal_trigger_health.clone());
    let log_index_health = Arc::new(WorkerHealth::new(
      LOG_INDEX_WORKER_NAME,
      log_index_policy
        .worker()
        .poll_interval()
        .saturating_add(log_index_policy.worker().claim_lifetime())
        .saturating_add(config.readiness_check_interval()),
    ));
    resources.readiness.push(log_index_health.clone());
    let retention_health = Arc::new(WorkerHealth::new(
      RETENTION_WORKER_NAME,
      retention_policy
        .retrying()
        .worker()
        .poll_interval()
        .saturating_add(retention_policy.retrying().worker().claim_lifetime())
        .saturating_add(config.readiness_check_interval()),
    ));
    resources.readiness.push(retention_health.clone());
    let maintenance = retention_policy.retrying().worker();
    let input_retention_health = Arc::new(WorkerHealth::new(
      super::workers::INPUT_RETENTION_WORKER_NAME,
      config
        .factory()
        .map_or(
          maintenance.poll_interval().saturating_add(maintenance.claim_lifetime()),
          |policy| policy.poll_interval().saturating_add(policy.claim_lifetime()),
        )
        .saturating_add(config.readiness_check_interval()),
    ));
    if config.factory().is_some() {
      resources.readiness.push_factory(input_retention_health.clone());
    }
    let manual_trigger_retry_health = Arc::new(WorkerHealth::new(
      MANUAL_TRIGGER_RETRY_WORKER_NAME,
      trigger_evaluation_policy
        .worker()
        .poll_interval()
        .saturating_add(trigger_evaluation_policy.worker().claim_lifetime())
        .saturating_add(config.readiness_check_interval()),
    ));
    resources.readiness.push(manual_trigger_retry_health.clone());
    let expiry_worker = LeaseExpiryWorker::new(
      placement_store.clone(),
      WorkerOwner::new(format!("server:{}", uuid::Uuid::new_v4()))
        .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
      lease_expiry_policy.batch_size(),
    )
    .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?;
    let ready_jobs = Arc::new(ReadyJobNotificationHub::default());
    let lease_service = AgentLeaseService::new(
      registration_service.clone(),
      placement_store.clone(),
      ready_jobs.clone(),
      config.agent_lease_lifetime(),
    )
    .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?
    .with_resources(registration_store.clone());
    let execution_service = AgentExecutionService::with_log_archive(
      registration_service.clone(),
      placement_store.clone(),
      resources.object_storage.clone(),
      resources.log_redactor.clone(),
      config.orphan_log_cleanup_grace(),
    );
    let execution_service = match factory.as_ref().and_then(|worker| worker.output_finalizer.clone()) {
      Some(finalizer) => execution_service.with_output_finalizer(finalizer),
      None => execution_service,
    };
    let heartbeat_service = AgentHeartbeatService::new(
      registration_service.clone(),
      registration_store.clone(),
      config.agent_lease_lifetime(),
    )
    .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?;
    let telemetry_service = AgentTelemetryIngress::new(
      Arc::new(AgentTelemetryService::new(registration_service.clone())),
      Arc::new(UnavailableAgentTelemetryExporter),
      AgentTelemetryPolicy::new(
        config.agent_telemetry_max_in_flight_exports(),
        config.agent_telemetry_export_timeout(),
      )
      .ok_or(ServerRuntimeError::InvalidAgentPolicy)?,
    );
    let api_config =
      AgentApiConfig::new(config.agent_max_retry_delay_milliseconds()).ok_or(ServerRuntimeError::InvalidAgentPolicy)?;
    let tool_actions = factory.as_ref().map(|worker| {
      Arc::new(octacity_server_application::AgentToolActionService::new(
        registration_service.clone(),
        placement_store.clone(),
        worker.tool_authorizer.clone(),
      )) as Arc<dyn octacity_server_application::AgentToolActionUseCases>
    });
    let webhook_deliveries = if config.webhook_bind().is_some() {
      let health = Arc::new(WorkerHealth::new(
        WEBHOOK_DELIVERY_WORKER_NAME,
        webhook_delivery_policy
          .worker()
          .poll_interval()
          .saturating_add(webhook_delivery_policy.worker().claim_lifetime())
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
          webhook_delivery_policy
            .worker()
            .poll_interval()
            .saturating_add(webhook_delivery_policy.worker().claim_lifetime())
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
          {
            let dependencies = AgentRouterDependencies::new(
              registration_service,
              Arc::new(lease_service),
              Arc::new(heartbeat_service),
              telemetry_service,
              Arc::new(execution_service),
              artifact_service,
              cache_service,
            );
            match tool_actions {
              Some(actions) => dependencies.with_tool_actions(actions),
              None => dependencies,
            }
          },
          api_config,
        ),
        cache_router,
        management_application: Some(management_application),
        webhook_router,
        workers: Some(DurableWorkers {
          source_reads,
          managed_factory_source,
          source_reporting,
          factory,
          legacy_issues: registration_store.clone(),
          actions: octacity_server_application::FactoryActionProjectionWorker::new(registration_store.clone()),
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
          log_indexing: LogIndexingWorker::new(
            registration_store.clone(),
            log_search_index.clone(),
            resources.object_storage.clone(),
            log_index_retry_policy,
          ),
          log_index_owner: WorkerOwner::new(format!("log-index:{}", uuid::Uuid::new_v4()))
            .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
          log_index_health,
          retention: BuildRetentionWorker::new(
            registration_store.clone(),
            log_search_index,
            resources.object_storage.clone(),
            resources.object_storage.clone(),
            retention_retry_policy,
          ),
          retention_owner: WorkerOwner::new(format!("retention:{}", uuid::Uuid::new_v4()))
            .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
          retention_health,
          input_retention: octacity_server_application::FactoryExecutionInputRetentionWorker::new(
            registration_store.clone(),
            resources.object_storage.clone(),
          ),
          input_retention_health,
          source_retention: octacity_server_application::FactorySourceInputRetentionWorker::new(
            registration_store.clone(),
            resources.object_storage.clone(),
            retention_policy.retrying().worker().poll_interval(),
          )
          .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
          manual_trigger_retries,
          manual_trigger_retry_owner: WorkerOwner::new(format!("manual-trigger:worker:{}", uuid::Uuid::new_v4()))
            .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
          manual_trigger_retry_health,
          webhook_deliveries,
          managed_webhooks,
        }),
        ready_job_notifications: Some((notification_pool, ready_jobs)),
        metrics,
        critical_conditions: Some(registration_store),
        cancellation,
      },
    )
    .await
  }
}

// Both public entry points use the same optional Factory profile installation policy.
async fn configured_factory_installer(
  config: &ServerConfig,
) -> Result<Option<Arc<dyn super::FactoryRuntimeInstaller>>, ServerRuntimeError> {
  let installer = match config.factory_profiles_file() {
    Some(path) => {
      let providers = match config.factory_signal_connectors_file() {
        Some(connectors) => super::load_factory_signal_connectors(connectors)
          .await
          .map_err(|_| ServerRuntimeError::InvalidFactoryInstallation)?,
        None => vec![],
      };
      Some(Arc::new(
        super::factory_profile_file::FileProfileInstaller::load(path, &providers)
          .await
          .map_err(|_| ServerRuntimeError::InvalidFactoryInstallation)?,
      ) as Arc<dyn super::FactoryRuntimeInstaller>)
    }
    None => None,
  };
  Ok(installer)
}
