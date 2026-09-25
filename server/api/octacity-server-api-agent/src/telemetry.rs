//! Bounded best-effort Agent telemetry HTTP ingress.

use std::{sync::Arc, time::Duration};

use axum::{
  Json,
  body::Body,
  extract::{FromRequest, Request, State},
  http::StatusCode,
  response::{IntoResponse, Response},
};
use octacity_protocol::{COORDINATOR_PROTOCOL_VERSION, IngestAgentTelemetryRequest, IngestAgentTelemetryResponse};
use octacity_server_application::{
  AgentTelemetryBatch, AgentTelemetryError, AgentTelemetryExporter, AgentTelemetryInput, AgentTelemetryUseCases,
};
use tokio::{sync::Semaphore, time::timeout};

use super::{AgentState, operation_context, operation_context_error, protocol_error, rejection_response};

/// Server-owned memory and latency limits for best-effort telemetry export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentTelemetryPolicy {
  max_in_flight_exports: usize,
  export_timeout: Duration,
}

impl AgentTelemetryPolicy {
  /// Validates a non-zero exporter concurrency and latency budget.
  pub const fn new(max_in_flight_exports: usize, export_timeout: Duration) -> Option<Self> {
    if max_in_flight_exports == 0 || export_timeout.is_zero() {
      None
    } else {
      Some(Self {
        max_in_flight_exports,
        export_timeout,
      })
    }
  }

  /// Maximum batches concurrently retained by exporter futures.
  pub const fn max_in_flight_exports(self) -> usize {
    self.max_in_flight_exports
  }

  /// Maximum time allowed for one exporter call.
  pub const fn export_timeout(self) -> Duration {
    self.export_timeout
  }
}

/// Authenticated application boundary plus a bounded exporter gate.
#[derive(Clone)]
pub struct AgentTelemetryIngress {
  application: Arc<dyn AgentTelemetryUseCases>,
  exporter: Arc<dyn AgentTelemetryExporter>,
  permits: Arc<Semaphore>,
  export_timeout: Duration,
}

impl AgentTelemetryIngress {
  /// Creates an ingress with no waiting queue beyond active exporter futures.
  pub fn new(
    application: Arc<dyn AgentTelemetryUseCases>,
    exporter: Arc<dyn AgentTelemetryExporter>,
    policy: AgentTelemetryPolicy,
  ) -> Self {
    Self {
      application,
      exporter,
      permits: Arc::new(Semaphore::new(policy.max_in_flight_exports)),
      export_timeout: policy.export_timeout,
    }
  }

  async fn export(&self, batch: AgentTelemetryBatch) -> (u16, u16) {
    let sample_count = batch.sample_count();
    let Ok(_permit) = Arc::clone(&self.permits).try_acquire_owned() else {
      return (0, sample_count);
    };
    match timeout(self.export_timeout, self.exporter.export(batch)).await {
      Ok(Ok(())) => (sample_count, 0),
      Ok(Err(_)) | Err(_) => (0, sample_count),
    }
  }
}

pub(super) async fn ingest(State(state): State<AgentState>, request: Request<Body>) -> Response {
  let headers = request.headers().clone();
  let parsed = match Json::<IngestAgentTelemetryRequest>::from_request(request, &()).await {
    Ok(Json(request)) => request,
    Err(rejection) => return rejection_response(rejection),
  };
  let request_id = parsed.request_id.clone();
  let (credential, observed_at_unix_ms) = match operation_context(&headers, &request_id) {
    Ok(context) => context,
    Err(error) => return operation_context_error(&request_id, error),
  };
  let batch = match state
    .telemetry
    .application
    .authorize_ingest(AgentTelemetryInput {
      request: parsed,
      credential,
      observed_at_unix_ms,
    })
    .await
  {
    Ok(batch) => batch,
    Err(error) => return telemetry_error(&request_id, error),
  };
  let (accepted_samples, dropped_samples) = state.telemetry.export(batch).await;
  Json(IngestAgentTelemetryResponse {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id,
    accepted_samples,
    dropped_samples,
  })
  .into_response()
}

fn telemetry_error(request_id: &str, error: AgentTelemetryError) -> Response {
  match error {
    AgentTelemetryError::InvalidRequest => protocol_error(
      StatusCode::BAD_REQUEST,
      request_id,
      "invalid_request",
      "agent telemetry request is invalid",
      false,
    ),
    AgentTelemetryError::CredentialRejected => protocol_error(
      StatusCode::UNAUTHORIZED,
      request_id,
      "credential_rejected",
      "agent credential rejected",
      false,
    ),
    AgentTelemetryError::Fenced => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "registration_fenced",
      "agent registration is no longer current",
      false,
    ),
    AgentTelemetryError::Unavailable => protocol_error(
      StatusCode::SERVICE_UNAVAILABLE,
      request_id,
      "unavailable",
      "agent telemetry authorization unavailable",
      true,
    ),
  }
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::{AtomicUsize, Ordering};

  use async_trait::async_trait;
  use octacity_server_application::AgentTelemetryExportError;
  use tokio::sync::Notify;

  use super::*;

  struct UnusedTelemetryApplication;

  #[async_trait]
  impl AgentTelemetryUseCases for UnusedTelemetryApplication {
    async fn authorize_ingest(&self, _input: AgentTelemetryInput) -> Result<AgentTelemetryBatch, AgentTelemetryError> {
      unreachable!("exporter bound test starts with an authenticated batch")
    }
  }

  struct BlockingExporter {
    active: AtomicUsize,
    maximum_active: AtomicUsize,
    entered: Notify,
    release: Notify,
  }

  struct NeverReturningExporter;

  #[async_trait]
  impl AgentTelemetryExporter for NeverReturningExporter {
    async fn export(&self, _batch: AgentTelemetryBatch) -> Result<(), AgentTelemetryExportError> {
      std::future::pending().await
    }
  }

  #[async_trait]
  impl AgentTelemetryExporter for BlockingExporter {
    async fn export(&self, _batch: AgentTelemetryBatch) -> Result<(), AgentTelemetryExportError> {
      let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
      self.maximum_active.fetch_max(active, Ordering::SeqCst);
      self.entered.notify_one();
      self.release.notified().await;
      self.active.fetch_sub(1, Ordering::SeqCst);
      Ok(())
    }
  }

  #[tokio::test]
  async fn saturated_exporter_has_no_waiting_batch_queue() {
    let exporter = Arc::new(BlockingExporter {
      active: AtomicUsize::new(0),
      maximum_active: AtomicUsize::new(0),
      entered: Notify::new(),
      release: Notify::new(),
    });
    let ingress = Arc::new(AgentTelemetryIngress::new(
      Arc::new(UnusedTelemetryApplication),
      exporter.clone(),
      AgentTelemetryPolicy::new(1, Duration::from_secs(5)).unwrap(),
    ));
    let first = tokio::spawn({
      let ingress = Arc::clone(&ingress);
      async move { ingress.export(batch("first")).await }
    });
    exporter.entered.notified().await;

    for request_id in ["second", "third", "fourth"] {
      assert_eq!(ingress.export(batch(request_id)).await, (0, 1));
    }
    assert_eq!(exporter.active.load(Ordering::SeqCst), 1);
    assert_eq!(exporter.maximum_active.load(Ordering::SeqCst), 1);

    exporter.release.notify_one();
    assert_eq!(first.await.unwrap(), (1, 0));
  }

  #[tokio::test]
  async fn exporter_timeout_releases_the_in_flight_budget() {
    let ingress = AgentTelemetryIngress::new(
      Arc::new(UnusedTelemetryApplication),
      Arc::new(NeverReturningExporter),
      AgentTelemetryPolicy::new(1, Duration::from_millis(1)).unwrap(),
    );

    assert_eq!(ingress.export(batch("first")).await, (0, 1));
    assert_eq!(ingress.export(batch("second")).await, (0, 1));
  }

  fn batch(request_id: &str) -> AgentTelemetryBatch {
    AgentTelemetryBatch::try_from_request(octacity_protocol::IngestAgentTelemetryRequest {
      protocol_version: octacity_protocol::COORDINATOR_PROTOCOL_VERSION,
      request_id: request_id.to_owned(),
      registration_id: "registration".to_owned(),
      samples: vec![octacity_protocol::AgentTelemetrySample {
        observed_at_unix_ms: 1,
        runtime: octacity_protocol::AgentTelemetryRuntime::Native,
        isolation: octacity_protocol::AgentTelemetryIsolation::Native,
        cpu_time_ms: 1,
        memory_current_bytes: 2,
        io_read_bytes: 3,
        io_written_bytes: 4,
        network_received_bytes: None,
        network_transmitted_bytes: None,
      }],
    })
    .unwrap()
  }
}
