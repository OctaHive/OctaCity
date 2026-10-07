//! Provider-neutral and legacy execution intent normalized for orchestration.

use std::{collections::BTreeSet, sync::Arc};

use octacity_cache_session::CacheExecutionIdentity;
use octacity_execution::{
  ExecutionArchitecture, ExecutionBackend, ExecutionOs, ExecutionPlatform, ExecutionTarget,
  OciIsolation as ExecutionOciIsolation,
};
use octacity_protocol::{
  CachePolicy, ExecutionCacheIdentityV2, ExecutionCapabilityV2, ExecutionEnvironmentId, ExecutionEvidenceV2,
  ExecutionMode, ExecutionSpec, FactoryEnforcementCapabilityV3, JobSpecV1, JobSpecV2, JobSpecV3, NetworkPolicy,
  OciIsolation as ProtocolOciIsolation, OctaSpec, OutputLimits, PlatformArchitecture, PlatformOs, RuntimeSpec,
  RuntimeSpecV2, RuntimeTarget, SourceSpec, VerifiedJobSpec,
};

use crate::{JobError, factory_preflight::ManagedFactoryExecution};

/// One operator-selected provider route for negotiated execution contract v2.
pub struct ExecutionBackendRoute {
  pub(super) capability: ExecutionCapabilityV2,
  pub(super) environment: ExecutionEnvironmentId,
  pub(super) backend: Arc<dyn ExecutionBackend>,
  pub(super) factory_enforcement: BTreeSet<FactoryEnforcementCapabilityV3>,
}

impl ExecutionBackendRoute {
  /// Binds validated inventory evidence and cache identity to one backend.
  pub fn new(
    capability: ExecutionCapabilityV2,
    environment: ExecutionEnvironmentId,
    backend: Arc<dyn ExecutionBackend>,
  ) -> Result<Self, JobError> {
    capability.validate().map_err(JobError::Invalid)?;
    Ok(Self {
      capability,
      environment,
      backend,
      factory_enforcement: BTreeSet::new(),
    })
  }

  /// Capability advertised to the scheduler for this route.
  #[must_use]
  pub const fn capability(&self) -> &ExecutionCapabilityV2 {
    &self.capability
  }

  /// Records semantic Factory controls proved by backend conformance.
  ///
  /// Ordinary routes advertise none by default. Backend-specific assembly must
  /// opt in only after its positive and negative enforcement contracts pass.
  pub fn with_factory_enforcement(
    mut self,
    capabilities: impl IntoIterator<Item = FactoryEnforcementCapabilityV3>,
  ) -> Result<Self, JobError> {
    let capabilities = capabilities.into_iter().collect::<Vec<_>>();
    self.factory_enforcement = capabilities.iter().copied().collect();
    if self.factory_enforcement.len() != capabilities.len() {
      return Err(JobError::Invalid(
        "Factory enforcement capabilities must not contain duplicates".to_owned(),
      ));
    }
    Ok(self)
  }
}

pub(super) struct SelectedBackend {
  pub(super) backend: Arc<dyn ExecutionBackend>,
  pub(super) evidence: Option<ExecutionEvidenceV2>,
  pub(super) cache_identity: Option<ExecutionCacheIdentityV2>,
  pub(super) factory_enforcement: BTreeSet<FactoryEnforcementCapabilityV3>,
}

pub(super) struct ExecutableJobSpec {
  pub(super) job_id: String,
  pub(super) attempt: u32,
  pub(super) source: SourceSpec,
  pub(super) octa: OctaSpec,
  pub(super) execution: ExecutableExecution,
  pub(super) runtime: ExecutableRuntime,
  pub(super) cache: Option<CachePolicy>,
  pub(super) outputs: OutputLimits,
}

pub(super) enum ExecutableExecution {
  Ordinary(ExecutionSpec),
  Managed(Box<ManagedFactoryExecution>),
}

impl ExecutableExecution {
  pub(super) const fn managed(&self) -> Option<&ManagedFactoryExecution> {
    match self {
      Self::Ordinary(_) => None,
      Self::Managed(value) => Some(value),
    }
  }

  pub(super) const fn ordinary(&self) -> Option<&ExecutionSpec> {
    match self {
      Self::Ordinary(value) => Some(value),
      Self::Managed(_) => None,
    }
  }
}

impl TryFrom<VerifiedJobSpec> for ExecutableJobSpec {
  type Error = JobError;

  fn try_from(spec: VerifiedJobSpec) -> Result<Self, Self::Error> {
    match spec {
      VerifiedJobSpec::V1(spec) => Ok(Self::from(spec)),
      VerifiedJobSpec::V2(spec) => Ok(Self::from(spec)),
      VerifiedJobSpec::V3(spec) => Ok(Self::from(*spec)),
    }
  }
}

impl From<JobSpecV1> for ExecutableJobSpec {
  fn from(spec: JobSpecV1) -> Self {
    Self {
      job_id: spec.job_id,
      attempt: spec.attempt,
      source: spec.source,
      octa: spec.octa,
      execution: ExecutableExecution::Ordinary(spec.execution),
      runtime: ExecutableRuntime::Legacy(spec.runtime),
      cache: spec.cache,
      outputs: spec.outputs,
    }
  }
}

impl From<JobSpecV2> for ExecutableJobSpec {
  fn from(spec: JobSpecV2) -> Self {
    Self {
      job_id: spec.job_id,
      attempt: spec.attempt,
      source: spec.source,
      octa: spec.octa,
      execution: ExecutableExecution::Ordinary(spec.execution),
      runtime: ExecutableRuntime::Current(spec.runtime),
      cache: spec.cache,
      outputs: spec.outputs,
    }
  }
}

impl From<JobSpecV3> for ExecutableJobSpec {
  fn from(spec: JobSpecV3) -> Self {
    Self {
      job_id: spec.job_id,
      attempt: spec.attempt,
      source: spec.source,
      octa: spec.octa,
      execution: ExecutableExecution::Managed(Box::new(ManagedFactoryExecution {
        execution: spec.execution,
        protected_inputs: spec.protected_inputs,
        permissions: spec.permissions,
        required_enforcement: spec.required_enforcement,
      })),
      runtime: ExecutableRuntime::Current(spec.runtime),
      cache: spec.cache,
      outputs: spec.outputs,
    }
  }
}

pub(super) enum ExecutableRuntime {
  Legacy(RuntimeSpec),
  Current(RuntimeSpecV2),
}

impl ExecutableRuntime {
  pub(super) const fn cpu_millis(&self) -> u32 {
    match self {
      Self::Legacy(runtime) => runtime.cpu_millis,
      Self::Current(runtime) => runtime.cpu_millis,
    }
  }

  pub(super) const fn memory_bytes(&self) -> u64 {
    match self {
      Self::Legacy(runtime) => runtime.memory_bytes,
      Self::Current(runtime) => runtime.memory_bytes,
    }
  }

  pub(super) const fn writable_disk_bytes(&self) -> u64 {
    match self {
      Self::Legacy(runtime) => runtime.writable_disk_bytes,
      Self::Current(runtime) => runtime.writable_disk_bytes,
    }
  }

  pub(super) const fn timeout_seconds(&self) -> u64 {
    match self {
      Self::Legacy(runtime) => runtime.timeout_seconds,
      Self::Current(runtime) => runtime.timeout_seconds,
    }
  }

  pub(super) const fn network(&self) -> &NetworkPolicy {
    match self {
      Self::Legacy(runtime) => &runtime.network,
      Self::Current(runtime) => &runtime.network,
    }
  }

  pub(super) fn workload_identity_profile(&self) -> Option<&String> {
    match self {
      Self::Legacy(runtime) => runtime.workload_identity_profile.as_ref(),
      Self::Current(runtime) => runtime.workload_identity_profile.as_ref(),
    }
  }

  pub(super) fn cache_identity<'a>(
    &'a self,
    current: Option<&'a ExecutionCacheIdentityV2>,
  ) -> Result<CacheExecutionIdentity<'a>, JobError> {
    match (self, current) {
      (Self::Legacy(runtime), None) => Ok(CacheExecutionIdentity::from(&runtime.target)),
      (Self::Current(_), Some(identity)) => Ok(CacheExecutionIdentity::from(identity)),
      _ => Err(JobError::Invalid(
        "selected execution route has an inconsistent cache identity".to_owned(),
      )),
    }
  }

  pub(super) fn label(&self) -> String {
    match self {
      Self::Legacy(runtime) => format!("legacy:{:?}", runtime.mode()),
      Self::Current(runtime) => format!("v2:{:?}", runtime.target.mode),
    }
  }

  pub(super) fn execution_target(&self) -> Result<ExecutionTarget, JobError> {
    match self {
      Self::Legacy(runtime) => Ok(legacy_execution_target(runtime)),
      Self::Current(runtime) => current_execution_target(runtime),
    }
  }
}

fn current_execution_target(runtime: &RuntimeSpecV2) -> Result<ExecutionTarget, JobError> {
  let platform = execution_platform(runtime.target.target_platform);
  match runtime.target.mode {
    ExecutionMode::Host => Ok(ExecutionTarget::Host { platform }),
    ExecutionMode::Isolation | ExecutionMode::Virtualization => {
      let reference = runtime.target.immutable_image.clone().ok_or_else(|| {
        JobError::Invalid(format!(
          "execution mode '{:?}' requires an immutable image for the selected provider",
          runtime.target.mode
        ))
      })?;
      Ok(ExecutionTarget::Oci {
        reference,
        platform,
        isolation: match runtime.target.mode {
          ExecutionMode::Isolation => ExecutionOciIsolation::Process,
          ExecutionMode::Virtualization => ExecutionOciIsolation::Hypervisor,
          ExecutionMode::Host => unreachable!(),
        },
      })
    }
  }
}

pub(super) fn legacy_execution_target(runtime: &RuntimeSpec) -> ExecutionTarget {
  match &runtime.target {
    RuntimeTarget::Native { platform } => ExecutionTarget::Native {
      platform: execution_platform(*platform),
    },
    RuntimeTarget::Oci {
      platform,
      isolation,
      image,
    } => ExecutionTarget::Oci {
      reference: image.clone(),
      platform: execution_platform(*platform),
      isolation: match isolation {
        ProtocolOciIsolation::Process => ExecutionOciIsolation::Process,
        ProtocolOciIsolation::Hypervisor => ExecutionOciIsolation::Hypervisor,
      },
    },
  }
}

fn execution_platform(platform: octacity_protocol::PlatformSpec) -> ExecutionPlatform {
  ExecutionPlatform {
    os: match platform.os {
      PlatformOs::Linux => ExecutionOs::Linux,
      PlatformOs::Windows => ExecutionOs::Windows,
      PlatformOs::Macos => ExecutionOs::Macos,
    },
    architecture: match platform.architecture {
      PlatformArchitecture::Amd64 => ExecutionArchitecture::Amd64,
      PlatformArchitecture::Arm64 => ExecutionArchitecture::Arm64,
    },
  }
}
