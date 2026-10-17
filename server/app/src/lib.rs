//! Composition root and process lifecycle for the OctaCity server.
//!
//! This crate is the only server crate allowed to select concrete adapters.
//! The initial runtime exposes bounded health probes and operational metadata;
//! it deliberately registers no domain command or query routes.
//!
//! Crate ownership, dependency direction, and public protocol seams are mapped
//! in the [server architecture guide].
//!
//! [server architecture guide]: https://github.com/OctaHive/OctaCity/blob/main/docs/architecture/server.md

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod config;
mod readiness;
mod runtime;

pub use config::{FactoryRuntimeConfig, ServerConfig, ServerConfigError};
pub use runtime::{
  DurableWorkerError, FactoryDeliveryInputs, FactoryDeliveryMaterializer, FactoryDeliveryPolicy,
  FactoryDeliveryPolicyDraft, FactoryDeliveryTargetPolicy, FactoryProfileRuntimeInstaller, FactoryRuntimeInstallation,
  FactoryRuntimeInstallationError, FactoryRuntimeInstaller, FactoryRuntimeProfileCatalog,
  FactoryRuntimeProtectedValidationProfile, FactoryRuntimeReadiness, FactoryRuntimeResources,
  FactoryRuntimeSignalConnector, FactoryRuntimeSignalProfile, FactoryRuntimeSignalReadiness,
  HostedFactoryCandidatePublisher, LogSearchMaintenanceError, RestoreMaintenanceError, RestoredStateSummary,
  RuntimeAssemblyError, ServerRuntime, ServerRuntimeError, load_factory_signal_connectors, rebuild_log_search,
  reconcile_restored_state, validate_runtime_files,
};

pub use runtime::{
  FactoryReporterProfile, FactoryReporterProfileDraft, FactorySourceInstallation, FactorySourceProfile,
  FactorySourceRuntime, FactorySourceWorker, FactorySourceWorkerDependencies, FactorySourceWorkerOutcome,
  FactorySourceWorkerSettings, HostedFactoryWorkStatusProjector,
};

pub use runtime::{HostedIntegrationConnectionChecker, IntegrationCheckProfile};
