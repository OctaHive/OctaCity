use async_trait::async_trait;
use octacity_server_application::{AgentTelemetryBatch, AgentTelemetryExportError, AgentTelemetryExporter};

/// Explicit null adapter used until a diagnostic exporter is configured.
pub(super) struct UnavailableAgentTelemetryExporter;

#[async_trait]
impl AgentTelemetryExporter for UnavailableAgentTelemetryExporter {
  async fn export(&self, _batch: AgentTelemetryBatch) -> Result<(), AgentTelemetryExportError> {
    Err(AgentTelemetryExportError)
  }
}
