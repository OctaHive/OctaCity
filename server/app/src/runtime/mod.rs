use std::{net::SocketAddr, sync::Arc};

use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

use crate::readiness::ReadinessState;

mod admission;
mod application;
mod assembly;
mod composition;
mod delivery;
mod delivery_material;
pub use delivery_material::RetainedFactoryDeliveryMaterializer;
mod error;
mod factory;
mod factory_command_runtime;
pub use factory_command_runtime::FactoryRuntimeProtectedValidationProfile;
mod factory_connector_file;
pub use factory_connector_file::{
  FactoryRuntimeSignalConnector, FactoryRuntimeSignalReadiness, load_factory_signal_connectors,
};
mod factory_model_catalog;
mod factory_profiles;
mod factory_signal_provider;
pub use factory_profiles::{FactoryProfileRuntimeInstaller, FactoryRuntimeProfileCatalog, FactoryRuntimeSignalProfile};
#[cfg(test)]
mod factory_tests;
pub use delivery::{
  FactoryDeliveryInputs, FactoryDeliveryMaterializer, FactoryDeliveryPolicy, FactoryDeliveryPolicyDraft,
  FactoryDeliveryTargetPolicy, HostedFactoryCandidatePublisher,
};
#[cfg(all(test, unix))]
mod delivery_material_tests;
#[cfg(test)]
mod delivery_tests;
pub use factory::{
  FactoryRuntimeInstallation, FactoryRuntimeInstallationError, FactoryRuntimeInstaller, FactoryRuntimeReadiness,
  FactoryRuntimeResources,
};
mod factory_profile_file;
mod integration_check;
mod integration_management;
#[cfg(all(test, unix))]
mod integration_management_tests;
mod issue_mcp;
mod issue_source_worker;
#[cfg(test)]
mod issue_tests;
#[cfg(test)]
mod issue_workflow_tests;
mod managed_factory_source;
#[cfg(all(test, unix))]
mod source_factory_predicate_tests;
#[cfg(all(test, unix))]
mod source_factory_refresh_tests;
#[cfg(all(test, unix))]
mod source_factory_restart_tests;
#[cfg(all(test, unix))]
mod source_factory_tests;
#[cfg(all(test, unix))]
mod source_import_tests;
#[cfg(all(test, unix))]
mod source_lifecycle_tests;
mod source_management;
#[cfg(all(test, unix))]
mod source_management_tests;
#[cfg(all(test, unix))]
mod source_overlap_tests;
mod source_profiles_file;
#[cfg(all(test, unix))]
mod source_profiles_file_tests;
mod source_reporter_actions;
mod source_reporting;
#[cfg(all(test, unix))]
mod source_restart_tests;
pub use integration_check::{HostedIntegrationConnectionChecker, IntegrationCheckProfile};
mod issue_code_review_reader;
mod lifecycle;
mod listeners;
#[cfg(unix)]
mod local_management;
mod maintenance;
mod notifications;
mod telemetry;
mod vcs;
mod webhook;
mod work_integration_read;
mod work_reporter;
mod work_source;
pub use issue_code_review_reader::{
  HostedIssueCodeReviewObservations, IssueCodeReviewReadProfile, ReadIssueCodeReviewChecks,
};
pub use work_reporter::{FactoryReporterProfile, FactoryReporterProfileDraft, HostedFactoryWorkStatusProjector};
#[cfg(test)]
mod work_reporter_tests;
mod workers;
pub use work_source::*;
#[cfg(all(test, unix))]
mod work_source_tests;

pub use assembly::{RuntimeAssemblyError, validate_runtime_files};
pub use error::{DurableWorkerError, ServerRuntimeError};
pub use maintenance::{
  LogSearchMaintenanceError, RestoreMaintenanceError, RestoredStateSummary, rebuild_log_search,
  reconcile_restored_state,
};

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

#[cfg(test)]
mod factory_profile_file_tests;

#[cfg(test)]
mod factory_command_runtime_tests;
mod factory_delivery_file;
#[cfg(test)]
mod factory_evaluation_runtime_tests;

#[cfg(all(test, unix))]
mod integration_check_tests;

// Shared fixtures are loaded once for workflow, ingress and integration tests.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod postgres_test_support;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../tests/support/runtime.rs"]
mod test_runtime_config;

#[cfg(all(test, unix))]
mod factory_model_catalog_tests;
