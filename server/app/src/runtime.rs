use std::{net::SocketAddr, sync::Arc};

use thiserror::Error;
use tokio::{
  net::TcpListener,
  task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

mod application;
mod vcs;
mod webhook;
mod workers;

use application::{ApplicationAssembly, ApplicationComponents, management_application};
use vcs::HostedRevisionResolver;
use webhook::{HostedWebhookVerifier, UnavailableWebhookVerifier};
use workers::{
  EXPIRY_WORKER_NAME, INTERNAL_TRIGGER_WORKER_NAME, MANAGED_WEBHOOK_WORKER_NAME, MANUAL_TRIGGER_RETRY_WORKER_NAME,
  SCHEDULE_WORKER_NAME, WEBHOOK_DELIVERY_WORKER_NAME,
};

use octacity_server_api_agent::{AgentApiConfig, AgentRouterDependencies, ReadyJobNotificationHub, agent_router};
use octacity_server_api_rest::{
  JobEventNotificationHub,
  v1::{
    AgentManagementApplication, ArtifactManagementApplication, BuildManagementApplication, CacheManagementApplication,
    CatalogManagementApplication, ConfigurationManagementApplication, DefinitionManagementApplication,
    ExecutionManagementApplication, JobEventManagementApplication, ManagementApplication,
    ManagementApplicationHandlers, ManualTriggerManagementApplication, PipelineManagementApplication,
    ProjectManagementApplication, ScheduleManagementApplication,
  },
};
use octacity_server_api_webhook::{WebhookApplication, webhook_router};
use octacity_server_application::{
  AgentEnrollmentHandler, AgentExecutionService, AgentHandlers, AgentHeartbeatService, AgentLeaseService,
  AgentPoolHandlers, AgentRegistrationService, ArtifactHandlers, BuildConfigurationHandlers, BuildHandlers,
  CacheSessionHandlers, DefinitionHandlers, DurableManualTriggerService, DurableRetryPolicy, InternalTriggerWorker,
  JobEventLongPoll, JobSpecToolchainPolicy, LeaseExpiryWorker, ManagedWebhookRegistrationWorker,
  ManualTriggerRetryWorker, ManualTriggerService, PipelineHandlers, ProjectHandlers, RevisionResolver,
  ScheduleHandlers, ScheduleWorker, StoreBackedEffectiveProjectPolicySource, StoreBackedManualTriggerContext,
  WebhookDeliveryVerifier, WebhookDeliveryWorker, WebhookIngressService, WebhookManagementProvider,
  WebhookManagementService,
};
use octacity_server_store::WorkerOwner;
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
use octacity_server_vcs::VcsAdapterRegistry;
use octacity_server_webhook::WebhookAdapterRegistry;

use crate::{
  ServerConfig,
  readiness::{ReadinessChecks, ReadinessMonitor, ReadinessState, RuntimeDependencies, WorkerHealth},
};

/// Running server process and ownership handle for its cancellation tree.
#[must_use = "dropping the runtime aborts its listeners; call shutdown for graceful drain"]
pub struct ServerRuntime {
  management_addr: SocketAddr,
  agent_addr: Option<SocketAddr>,
  webhook_addr: Option<SocketAddr>,
  shutdown_grace: std::time::Duration,
  readiness: Arc<ReadinessState>,
  cancellation: CancellationToken,
  listener_tasks: JoinSet<ListenerTaskResult>,
  readiness_task: Option<JoinHandle<()>>,
  worker_task: Option<JoinHandle<()>>,
  notification_task: Option<JoinHandle<()>>,
}

type ListenerTaskResult = Result<&'static str, (&'static str, std::io::Error)>;

enum RuntimeTaskExit {
  Listener(Option<Result<ListenerTaskResult, tokio::task::JoinError>>),
  Readiness(Result<(), tokio::task::JoinError>),
  Workers(Result<(), tokio::task::JoinError>),
  Notifications(Result<(), tokio::task::JoinError>),
}

struct DurableWorkers {
  expiry: LeaseExpiryWorker<PostgresAuthoritativeStore>,
  expiry_health: Arc<WorkerHealth>,
  schedules: ScheduleWorker<PostgresStore>,
  schedule_owner: WorkerOwner,
  schedule_health: Arc<WorkerHealth>,
  internal_triggers: InternalTriggerWorker<PostgresStore>,
  internal_trigger_owner: WorkerOwner,
  internal_trigger_health: Arc<WorkerHealth>,
  manual_trigger_retries: ManualTriggerRetryWorker,
  manual_trigger_retry_owner: WorkerOwner,
  manual_trigger_retry_health: Arc<WorkerHealth>,
  webhook_deliveries: Option<(WebhookDeliveryWorker, WorkerOwner, Arc<WorkerHealth>)>,
  managed_webhooks: Option<(ManagedWebhookRegistrationWorker, WorkerOwner, Arc<WorkerHealth>)>,
}

struct RuntimeComponents {
  agent_router: axum::Router,
  management_application: Option<ManagementApplication>,
  webhook_router: Option<axum::Router>,
  workers: Option<DurableWorkers>,
  ready_job_notifications: Option<(sqlx::PgPool, Arc<ReadyJobNotificationHub>)>,
  cancellation: CancellationToken,
}

impl ServerRuntime {
  /// Binds the listener and starts supervised work from validated configuration.
  pub async fn start(config: ServerConfig) -> Result<Self, ServerRuntimeError> {
    let cancellation = CancellationToken::new();
    let mut dependencies = RuntimeDependencies::from_config(&config)
      .await
      .map_err(ServerRuntimeError::ReadinessSetup)?;
    let registration_store = Arc::new(PostgresStore::new(dependencies.postgres.clone()));
    let notification_pool = dependencies.postgres.clone();
    let registration_service = Arc::new(
      AgentRegistrationService::new(registration_store.clone(), config.agent_registration_lifetime())
        .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
    );
    let artifact_service = Arc::new(
      ArtifactHandlers::new(
        registration_service.clone(),
        registration_store.clone(),
        dependencies.object_storage.clone(),
        config.object_storage().upload_capability_lifetime(),
        config.object_storage().download_capability_lifetime(),
      )
      .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
    );
    let cache_service = Arc::new(
      CacheSessionHandlers::new(
        registration_service.clone(),
        registration_store.clone(),
        dependencies.cache_credential_key,
        config.cache().endpoint.clone(),
        config.cache().session_lifetime(),
      )
      .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
    );
    let placement_store = Arc::new(PostgresAuthoritativeStore::new(
      dependencies.postgres,
      dependencies.job_spec_signer,
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
      toolchain: dependencies.job_spec_toolchain,
      supported_pipeline_capabilities: config.supported_pipeline_capabilities().to_vec(),
      agent_enrollment_lifetime: config.agent_enrollment_lifetime(),
      agent_enrollment_secret_key: dependencies.agent_enrollment_secret_key,
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
    dependencies.readiness.push(worker_health.clone());
    let schedule_health = Arc::new(WorkerHealth::new(
      SCHEDULE_WORKER_NAME,
      config
        .schedule_poll_interval()
        .saturating_add(config.schedule_claim_lifetime())
        .saturating_add(config.readiness_check_interval()),
    ));
    dependencies.readiness.push(schedule_health.clone());
    let internal_trigger_health = Arc::new(WorkerHealth::new(
      INTERNAL_TRIGGER_WORKER_NAME,
      config
        .internal_trigger_poll_interval()
        .saturating_add(config.internal_trigger_claim_lifetime())
        .saturating_add(config.readiness_check_interval()),
    ));
    dependencies.readiness.push(internal_trigger_health.clone());
    let manual_trigger_retry_health = Arc::new(WorkerHealth::new(
      MANUAL_TRIGGER_RETRY_WORKER_NAME,
      config
        .trigger_evaluation_poll_interval()
        .saturating_add(config.trigger_evaluation_claim_lifetime())
        .saturating_add(config.readiness_check_interval()),
    ));
    dependencies.readiness.push(manual_trigger_retry_health.clone());
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
    let execution_service = AgentExecutionService::new(registration_service.clone(), placement_store);
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
      dependencies.readiness.push(health.clone());
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
        dependencies.readiness.push(health.clone());
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
    Self::start_with_components(
      config,
      dependencies.readiness,
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

  /// Starts the process with concrete dependency and worker health checks.
  #[cfg(test)]
  pub(crate) async fn start_with_readiness(
    config: ServerConfig,
    checks: ReadinessChecks,
  ) -> Result<Self, ServerRuntimeError> {
    Self::start_with_components(
      config,
      checks,
      RuntimeComponents {
        agent_router: axum::Router::new(),
        management_application: None,
        webhook_router: None,
        workers: None,
        ready_job_notifications: None,
        cancellation: CancellationToken::new(),
      },
    )
    .await
  }

  async fn start_with_components(
    config: ServerConfig,
    checks: ReadinessChecks,
    components: RuntimeComponents,
  ) -> Result<Self, ServerRuntimeError> {
    let RuntimeComponents {
      agent_router,
      management_application,
      webhook_router,
      workers,
      ready_job_notifications,
      cancellation,
    } = components;
    if config.webhook_bind().is_some() != webhook_router.is_some() {
      return Err(ServerRuntimeError::InvalidManagementPolicy);
    }
    let management_listener = bind_listener("management", config.management_bind()).await?;
    let agent_listener = bind_optional_listener("agent", config.agent_bind()).await?;
    let webhook_listener = bind_optional_listener("webhook", config.webhook_bind()).await?;
    let management_addr = inspect_listener("management", &management_listener)?;
    let agent_addr = agent_listener
      .as_ref()
      .map(|listener| inspect_listener("agent", listener))
      .transpose()?;
    let webhook_addr = webhook_listener
      .as_ref()
      .map(|listener| inspect_listener("webhook", listener))
      .transpose()?;
    let monitor = ReadinessMonitor::start(
      checks,
      config.readiness_check_interval(),
      config.readiness_check_timeout(),
      cancellation.child_token(),
    )
    .await;
    let readiness = monitor.state();
    let readiness_task = monitor.into_task();
    let worker_task =
      workers.map(|workers| workers::spawn_durable_workers(workers, &config, cancellation.child_token()));
    let notification_task = ready_job_notifications.map(|(pool, hub)| {
      spawn_ready_job_listener(
        pool,
        hub,
        config.ready_job_listener_reconnect_delay(),
        cancellation.child_token(),
      )
    });
    let router_readiness = readiness.clone();
    let metadata = octacity_server_api_rest::v1::OperationalMetadata::trusted_network(
      config.management_externally_reachable(),
      config.unauthenticated_management_acknowledged(),
      agent_addr.is_some(),
      webhook_addr.is_some(),
    );
    let management_router = match management_application {
      Some(application) => octacity_server_api_rest::management_router_with_application_and_metadata(
        move || router_readiness.is_ready(),
        application,
        metadata,
      ),
      None => octacity_server_api_rest::management_router_with_metadata(move || router_readiness.is_ready(), metadata),
    };
    let mut listener_tasks = JoinSet::new();
    spawn_listener(
      &mut listener_tasks,
      "management",
      management_listener,
      management_router,
      cancellation.child_token(),
    );
    if let Some(listener) = agent_listener {
      spawn_listener(
        &mut listener_tasks,
        "agent",
        listener,
        agent_router,
        cancellation.child_token(),
      );
    }
    if let (Some(listener), Some(router)) = (webhook_listener, webhook_router) {
      spawn_listener(
        &mut listener_tasks,
        "webhook",
        listener,
        router,
        cancellation.child_token(),
      );
    }
    info!(%management_addr, ?agent_addr, ?webhook_addr, "server listeners ready");
    Ok(Self {
      management_addr,
      agent_addr,
      webhook_addr,
      shutdown_grace: config.shutdown_grace(),
      readiness,
      cancellation,
      listener_tasks,
      readiness_task: Some(readiness_task),
      worker_task,
      notification_task,
    })
  }

  /// Actual bound management address, including an OS-assigned port.
  pub const fn management_addr(&self) -> SocketAddr {
    self.management_addr
  }

  /// Actual bound Agent address when authenticated Agent ingress is configured.
  pub const fn agent_addr(&self) -> Option<SocketAddr> {
    self.agent_addr
  }

  /// Actual bound webhook address when authenticated webhook ingress is configured.
  pub const fn webhook_addr(&self) -> Option<SocketAddr> {
    self.webhook_addr
  }

  /// Waits until any supervised process task exits unexpectedly.
  ///
  /// Process entry points should select this future against their shutdown
  /// signal so a failed listener or background worker cannot leave an
  /// apparently healthy process.
  pub async fn wait(&mut self) -> Result<(), ServerRuntimeError> {
    let exit = tokio::select! {
      result = self.listener_tasks.join_next() => RuntimeTaskExit::Listener(result),
      result = wait_for_optional_task(&mut self.readiness_task) => RuntimeTaskExit::Readiness(result),
      result = wait_for_optional_task(&mut self.worker_task) => RuntimeTaskExit::Workers(result),
      result = wait_for_optional_task(&mut self.notification_task) => RuntimeTaskExit::Notifications(result),
    };
    let failure = match exit {
      RuntimeTaskExit::Listener(result) => match result {
        Some(Ok(Ok(ingress))) => ServerRuntimeError::ListenerUnexpectedExit { ingress },
        Some(Ok(Err((ingress, source)))) => ServerRuntimeError::Serve { ingress, source },
        Some(Err(source)) => ServerRuntimeError::ListenerTask(source),
        None => ServerRuntimeError::UnexpectedExit,
      },
      RuntimeTaskExit::Readiness(result) => {
        self.readiness_task.take();
        result.map_or_else(ServerRuntimeError::ReadinessTask, |()| {
          ServerRuntimeError::SupervisedTaskUnexpectedExit {
            task: "readiness-monitor",
          }
        })
      }
      RuntimeTaskExit::Workers(result) => {
        self.worker_task.take();
        result.map_or_else(ServerRuntimeError::WorkerTask, |()| {
          ServerRuntimeError::SupervisedTaskUnexpectedExit {
            task: "durable-workers",
          }
        })
      }
      RuntimeTaskExit::Notifications(result) => {
        self.notification_task.take();
        result.map_or_else(ServerRuntimeError::NotificationTask, |()| {
          ServerRuntimeError::SupervisedTaskUnexpectedExit {
            task: "ready-job-notifications",
          }
        })
      }
    };
    self.readiness.set(false);
    self.cancellation.cancel();
    while self.listener_tasks.join_next().await.is_some() {}
    if let Some(readiness_task) = self.readiness_task.take()
      && let Err(source) = readiness_task.await
    {
      return Err(ServerRuntimeError::ReadinessTask(source));
    }
    if let Some(worker_task) = self.worker_task.take()
      && let Err(source) = worker_task.await
    {
      return Err(ServerRuntimeError::WorkerTask(source));
    }
    if let Some(notification_task) = self.notification_task.take()
      && let Err(source) = notification_task.await
    {
      return Err(ServerRuntimeError::NotificationTask(source));
    }
    Err(failure)
  }

  /// Stops admission, cancels the process tree, and waits for bounded drain.
  pub async fn shutdown(mut self) -> Result<(), ServerRuntimeError> {
    self.readiness.set(false);
    self.cancellation.cancel();
    if self.listener_tasks.is_empty() {
      return Err(ServerRuntimeError::UnexpectedExit);
    }
    let Some(mut readiness_task) = self.readiness_task.take() else {
      return Err(ServerRuntimeError::UnexpectedExit);
    };
    let mut worker_task = self.worker_task.take();
    let mut notification_task = self.notification_task.take();
    let result = tokio::time::timeout(self.shutdown_grace, async {
      let mut listener_error = None;
      while let Some(result) = self.listener_tasks.join_next().await {
        match result {
          Ok(Ok(_)) => {}
          Ok(Err((ingress, source))) if listener_error.is_none() => {
            listener_error = Some(ServerRuntimeError::Serve { ingress, source });
          }
          Err(source) if listener_error.is_none() => {
            listener_error = Some(ServerRuntimeError::ListenerTask(source));
          }
          _ => {}
        }
      }
      if let Err(source) = (&mut readiness_task).await {
        return Err(ServerRuntimeError::ReadinessTask(source));
      }
      if let Some(task) = worker_task.as_mut()
        && let Err(source) = task.await
      {
        return Err(ServerRuntimeError::WorkerTask(source));
      }
      if let Some(task) = notification_task.as_mut()
        && let Err(source) = task.await
      {
        return Err(ServerRuntimeError::NotificationTask(source));
      }
      listener_error.map_or(Ok(()), Err)
    })
    .await;
    match result {
      Ok(Ok(())) => {
        info!(%self.management_addr, "server shutdown complete");
        Ok(())
      }
      Ok(Err(error)) => Err(error),
      Err(_) => {
        self.listener_tasks.abort_all();
        readiness_task.abort();
        if let Some(task) = worker_task.as_mut() {
          task.abort();
        }
        if let Some(task) = notification_task.as_mut() {
          task.abort();
        }
        while self.listener_tasks.join_next().await.is_some() {}
        let _ = readiness_task.await;
        if let Some(task) = worker_task {
          let _ = task.await;
        }
        if let Some(task) = notification_task {
          let _ = task.await;
        }
        Err(ServerRuntimeError::ShutdownTimeout(self.shutdown_grace))
      }
    }
  }
}

impl Drop for ServerRuntime {
  fn drop(&mut self) {
    self.readiness.set(false);
    self.cancellation.cancel();
    self.listener_tasks.abort_all();
    if let Some(readiness_task) = self.readiness_task.take() {
      readiness_task.abort();
    }
    if let Some(worker_task) = self.worker_task.take() {
      worker_task.abort();
    }
    if let Some(notification_task) = self.notification_task.take() {
      notification_task.abort();
    }
  }
}

fn spawn_ready_job_listener(
  pool: sqlx::PgPool,
  hub: Arc<ReadyJobNotificationHub>,
  reconnect_delay: std::time::Duration,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  tokio::spawn(async move {
    loop {
      let result = ready_job_listener_pass(&pool, &hub, cancellation.child_token()).await;
      if cancellation.is_cancelled() {
        return;
      }
      if let Err(error) = result {
        warn!(%error, "ready-Job notification listener disconnected");
      }
      tokio::select! {
        biased;
        () = cancellation.cancelled() => return,
        () = tokio::time::sleep(reconnect_delay) => {}
      }
    }
  })
}

async fn ready_job_listener_pass(
  pool: &sqlx::PgPool,
  hub: &ReadyJobNotificationHub,
  cancellation: CancellationToken,
) -> Result<(), sqlx::Error> {
  let mut listener = sqlx::postgres::PgListener::connect_with(pool).await?;
  listener
    .listen(octacity_server_store_postgres::READY_JOB_NOTIFICATION_CHANNEL)
    .await?;
  loop {
    tokio::select! {
      biased;
      () = cancellation.cancelled() => return Ok(()),
      notification = listener.recv() => {
        notification?;
        hub.notify();
      }
    }
  }
}

async fn bind_listener(ingress: &'static str, address: SocketAddr) -> Result<TcpListener, ServerRuntimeError> {
  TcpListener::bind(address)
    .await
    .map_err(|source| ServerRuntimeError::Bind {
      ingress,
      address,
      source,
    })
}

async fn bind_optional_listener(
  ingress: &'static str,
  address: Option<SocketAddr>,
) -> Result<Option<TcpListener>, ServerRuntimeError> {
  match address {
    Some(address) => bind_listener(ingress, address).await.map(Some),
    None => Ok(None),
  }
}

fn inspect_listener(ingress: &'static str, listener: &TcpListener) -> Result<SocketAddr, ServerRuntimeError> {
  listener
    .local_addr()
    .map_err(|source| ServerRuntimeError::InspectListener { ingress, source })
}

fn spawn_listener(
  tasks: &mut JoinSet<ListenerTaskResult>,
  ingress: &'static str,
  listener: TcpListener,
  router: axum::Router,
  cancellation: CancellationToken,
) {
  tasks.spawn(async move {
    axum::serve(listener, router)
      .with_graceful_shutdown(cancellation.cancelled_owned())
      .await
      .map_err(|source| (ingress, source))?;
    Ok(ingress)
  });
}

async fn wait_for_optional_task(task: &mut Option<JoinHandle<()>>) -> Result<(), tokio::task::JoinError> {
  match task {
    Some(task) => task.await,
    None => std::future::pending().await,
  }
}

/// Failure to start, serve, or gracefully stop the server process.
#[derive(Debug, Error)]
pub enum ServerRuntimeError {
  /// Static dependency material could not be loaded safely.
  #[error("failed to configure readiness dependencies: {0}")]
  ReadinessSetup(crate::ReadinessSetupError),
  /// Agent registration policy was invalid after configuration loading.
  #[error("agent registration policy is invalid")]
  InvalidAgentPolicy,
  /// Management input or executable-policy configuration was invalid.
  #[error("management application policy is invalid")]
  InvalidManagementPolicy,
  /// Operator-installed webhook adapter registry failed strict verification.
  #[error("webhook adapter registry is invalid: {0}")]
  WebhookRegistry(octacity_server_webhook::RegistryError),
  /// Operator-installed VCS adapter registry failed strict verification.
  #[error("VCS adapter registry is invalid: {0}")]
  VcsRegistry(octacity_server_vcs::RegistryError),
  /// One independently configured listener could not bind its address.
  #[error("failed to bind {ingress} listener at {address}: {source}")]
  Bind {
    /// Independently configured ingress whose listener could not bind.
    ingress: &'static str,
    /// Requested listener address.
    address: SocketAddr,
    /// Underlying socket error.
    source: std::io::Error,
  },
  /// The bound listener address could not be inspected.
  #[error("failed to inspect bound {ingress} listener: {source}")]
  InspectListener {
    /// Independently configured ingress whose listener could not be inspected.
    ingress: &'static str,
    /// Underlying socket error.
    source: std::io::Error,
  },
  /// The HTTP server returned an I/O failure.
  #[error("{ingress} listener failed: {source}")]
  Serve {
    /// Independently configured ingress whose server failed.
    ingress: &'static str,
    /// Underlying server error.
    source: std::io::Error,
  },
  /// A supervised listener task panicked or was cancelled unexpectedly.
  #[error("listener task failed: {0}")]
  ListenerTask(tokio::task::JoinError),
  /// The supervised readiness monitor panicked or was cancelled unexpectedly.
  #[error("readiness monitor task failed: {0}")]
  ReadinessTask(tokio::task::JoinError),
  /// The supervised durable-worker group panicked or was cancelled unexpectedly.
  #[error("durable worker task failed: {0}")]
  WorkerTask(tokio::task::JoinError),
  /// The process-local PostgreSQL notification listener panicked or was cancelled unexpectedly.
  #[error("ready-Job notification task failed: {0}")]
  NotificationTask(tokio::task::JoinError),
  /// A supervised process task returned without a shutdown request.
  #[error("supervised task {task} exited unexpectedly")]
  SupervisedTaskUnexpectedExit {
    /// Stable non-sensitive task name.
    task: &'static str,
  },
  /// The listener stopped without a requested shutdown.
  #[error("management listener exited unexpectedly")]
  UnexpectedExit,
  /// One specific independently configured listener stopped without shutdown.
  #[error("{ingress} listener exited unexpectedly")]
  ListenerUnexpectedExit {
    /// Ingress whose listener stopped.
    ingress: &'static str,
  },
  /// In-flight work did not drain within the configured grace period.
  #[error("server did not shut down within {0:?}")]
  ShutdownTimeout(std::time::Duration),
}

#[cfg(test)]
mod tests;
