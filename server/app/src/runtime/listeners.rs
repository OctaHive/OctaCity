use std::{net::SocketAddr, sync::Arc};

use octacity_server_api_agent::ReadyJobNotificationHub;
use octacity_server_api_rest::v1::ManagementApplication;
use tokio::{net::TcpListener, task::JoinSet};
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::{
  ListenerTaskResult, ServerRuntime, ServerRuntimeError,
  notifications::spawn_ready_job_listener,
  telemetry::spawn_metrics_upkeep,
  workers::{self, DurableWorkers},
};
use crate::{
  ServerConfig,
  readiness::{ReadinessChecks, ReadinessMonitor},
};

pub(super) struct RuntimeComponents {
  pub(super) agent_router: axum::Router,
  pub(super) cache_router: Option<axum::Router>,
  pub(super) management_application: Option<ManagementApplication>,
  pub(super) webhook_router: Option<axum::Router>,
  pub(super) workers: Option<DurableWorkers>,
  pub(super) ready_job_notifications: Option<(sqlx::PgPool, Arc<ReadyJobNotificationHub>)>,
  pub(super) metrics: Option<metrics_exporter_prometheus::PrometheusHandle>,
  pub(super) cancellation: CancellationToken,
}

impl ServerRuntime {
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
        cache_router: None,
        management_application: None,
        webhook_router: None,
        workers: None,
        ready_job_notifications: None,
        metrics: None,
        cancellation: CancellationToken::new(),
      },
    )
    .await
  }

  pub(super) async fn start_with_components(
    config: ServerConfig,
    checks: ReadinessChecks,
    components: RuntimeComponents,
  ) -> Result<Self, ServerRuntimeError> {
    let RuntimeComponents {
      agent_router,
      cache_router,
      management_application,
      webhook_router,
      workers,
      ready_job_notifications,
      metrics,
      cancellation,
    } = components;
    if config.webhook_bind().is_some() != webhook_router.is_some() {
      return Err(ServerRuntimeError::InvalidManagementPolicy);
    }
    if config.cache_bind().is_some() != cache_router.is_some() {
      return Err(ServerRuntimeError::InvalidManagementPolicy);
    }
    let management_listener = bind_listener("management", config.management_bind()).await?;
    let agent_listener = bind_optional_listener("agent", config.agent_bind()).await?;
    let cache_listener = bind_optional_listener("cache", config.cache_bind()).await?;
    let webhook_listener = bind_optional_listener("webhook", config.webhook_bind()).await?;
    let management_addr = inspect_listener("management", &management_listener)?;
    let agent_addr = agent_listener
      .as_ref()
      .map(|listener| inspect_listener("agent", listener))
      .transpose()?;
    let cache_addr = cache_listener
      .as_ref()
      .map(|listener| inspect_listener("cache", listener))
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
        config.readiness_check_interval(),
        cancellation.child_token(),
      )
    });
    let metrics_task = metrics
      .as_ref()
      .map(|handle| spawn_metrics_upkeep(handle.clone(), cancellation.child_token()));
    let router_readiness = readiness.clone();
    let metadata = octacity_server_api_rest::v1::OperationalMetadata::trusted_network(
      config.management_externally_reachable(),
      config.unauthenticated_management_acknowledged(),
      agent_addr.is_some(),
      webhook_addr.is_some(),
    );
    let management_router = match (management_application, metrics) {
      (Some(application), Some(metrics)) => {
        octacity_server_api_rest::management_router_with_application_metadata_and_metrics(
          move || router_readiness.is_ready(),
          application,
          metadata,
          move || metrics.render(),
        )
      }
      (Some(application), None) => octacity_server_api_rest::management_router_with_application_and_metadata(
        move || router_readiness.is_ready(),
        application,
        metadata,
      ),
      (None, _) => {
        octacity_server_api_rest::management_router_with_metadata(move || router_readiness.is_ready(), metadata)
      }
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
    if let (Some(listener), Some(router)) = (cache_listener, cache_router) {
      spawn_listener(
        &mut listener_tasks,
        "cache",
        listener,
        router,
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
    info!(%management_addr, ?agent_addr, ?cache_addr, ?webhook_addr, "server listeners ready");
    Ok(Self {
      management_addr,
      agent_addr,
      cache_addr,
      webhook_addr,
      shutdown_grace: config.shutdown_grace(),
      readiness,
      cancellation,
      listener_tasks,
      readiness_task: Some(readiness_task),
      worker_task,
      notification_task,
      metrics_task,
    })
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
