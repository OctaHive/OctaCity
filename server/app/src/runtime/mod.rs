use std::{net::SocketAddr, sync::Arc};

use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

use crate::readiness::ReadinessState;

mod admission;
mod application;
mod assembly;
mod composition;
mod delivery;
mod error;
mod factory;
mod integrations;
mod issues;
mod lifecycle;
mod listeners;
#[cfg(unix)]
mod local_management;
mod maintenance;
mod notifications;
mod reporting;
mod sources;
mod telemetry;
mod vcs;
mod webhook;
mod workers;

pub use assembly::{RuntimeAssemblyError, validate_runtime_files};
pub use delivery::delivery_material::RetainedFactoryDeliveryMaterializer;
pub use delivery::{
  FactoryDeliveryInputs, FactoryDeliveryMaterializer, FactoryDeliveryPolicy, FactoryDeliveryPolicyDraft,
  FactoryDeliveryTargetPolicy, HostedFactoryCandidatePublisher,
};
pub use error::{DurableWorkerError, ServerRuntimeError};
pub use factory::factory_command_runtime::FactoryRuntimeProtectedValidationProfile;
pub use factory::factory_connector_file::{
  FactoryRuntimeSignalConnector, FactoryRuntimeSignalReadiness, load_factory_signal_connectors,
};
pub use factory::factory_profiles::{
  FactoryProfileRuntimeInstaller, FactoryRuntimeProfileCatalog, FactoryRuntimeSignalProfile,
};
pub use factory::{
  FactoryRuntimeInstallation, FactoryRuntimeInstallationError, FactoryRuntimeInstaller, FactoryRuntimeReadiness,
  FactoryRuntimeResources,
};
pub use integrations::integration_check::{HostedIntegrationConnectionChecker, IntegrationCheckProfile};
pub use issues::issue_code_review_reader::{
  HostedIssueCodeReviewObservations, IssueCodeReviewReadProfile, ReadIssueCodeReviewChecks,
};
pub use maintenance::{
  LogSearchMaintenanceError, RestoreMaintenanceError, RestoredStateSummary, rebuild_log_search,
  reconcile_restored_state,
};
pub use reporting::worker::{FactoryReporterProfile, FactoryReporterProfileDraft, HostedFactoryWorkStatusProjector};
pub use sources::worker::*;

/// Independent optional Issue integrations. Empty installations preserve ordinary manual tasks.
#[derive(Default)]
pub struct IssueRuntimeIntegrations {
  /// Existing ticket Source reads and mappings, without requiring a review provider.
  pub sources: Option<IssueSourceRuntimeInstallation>,
  /// Existing read-only review observation port, without requiring a ticket Source or Factory.
  pub code_reviews: Option<Arc<dyn octacity_server_application::IssueCodeReviewObservations>>,
}

/// Explicit task Source installation, usable without a Factory target or worker.
/// The reader owns current credential/grant checks; mappings expose only finite safe choices.
/// Construction and server startup perform no Source read or Reporter dispatch.
pub struct IssueSourceRuntimeInstallation {
  /// Existing authorized, bounded Source read port.
  pub reader: Arc<dyn octacity_server_application::WorkSourceObservations>,
  /// Existing finite Source read budgets, retry cadence and crash-recoverable ownership lease.
  pub refresh_policy: octacity_server_application::IssueSourceRefreshPolicy,
  /// Exact installed field/status capabilities, independent of workflow administration.
  pub mappings: Vec<octacity_server_application::InstalledIssueSourceMapping>,
}

type ListenerTaskResult = Result<&'static str, (&'static str, std::io::Error)>;

/// Running server process and ownership handle for its cancellation tree.
#[must_use = "dropping the runtime aborts its listeners; call shutdown for graceful drain"]
pub struct ServerRuntime {
  management_addr: SocketAddr,
  agent_addr: Option<SocketAddr>,
  cache_addr: Option<SocketAddr>,
  webhook_addr: Option<SocketAddr>,
  shutdown_grace: std::time::Duration,
  readiness: Arc<ReadinessState>,
  cancellation: CancellationToken,
  listener_tasks: JoinSet<ListenerTaskResult>,
  readiness_task: Option<JoinHandle<()>>,
  worker_task: Option<JoinHandle<Result<(), DurableWorkerError>>>,
  notification_task: Option<JoinHandle<()>>,
  metrics_task: Option<JoinHandle<()>>,
}

impl ServerRuntime {
  /// Actual bound management address, including an OS-assigned port.
  pub const fn management_addr(&self) -> SocketAddr {
    self.management_addr
  }

  /// Actual bound Agent address when authenticated Agent ingress is configured.
  pub const fn agent_addr(&self) -> Option<SocketAddr> {
    self.agent_addr
  }

  /// Actual bound cache address when authenticated Octa cache ingress is configured.
  pub const fn cache_addr(&self) -> Option<SocketAddr> {
    self.cache_addr
  }

  /// Actual bound webhook address when authenticated webhook ingress is configured.
  pub const fn webhook_addr(&self) -> Option<SocketAddr> {
    self.webhook_addr
  }
}

#[cfg(test)]
mod tests;

// Shared fixtures are loaded once for workflow, ingress and integration tests.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod postgres_test_support;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../tests/support/runtime.rs"]
mod test_runtime_config;
