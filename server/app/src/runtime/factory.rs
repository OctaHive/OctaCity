//! Optional configurable owner and explicit deployment installation boundary.
use async_trait::async_trait;
use octacity_server_application::{FactoryFlowInputPreparer, FactoryNodeExecutorBinding, JobSpecToolchainPolicy};
use octacity_server_store_postgres::{FactoryCapabilitySource, PostgresAuthoritativeStore, PostgresStore};
use std::sync::Arc;

/// Runtime-owned resources supplied to trusted installed capability compilers.
/// No task-authored routing or mutable provider aliases select an installation.
pub struct FactoryRuntimeResources {
  /// Common durable Run, configuration, input and result journal.
  pub runs: Arc<PostgresStore>,
  /// Ordinary atomic signed Build execution and cancellation.
  pub builds: Arc<PostgresAuthoritativeStore>,
  /// Bounded retained protected inputs and ordinary execution outputs.
  pub objects: Arc<octacity_artifact_s3::S3ArtifactStore>,
  /// Exact server-approved source and Octa toolchain snapshot.
  pub toolchain: JobSpecToolchainPolicy,
}

/// Required availability of installed Factory capabilities, evaluated only in enabled mode.
#[async_trait]
pub trait FactoryRuntimeReadiness: Send + Sync {
  /// Reports availability without disclosing provider data or credentials.
  async fn available(&self) -> bool;
}

/// Verified deployment components for the common graph owner.
/// Executors are selected by exact immutable identities, never logical node names.
pub struct FactoryRuntimeInstallation {
  /// Current Project-scoped choices used for new configuration publication.
  pub capabilities: Arc<dyn FactoryCapabilitySource>,
  /// Unique exact installed bindings for effectful primitives.
  pub executors: Vec<FactoryNodeExecutorBinding>,
  /// Trusted owner of configured projections, selected Context and retained bytes.
  pub inputs: Arc<dyn FactoryFlowInputPreparer>,
  /// Availability of the dependencies required by this deployment.
  pub readiness: Arc<dyn FactoryRuntimeReadiness>,
}

/// Secret-safe deployment installation failure.
#[derive(Debug, thiserror::Error)]
pub enum FactoryRuntimeInstallationError {
  /// A binding or protected execution policy cannot be verified.
  #[error("Factory installation is invalid")]
  Invalid,
  /// Required installed material is not currently available.
  #[error("Factory installation is unavailable")]
  Unavailable,
}

/// Explicit deployment extension seam for trusted capability implementations.
/// Ordinary startup never constructs or probes an installer when Factory is disabled.
#[async_trait]
pub trait FactoryRuntimeInstaller: Send + Sync {
  /// Resolves exact capability bindings from runtime-owned resources.
  async fn install(
    &self,
    resources: FactoryRuntimeResources,
  ) -> Result<FactoryRuntimeInstallation, FactoryRuntimeInstallationError>;
}

pub(super) struct CapabilityReadiness(pub(super) Arc<dyn FactoryRuntimeReadiness>);
#[async_trait]
impl crate::readiness::ReadinessCheck for CapabilityReadiness {
  fn name(&self) -> &'static str {
    "factory-capabilities"
  }
  async fn check(&self) -> bool {
    self.0.available().await
  }
}

/// Worker plus health tracked by the existing durable supervisor.
pub(super) type InstalledFactoryWorker = (
  octacity_server_application::FactoryReconciler<PostgresStore>,
  Arc<crate::readiness::WorkerHealth>,
);

pub(super) async fn assemble(
  config: &crate::ServerConfig,
  resources: &mut super::assembly::RuntimeResources,
  placement_store: Arc<PostgresAuthoritativeStore>,
  installer: Option<Arc<dyn FactoryRuntimeInstaller>>,
) -> Result<(Arc<PostgresStore>, Option<InstalledFactoryWorker>), super::ServerRuntimeError> {
  use super::ServerRuntimeError;
  use crate::readiness::WorkerHealth;
  let installed = match (config.factory(), installer) {
    (Some(_), Some(installer)) => Some(
      tokio::time::timeout(
        config.readiness_check_timeout(),
        installer.install(FactoryRuntimeResources {
          runs: Arc::new(PostgresStore::new(resources.postgres.clone())),
          builds: placement_store.clone(),
          objects: resources.object_storage.clone(),
          toolchain: resources.job_spec_toolchain.clone(),
        }),
      )
      .await
      .map_err(|_| ServerRuntimeError::InvalidFactoryInstallation)?
      .map_err(|_| ServerRuntimeError::InvalidFactoryInstallation)?,
    ),
    (None, _) => None,
    (Some(_), None) => unreachable!("installation requirement checked before loading resources"),
  };
  let registration_store = Arc::new(match &installed {
    Some(installed) => {
      PostgresStore::with_factory_capabilities(resources.postgres.clone(), installed.capabilities.clone())
    }
    None => PostgresStore::new(resources.postgres.clone()),
  });
  let factory = match (config.factory(), installed) {
    (Some(policy), Some(installed)) => {
      let health = Arc::new(WorkerHealth::new(
        super::workers::FACTORY_WORKER_NAME,
        policy
          .poll_interval()
          .saturating_add(policy.claim_lifetime())
          .saturating_add(config.readiness_check_interval()),
      ));
      resources.readiness.push(health.clone());
      resources
        .readiness
        .push(Arc::new(CapabilityReadiness(installed.readiness)));
      let executors = Arc::new(
        octacity_server_application::FactoryNodeExecutorRegistry::new(installed.executors)
          .map_err(|_| ServerRuntimeError::InvalidFactoryInstallation)?,
      );
      let flow = Arc::new(octacity_server_application::FactoryFlowRunner::new(
        registration_store.clone(),
        executors,
        installed.inputs,
        Arc::new(octacity_server_application::SystemFactoryExecutionClock),
      ));
      let worker = octacity_server_application::FactoryReconciler::new(
        registration_store.clone(),
        octacity_server_factory::FactoryKey::new(format!("factory-owner.{}", uuid::Uuid::new_v4()))
          .map_err(|_| ServerRuntimeError::InvalidFactoryInstallation)?,
        policy.batch_size(),
        policy.concurrency(),
        flow,
      )
      .map_err(|_| ServerRuntimeError::InvalidFactoryInstallation)?;
      Some((worker, health))
    }
    (None, None) => None,
    _ => unreachable!("Factory policy and installation are paired"),
  };
  Ok((registration_store, factory))
}
