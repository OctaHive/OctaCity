use std::collections::BTreeSet;

use octacity_protocol::{
  AgentInventory, BackendHealthStatus, HostSnapshot, OciIsolation, PlatformSpec, RuntimeMode, RuntimeTarget,
  TaskPluginInventory,
};
use octacity_server_domain::RuntimeClass;
use octacity_server_job::{JobRequirements, JobSpecTemplate};

/// Reports whether one accepting idle Agent exactly satisfies a ready Job.
///
/// This is a pure domain decision shared by every authoritative-store
/// adapter. The Agent still validates the signed JobSpec before execution.
#[must_use]
pub fn is_compatible(
  inventory: &AgentInventory,
  snapshot: &HostSnapshot,
  requirements: &JobRequirements,
  template: &JobSpecTemplate,
) -> bool {
  let policy = template.placement_policy();
  let platform = PlatformSpec {
    os: requirements.operating_system,
    architecture: requirements.architecture,
  };
  let plugin_platform = plugin_platform(inventory.host_platform);
  labels_match(inventory, requirements)
    && resources_match(snapshot, requirements)
    && runtime_matches(inventory, requirements.runtime_class, platform)
    && backend_is_available(inventory, snapshot, requirements.runtime_class, platform)
    && capabilities_match(inventory, requirements)
    && inventory.octa.version == policy.octa.version
    && inventory.octa.runner_sha256 == policy.octa.runner_sha256
    && inventory.octa.runner_protocols.contains(&policy.octa.runner_protocol)
    && inventory.octa.event_schemas.contains(&policy.octa.event_schema)
    && inventory.octa.plugin_protocols.contains(&policy.octa.plugin_protocol)
    && plugins_match(&inventory.octa.plugins, policy.octa, &plugin_platform)
    && inventory.source_plugins.iter().any(|source| {
      source.name == policy.source_provider
        && source.version == policy.source_plugin_version
        && source.sha256 == policy.source_plugin_sha256
        && source.platforms.iter().any(|candidate| candidate == &plugin_platform)
    })
    && (!policy.requires_cache
      || inventory.cache.as_ref().is_some_and(|cache| {
        cache.runner_protocol == policy.octa.runner_protocol
          && cache.action_key_format == octacity_protocol::CACHE_ACTION_KEY_FORMAT_V1
          && cache.remote_http
      }))
    && runtime_policy_matches(policy.runtime, requirements.runtime_class, platform)
}

fn plugin_platform(platform: PlatformSpec) -> String {
  let operating_system = match platform.os {
    octacity_protocol::PlatformOs::Linux => "linux",
    octacity_protocol::PlatformOs::Windows => "windows",
    octacity_protocol::PlatformOs::Macos => "macos",
  };
  let architecture = match platform.architecture {
    octacity_protocol::PlatformArchitecture::Amd64 => "x86_64",
    octacity_protocol::PlatformArchitecture::Arm64 => "aarch64",
  };
  format!("{operating_system}-{architecture}")
}

fn labels_match(inventory: &AgentInventory, requirements: &JobRequirements) -> bool {
  requirements
    .labels
    .iter()
    .all(|(key, value)| inventory.labels.get(key) == Some(value))
}

fn resources_match(snapshot: &HostSnapshot, requirements: &JobRequirements) -> bool {
  snapshot.available_cpu_millis >= u64::from(requirements.minimum_cpu_millis)
    && snapshot.available_memory_bytes >= requirements.minimum_memory_bytes
    && snapshot.work_disk_free_bytes >= requirements.minimum_disk_bytes
}

fn backend_is_available(
  inventory: &AgentInventory,
  snapshot: &HostSnapshot,
  class: RuntimeClass,
  platform: PlatformSpec,
) -> bool {
  inventory.runtimes.iter().any(|runtime| {
    runtime.platform == platform
      && match class {
        RuntimeClass::Native => runtime.mode == RuntimeMode::Native && runtime.isolation.is_none(),
        RuntimeClass::OciProcess => {
          runtime.mode == RuntimeMode::Oci && runtime.isolation == Some(OciIsolation::Process)
        }
        RuntimeClass::OciHypervisor => {
          runtime.mode == RuntimeMode::Oci && runtime.isolation == Some(OciIsolation::Hypervisor)
        }
      }
      && snapshot
        .backends
        .iter()
        .any(|health| health.backend == runtime.backend && !matches!(health.status, BackendHealthStatus::Unavailable))
  })
}

fn runtime_matches(inventory: &AgentInventory, class: RuntimeClass, platform: PlatformSpec) -> bool {
  inventory.runtimes.iter().any(|runtime| {
    runtime.platform == platform
      && match class {
        RuntimeClass::Native => runtime.mode == RuntimeMode::Native && runtime.isolation.is_none(),
        RuntimeClass::OciProcess => {
          runtime.mode == RuntimeMode::Oci && runtime.isolation == Some(OciIsolation::Process)
        }
        RuntimeClass::OciHypervisor => {
          runtime.mode == RuntimeMode::Oci && runtime.isolation == Some(OciIsolation::Hypervisor)
        }
      }
  })
}

fn runtime_policy_matches(
  runtime: &octacity_protocol::RuntimeSpec,
  class: RuntimeClass,
  platform: PlatformSpec,
) -> bool {
  match (&runtime.target, class) {
    (RuntimeTarget::Native { platform: required }, RuntimeClass::Native) => *required == platform,
    (
      RuntimeTarget::Oci {
        platform: required,
        isolation,
        ..
      },
      RuntimeClass::OciProcess,
    ) => *required == platform && *isolation == OciIsolation::Process,
    (
      RuntimeTarget::Oci {
        platform: required,
        isolation,
        ..
      },
      RuntimeClass::OciHypervisor,
    ) => *required == platform && *isolation == OciIsolation::Hypervisor,
    _ => false,
  }
}

fn capabilities_match(inventory: &AgentInventory, requirements: &JobRequirements) -> bool {
  let mut advertised = BTreeSet::<&str>::new();
  advertised.extend(inventory.octa.features.iter().map(String::as_str));
  for runtime in &inventory.runtimes {
    advertised.insert(runtime.backend.as_str());
    advertised.insert(match runtime.mode {
      RuntimeMode::Native => "native",
      RuntimeMode::Oci => "oci",
    });
    if let Some(isolation) = runtime.isolation {
      advertised.insert(match isolation {
        OciIsolation::Process => "oci.process",
        OciIsolation::Hypervisor => "oci.hypervisor",
      });
    }
  }
  for plugin in &inventory.octa.plugins {
    advertised.extend(plugin.capabilities.iter().map(String::as_str));
  }
  requirements
    .capabilities
    .iter()
    .all(|capability| advertised.contains(capability.as_str()))
}

fn plugins_match(installed: &[TaskPluginInventory], required: &octacity_protocol::OctaSpec, platform: &str) -> bool {
  required.plugin_digests.iter().all(|(name, digest)| {
    installed.iter().any(|plugin| {
      plugin.name == *name
        && plugin.sha256 == *digest
        && plugin.protocol == required.plugin_protocol
        && plugin.platforms.iter().any(|candidate| candidate == platform)
    })
  })
}

#[cfg(test)]
mod tests {
  use std::collections::{BTreeMap, BTreeSet};

  use octacity_protocol::{
    BackendHealth, BackendHealthStatus, HostCapacity, HostSnapshot, NetworkPolicy, OctaInventory, OctaSpec,
    OutputLimits, PlatformArchitecture, PlatformOs, RuntimeCapability, RuntimeSpec, SourcePluginInventory,
  };
  use octacity_server_domain::{BuildId, ImmutableRevision, PipelineNodeId, RepositoryLocator};
  use octacity_server_job::{
    JobSpecBuildSnapshot, JobSpecPolicySnapshot, JobSpecValidity, SourcePluginPolicy, derive_job_spec_template,
  };
  use octacity_server_pipeline::ExecutionCapability;
  use serde_json::json;
  use uuid::Uuid;

  use super::*;

  const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

  #[test]
  fn exact_inventory_is_compatible_and_every_scheduling_dimension_is_enforced() {
    let requirements = requirements();
    let template = template();
    let inventory = inventory();
    let snapshot = snapshot();
    assert!(is_compatible(&inventory, &snapshot, &requirements, &template));

    let mut wrong_label = inventory.clone();
    wrong_label.labels.insert("region".to_owned(), "other".to_owned());
    assert!(!is_compatible(&wrong_label, &snapshot, &requirements, &template));

    let mut insufficient_memory = snapshot.clone();
    insufficient_memory.available_memory_bytes = 1;
    assert!(!is_compatible(
      &inventory,
      &insufficient_memory,
      &requirements,
      &template
    ));

    let mut wrong_runtime = inventory.clone();
    wrong_runtime.runtimes[0].mode = RuntimeMode::Oci;
    wrong_runtime.runtimes[0].isolation = Some(OciIsolation::Process);
    assert!(!is_compatible(&wrong_runtime, &snapshot, &requirements, &template));

    let mut wrong_runner = inventory.clone();
    wrong_runner.octa.runner_protocols = vec![2];
    assert!(!is_compatible(&wrong_runner, &snapshot, &requirements, &template));

    let mut wrong_task_plugin = inventory.clone();
    wrong_task_plugin.octa.plugins[0].sha256 = "f".repeat(64);
    assert!(!is_compatible(&wrong_task_plugin, &snapshot, &requirements, &template));

    let mut wrong_source = inventory.clone();
    wrong_source.source_plugins[0].version = "2.0.0".to_owned();
    assert!(!is_compatible(&wrong_source, &snapshot, &requirements, &template));

    let mut unavailable_backend = snapshot;
    unavailable_backend.backends[0].status = BackendHealthStatus::Unavailable;
    assert!(!is_compatible(
      &inventory,
      &unavailable_backend,
      &requirements,
      &template
    ));
  }

  fn requirements() -> JobRequirements {
    JobRequirements {
      capabilities: BTreeSet::from([
        ExecutionCapability::new("native").unwrap(),
        ExecutionCapability::new("shell").unwrap(),
      ]),
      labels: BTreeMap::from([("region".to_owned(), "test".to_owned())]),
      minimum_cpu_millis: 1_500,
      minimum_memory_bytes: 2_048,
      minimum_disk_bytes: 4_096,
      runtime_class: RuntimeClass::Native,
      operating_system: PlatformOs::Linux,
      architecture: PlatformArchitecture::Amd64,
    }
  }

  fn inventory() -> AgentInventory {
    AgentInventory {
      agent_id: "agent".to_owned(),
      agent_version: "1.0.0".to_owned(),
      coordinator_protocols: vec![1],
      labels: BTreeMap::from([("region".to_owned(), "test".to_owned())]),
      host_platform: platform(),
      host_capacity: HostCapacity {
        logical_cpu_count: 2,
        total_memory_bytes: 4_096,
        work_disk_total_bytes: 8_192,
        state_disk_total_bytes: 8_192,
        virtualization_available: false,
      },
      runtimes: vec![RuntimeCapability {
        backend: "native".to_owned(),
        mode: RuntimeMode::Native,
        platform: platform(),
        isolation: None,
      }],
      octa: OctaInventory {
        version: "1.0.0".to_owned(),
        runner_sha256: DIGEST.to_owned(),
        build_commit: None,
        runner_protocols: vec![3],
        event_schemas: vec![4],
        plugin_protocols: vec![5],
        octafile_versions: vec![1],
        features: Vec::new(),
        plugins: vec![TaskPluginInventory {
          name: "shell".to_owned(),
          version: "1.0.0".to_owned(),
          protocol: 5,
          platforms: vec![plugin_platform(platform())],
          sha256: DIGEST.to_owned(),
          capabilities: vec!["shell".to_owned()],
        }],
      },
      source_plugins: vec![SourcePluginInventory {
        name: "git".to_owned(),
        version: "1.0.0".to_owned(),
        protocol_min: 1,
        protocol_max: 1,
        platforms: vec![plugin_platform(platform())],
        sha256: DIGEST.to_owned(),
      }],
      cache: None,
    }
  }

  fn snapshot() -> HostSnapshot {
    HostSnapshot {
      available_cpu_millis: 2_000,
      available_memory_bytes: 4_096,
      work_disk_free_bytes: 8_192,
      state_disk_free_bytes: 8_192,
      active_job: None,
      backends: vec![BackendHealth {
        backend: "native".to_owned(),
        status: BackendHealthStatus::Ready,
        message: None,
      }],
    }
  }

  fn template() -> JobSpecTemplate {
    let build = JobSpecBuildSnapshot::new(
      BuildId::from_uuid(Uuid::from_u128(1)).unwrap(),
      ImmutableRevision::new("revision").unwrap(),
      None,
      RepositoryLocator::new("https://example.test/repository.git").unwrap(),
      BTreeMap::new(),
    )
    .unwrap();
    let policy = JobSpecPolicySnapshot::new(
      SourcePluginPolicy::new("git", "1.0.0", DIGEST, "url").unwrap(),
      OctaSpec {
        version: "1.0.0".to_owned(),
        runner_sha256: DIGEST.to_owned(),
        runner_protocol: 3,
        event_schema: 4,
        plugin_protocol: 5,
        plugin_digests: BTreeMap::from([("shell".to_owned(), DIGEST.to_owned())]),
      },
      RuntimeSpec {
        target: RuntimeTarget::Native { platform: platform() },
        cpu_millis: 1_500,
        memory_bytes: 2_048,
        writable_disk_bytes: 4_096,
        timeout_seconds: 60,
        network: NetworkPolicy::Disabled,
        workload_identity_profile: None,
      },
      None,
      OutputLimits {
        artifact_count: 0,
        artifact_bytes: 0,
        report_count: 0,
        report_bytes: 0,
        single_output_bytes: 0,
      },
      JobSpecValidity::new(600).unwrap(),
    )
    .unwrap();
    derive_job_spec_template(
      &build,
      PipelineNodeId::new("build").unwrap(),
      json!({"commands": ["build"]}),
      &policy,
    )
    .unwrap()
  }

  const fn platform() -> PlatformSpec {
    PlatformSpec {
      os: PlatformOs::Linux,
      architecture: PlatformArchitecture::Amd64,
    }
  }
}
