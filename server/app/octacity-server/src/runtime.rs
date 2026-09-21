use std::{
  net::SocketAddr,
  sync::Arc,
  time::{SystemTime, UNIX_EPOCH},
};

use thiserror::Error;
use tokio::{
  net::TcpListener,
  task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use octacity_server_api_agent::{AgentApiConfig, ReadyJobNotificationHub, agent_router};
use octacity_server_api_rest::{
  JobEventNotificationHub,
  v1::{
    AgentManagementApplication, BuildManagementApplication, CatalogManagementApplication,
    ConfigurationManagementApplication, DefinitionManagementApplication, ExecutionManagementApplication,
    JobEventManagementApplication, ManagementApplication, ManagementApplicationHandlers,
    ManualTriggerManagementApplication, PipelineManagementApplication, ProjectManagementApplication,
  },
};
use octacity_server_application::{
  AgentEnrollmentHandler, AgentExecutionService, AgentHandlers, AgentHeartbeatService, AgentLeaseService,
  AgentPoolHandlers, AgentRegistrationService, BuildConfigurationHandlers, BuildHandlers, DefinitionHandlers,
  ExactRevisionResolver, JobEventLongPoll, JobSpecToolchainPolicy, LeaseExpiryWorker, ManualTriggerService,
  PipelineHandlers, ProjectHandlers, StoreBackedEffectiveProjectPolicySource, StoreBackedManualTriggerContext,
};
use octacity_server_domain::Timestamp;
use octacity_server_store::WorkerOwner;
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};

use crate::{
  ServerConfig,
  readiness::{ReadinessChecks, ReadinessMonitor, ReadinessState, RuntimeDependencies, WorkerHealth},
};

/// Running server process and ownership handle for its cancellation tree.
#[must_use = "dropping the runtime aborts its listeners; call shutdown for graceful drain"]
pub struct ServerRuntime {
  management_addr: SocketAddr,
  agent_addr: Option<SocketAddr>,
  shutdown_grace: std::time::Duration,
  readiness: Arc<ReadinessState>,
  cancellation: CancellationToken,
  listener_tasks: JoinSet<ListenerTaskResult>,
  readiness_task: Option<JoinHandle<()>>,
  worker_task: Option<JoinHandle<()>>,
  notification_task: Option<JoinHandle<()>>,
}

type ListenerTaskResult = Result<&'static str, (&'static str, std::io::Error)>;

impl ServerRuntime {
  /// Binds the listener and starts supervised work from validated configuration.
  pub async fn start(config: ServerConfig) -> Result<Self, ServerRuntimeError> {
    let mut dependencies = RuntimeDependencies::from_config(&config)
      .await
      .map_err(ServerRuntimeError::ReadinessSetup)?;
    let registration_store = Arc::new(PostgresStore::new(dependencies.postgres.clone()));
    let notification_pool = dependencies.postgres.clone();
    let registration_service = Arc::new(
      AgentRegistrationService::new(registration_store.clone(), config.agent_registration_lifetime())
        .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?,
    );
    let placement_store = Arc::new(PostgresAuthoritativeStore::new(
      dependencies.postgres,
      dependencies.job_spec_signer,
    ));
    let management_application = management_application(
      registration_store.clone(),
      placement_store.clone(),
      dependencies.job_spec_toolchain,
      config.supported_pipeline_capabilities(),
      config.agent_enrollment_lifetime(),
      dependencies.agent_enrollment_secret_key,
    )?;
    let worker_health = Arc::new(WorkerHealth::new(
      config
        .lease_expiry_poll_interval()
        .saturating_add(config.lease_expiry_claim_lifetime())
        .saturating_add(config.readiness_check_interval()),
    ));
    dependencies.readiness.push(worker_health.clone());
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
      registration_store,
      config.agent_lease_lifetime(),
    )
    .map_err(|_| ServerRuntimeError::InvalidAgentPolicy)?;
    let api_config =
      AgentApiConfig::new(config.agent_max_retry_delay_milliseconds()).ok_or(ServerRuntimeError::InvalidAgentPolicy)?;
    Self::start_with_components(
      config,
      dependencies.readiness,
      agent_router(
        registration_service,
        Arc::new(lease_service),
        Arc::new(heartbeat_service),
        Arc::new(execution_service),
        api_config,
      ),
      Some(management_application),
      Some((expiry_worker, worker_health)),
      Some((notification_pool, ready_jobs)),
    )
    .await
  }

  /// Starts the process with concrete dependency and worker health checks.
  #[cfg(test)]
  pub(crate) async fn start_with_readiness(
    config: ServerConfig,
    checks: ReadinessChecks,
  ) -> Result<Self, ServerRuntimeError> {
    Self::start_with_components(config, checks, axum::Router::new(), None, None, None).await
  }

  async fn start_with_components(
    config: ServerConfig,
    checks: ReadinessChecks,
    agent_router: axum::Router,
    management_application: Option<ManagementApplication>,
    expiry_worker: Option<(LeaseExpiryWorker<PostgresAuthoritativeStore>, Arc<WorkerHealth>)>,
    ready_job_notifications: Option<(sqlx::PgPool, Arc<ReadyJobNotificationHub>)>,
  ) -> Result<Self, ServerRuntimeError> {
    let management_listener = bind_listener("management", config.management_bind()).await?;
    let agent_listener = bind_optional_listener("agent", config.agent_bind()).await?;
    let management_addr = inspect_listener("management", &management_listener)?;
    let agent_addr = agent_listener
      .as_ref()
      .map(|listener| inspect_listener("agent", listener))
      .transpose()?;
    let cancellation = CancellationToken::new();
    let monitor = ReadinessMonitor::start(
      checks,
      config.readiness_check_interval(),
      config.readiness_check_timeout(),
      cancellation.child_token(),
    )
    .await;
    let readiness = monitor.state();
    let readiness_task = monitor.into_task();
    let worker_task = expiry_worker.map(|(worker, health)| {
      spawn_expiry_worker(
        worker,
        health,
        config.lease_expiry_poll_interval(),
        config.lease_expiry_claim_lifetime(),
        cancellation.child_token(),
      )
    });
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
      false,
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
    info!(%management_addr, ?agent_addr, "server listeners ready");
    Ok(Self {
      management_addr,
      agent_addr,
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

  /// Waits until any configured ingress listener exits unexpectedly.
  ///
  /// Process entry points should select this future against their shutdown
  /// signal so a failed listener cannot leave an apparently healthy process.
  pub async fn wait(&mut self) -> Result<(), ServerRuntimeError> {
    let Some(result) = self.listener_tasks.join_next().await else {
      return Err(ServerRuntimeError::UnexpectedExit);
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
    match result {
      Ok(Ok(ingress)) => Err(ServerRuntimeError::ListenerUnexpectedExit { ingress }),
      Ok(Err((ingress, source))) => Err(ServerRuntimeError::Serve { ingress, source }),
      Err(source) => Err(ServerRuntimeError::ListenerTask(source)),
    }
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

fn management_application(
  store: Arc<PostgresStore>,
  authoritative_store: Arc<PostgresAuthoritativeStore>,
  toolchain: JobSpecToolchainPolicy,
  supported_pipeline_capabilities: &[String],
  agent_enrollment_lifetime: std::time::Duration,
  agent_enrollment_secret_key: octacity_server_application::AgentEnrollmentSecretKey,
) -> Result<ManagementApplication, ServerRuntimeError> {
  let projects = Arc::new(ProjectHandlers::new(store.clone()));
  let pipelines = Arc::new(PipelineHandlers::new(store.clone()));
  let configurations = Arc::new(BuildConfigurationHandlers::new(store.clone()));
  let pools = Arc::new(AgentPoolHandlers::new(store.clone()));
  let agents = Arc::new(AgentHandlers::new(store.clone()));
  let enrollments = Arc::new(AgentEnrollmentHandler::new(store.clone()));
  let builds = Arc::new(BuildHandlers::new(authoritative_store.clone()));
  let definitions = Arc::new(DefinitionHandlers::new(store.clone()));
  let policy_source = Arc::new(StoreBackedEffectiveProjectPolicySource::new(store.clone()));
  let trigger_context = Arc::new(StoreBackedManualTriggerContext::new(
    store.clone(),
    policy_source,
    toolchain,
  ));
  let manual_triggers = Arc::new(ManualTriggerService::new(
    authoritative_store,
    trigger_context,
    Arc::new(ExactRevisionResolver),
  ));
  let job_events = Arc::new(JobEventLongPoll::new(
    store,
    Arc::new(JobEventNotificationHub::default()),
  ));
  ManagementApplication::new(
    supported_pipeline_capabilities.iter().cloned(),
    agent_enrollment_lifetime,
    agent_enrollment_secret_key,
    ManagementApplicationHandlers::new(
      CatalogManagementApplication::new(
        ProjectManagementApplication::new(projects),
        PipelineManagementApplication::new(pipelines),
        ConfigurationManagementApplication::new(configurations),
        DefinitionManagementApplication::new(definitions),
      ),
      AgentManagementApplication::new(pools, agents, enrollments),
      ExecutionManagementApplication::new(
        BuildManagementApplication::new(builds),
        ManualTriggerManagementApplication::new(manual_triggers),
        JobEventManagementApplication::new(job_events),
      ),
    ),
  )
  .map_err(|_| ServerRuntimeError::InvalidManagementPolicy)
}

fn spawn_expiry_worker(
  worker: LeaseExpiryWorker<PostgresAuthoritativeStore>,
  health: Arc<WorkerHealth>,
  poll_interval: std::time::Duration,
  claim_lifetime: std::time::Duration,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  tokio::spawn(async move {
    struct UnhealthyOnDrop(Arc<WorkerHealth>);
    impl Drop for UnhealthyOnDrop {
      fn drop(&mut self) {
        self.0.mark_failure();
      }
    }
    let _unhealthy_on_drop = UnhealthyOnDrop(health.clone());
    loop {
      tokio::select! {
        biased;
        () = cancellation.cancelled() => return,
        () = tokio::time::sleep(poll_interval) => {}
      }
      let now_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok());
      let claim_millis = i64::try_from(claim_lifetime.as_millis()).ok();
      let Some((observed_at, claim_expires_at)) = now_millis
        .zip(claim_millis)
        .and_then(|(now, lifetime)| now.checked_add(lifetime).map(|until| (now, until)))
        .and_then(|(now, until)| {
          Timestamp::from_unix_millis(now)
            .ok()
            .zip(Timestamp::from_unix_millis(until).ok())
        })
      else {
        health.mark_failure();
        warn!("lease expiry worker could not represent the system clock");
        continue;
      };
      match tokio::time::timeout(claim_lifetime, worker.run_once(observed_at, claim_expires_at)).await {
        Ok(Ok(outcome)) => {
          health.mark_success();
          if outcome.claimed > 0 {
            info!(
              claimed = outcome.claimed,
              requeued = outcome.requeued,
              failed = outcome.failed,
              cancelled = outcome.cancelled,
              "expired leases recovered"
            );
          }
        }
        Ok(Err(error)) => {
          health.mark_failure();
          warn!(%error, "lease expiry worker pass failed");
        }
        Err(_) => {
          health.mark_failure();
          warn!(?claim_lifetime, "lease expiry worker pass timed out");
        }
      }
    }
  })
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
  /// The supervised lease-expiry worker panicked or was cancelled unexpectedly.
  #[error("lease expiry worker task failed: {0}")]
  WorkerTask(tokio::task::JoinError),
  /// The process-local PostgreSQL notification listener panicked or was cancelled unexpectedly.
  #[error("ready-Job notification task failed: {0}")]
  NotificationTask(tokio::task::JoinError),
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
mod tests {
  use async_trait::async_trait;
  use reqwest::StatusCode;

  use super::*;
  use crate::readiness::ReadinessCheck;

  struct HealthyCheck;

  #[async_trait]
  impl ReadinessCheck for HealthyCheck {
    fn name(&self) -> &'static str {
      "test-dependency"
    }

    async fn check(&self) -> bool {
      true
    }
  }

  fn healthy_checks() -> ReadinessChecks {
    let check = || Arc::new(HealthyCheck) as Arc<dyn ReadinessCheck>;
    ReadinessChecks::new(check(), check(), check(), check(), std::iter::empty())
  }

  fn test_config() -> ServerConfig {
    ServerConfig::parse_toml(
      r#"
management_bind = "127.0.0.1:0"
agent_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 1000
supported_pipeline_capabilities = ["native"]
readiness_check_interval_milliseconds = 10
readiness_check_timeout_milliseconds = 100

[postgres]
url_file = "postgres-url"

[object_storage]
endpoint = "http://127.0.0.1:9000"
region = "us-east-1"
bucket = "octacity-artifacts"
access_key_file = "object-access-key"
secret_key_file = "object-secret-key"

[signing]
key_id = "test-key"
key_file = "signing-key"

[agent_credentials]
enrollment_key_file = "agent-enrollment-key"

[job_spec]
policy_file = "job-spec-policy.json"
"#,
    )
    .unwrap()
  }

  #[tokio::test]
  async fn startup_separates_ingress_and_exposes_operational_metadata() {
    let runtime = ServerRuntime::start_with_readiness(test_config(), healthy_checks())
      .await
      .unwrap();
    let client = reqwest::Client::new();
    let origin = format!("http://{}", runtime.management_addr());

    for (path, expected_body) in [
      ("/health/live", r#"{"status":"live"}"#),
      ("/health/ready", r#"{"status":"ready"}"#),
    ] {
      let response = client.get(format!("{origin}{path}")).send().await.unwrap();
      assert_eq!(response.status(), StatusCode::OK);
      assert!(response.headers().contains_key("x-request-id"));
      assert_eq!(response.text().await.unwrap(), expected_body);
    }

    let metadata: serde_json::Value = client
      .get(format!("{origin}/api/v1/operations/metadata"))
      .send()
      .await
      .unwrap()
      .json()
      .await
      .unwrap();
    assert_eq!(
      metadata["security"]["management"]["mode"],
      "trusted_network_unauthenticated"
    );
    assert_eq!(metadata["security"]["management"]["operator_authentication"], false);
    assert_eq!(metadata["security"]["agent"]["authentication_required"], true);
    assert_eq!(metadata["security"]["webhook"]["authentication_required"], true);
    assert_eq!(metadata["ingress"]["agent_enabled"], true);
    assert_eq!(metadata["ingress"]["webhook_enabled"], false);
    assert_eq!(metadata["ingress"]["listeners_separate"], true);

    let response = client
      .get(format!("http://{}/health/live", runtime.agent_addr().unwrap()))
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    runtime.shutdown().await.unwrap();
  }

  #[tokio::test]
  async fn shutdown_cancels_the_listener_and_waits_for_its_task() {
    let runtime = ServerRuntime::start_with_readiness(test_config(), healthy_checks())
      .await
      .unwrap();
    let addresses = [runtime.management_addr(), runtime.agent_addr().unwrap()];
    for address in addresses {
      assert!(tokio::net::TcpListener::bind(address).await.is_err());
    }

    runtime.shutdown().await.unwrap();

    for address in addresses {
      let replacement = tokio::net::TcpListener::bind(address)
        .await
        .expect("shutdown must release every ingress listener");
      assert_eq!(replacement.local_addr().unwrap(), address);
    }
  }

  #[tokio::test]
  async fn wait_reports_a_listener_that_stops_without_shutdown() {
    let cancellation = CancellationToken::new();
    let readiness_cancellation = cancellation.child_token();
    let mut listener_tasks = JoinSet::new();
    listener_tasks.spawn(async { Ok("management") });
    let mut runtime = ServerRuntime {
      management_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
      agent_addr: None,
      shutdown_grace: std::time::Duration::from_secs(1),
      readiness: Arc::new(ReadinessState::default()),
      cancellation,
      listener_tasks,
      readiness_task: Some(tokio::spawn(async move {
        readiness_cancellation.cancelled().await;
      })),
      worker_task: None,
      notification_task: None,
    };

    assert!(matches!(
      runtime.wait().await,
      Err(ServerRuntimeError::ListenerUnexpectedExit { ingress: "management" })
    ));
  }

  #[tokio::test]
  async fn dropping_runtime_aborts_owned_listener_work() {
    struct DropSignal(Option<tokio::sync::oneshot::Sender<()>>);

    impl Drop for DropSignal {
      fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
          let _ = sender.send(());
        }
      }
    }

    let (started_sender, started_receiver) = tokio::sync::oneshot::channel();
    let (dropped_sender, dropped_receiver) = tokio::sync::oneshot::channel();
    let mut listener_tasks = JoinSet::new();
    listener_tasks.spawn(async move {
      let _drop_signal = DropSignal(Some(dropped_sender));
      let _ = started_sender.send(());
      std::future::pending::<()>().await;
      Ok("management")
    });
    let runtime = ServerRuntime {
      management_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
      agent_addr: None,
      shutdown_grace: std::time::Duration::from_secs(1),
      readiness: Arc::new(ReadinessState::default()),
      cancellation: CancellationToken::new(),
      listener_tasks,
      readiness_task: Some(tokio::spawn(std::future::pending())),
      worker_task: None,
      notification_task: None,
    };

    started_receiver.await.unwrap();
    drop(runtime);
    tokio::time::timeout(std::time::Duration::from_secs(1), dropped_receiver)
      .await
      .expect("aborted listener future must be dropped")
      .unwrap();
  }
}
