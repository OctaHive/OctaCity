use std::collections::BTreeSet;

use octacity_protocol::{
  AgentInventory, BackendHealthStatus, EXECUTION_CONTRACT_V2, ExecutionCapabilityV2, ExecutionMode,
  FactoryEnforcementCapabilityV3, HostSnapshot, OciIsolation, PlatformSpec, RuntimeMode, RuntimeTarget,
  SUPPORTED_EXECUTION_CONTRACTS, TaskPluginInventory,
};
use octacity_server_domain::RuntimeClass;
use octacity_server_job::{JobRequirements, JobRuntimePolicy, JobSpecTemplate};

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
  let Some(execution_contract_version) = SUPPORTED_EXECUTION_CONTRACTS.negotiate(inventory.execution_contract) else {
    return false;
  };
  is_compatible_with_contract(inventory, snapshot, requirements, template, execution_contract_version)
}

/// Reports compatibility using the execution-contract revision selected for
/// the Agent's current registration.
#[must_use]
pub fn is_compatible_with_contract(
  inventory: &AgentInventory,
  snapshot: &HostSnapshot,
  requirements: &JobRequirements,
  template: &JobSpecTemplate,
  execution_contract_version: u16,
) -> bool {
  if !inventory.execution_contract.contains(execution_contract_version)
    || !SUPPORTED_EXECUTION_CONTRACTS.contains(execution_contract_version)
  {
    return false;
  }
  let policy = template.placement_policy();
  let platform = PlatformSpec {
    os: requirements.operating_system,
    architecture: requirements.architecture,
  };
  let source_plugin_platform = plugin_platform(inventory.host_platform);
  let task_plugin_platform = plugin_platform(platform);
  execution_contract_version >= policy.minimum_execution_contract
    && labels_match(inventory, requirements)
    && resources_match(snapshot, requirements)
    && backend_is_available(
      inventory,
      snapshot,
      requirements,
      policy.runtime,
      template.required_factory_enforcement(),
    )
    && capabilities_match(inventory, requirements, requirements.runtime_class.is_legacy())
    && inventory.octa.version == policy.octa.version
    && inventory.octa.runner_sha256 == policy.octa.runner_sha256
    && inventory.octa.runner_protocols.contains(&policy.octa.runner_protocol)
    && inventory.octa.event_schemas.contains(&policy.octa.event_schema)
    && inventory.octa.plugin_protocols.contains(&policy.octa.plugin_protocol)
    && plugins_match(
      &inventory.octa.plugins,
      policy.octa,
      &task_plugin_platform,
      template.required_tool_control(),
    )
    && inventory.source_plugins.iter().any(|source| {
      source.name == policy.source_provider
        && source.version == policy.source_plugin_version
        && source.sha256 == policy.source_plugin_sha256
        && source
          .platforms
          .iter()
          .any(|candidate| candidate == &source_plugin_platform)
    })
    && (!policy.requires_cache
      || inventory.cache.as_ref().is_some_and(|cache| {
        cache.runner_protocol == policy.octa.runner_protocol
          && cache.action_key_format == octacity_protocol::CACHE_ACTION_KEY_FORMAT_V1
          && cache.remote_http
      }))
    && runtime_policy_matches(
      policy.runtime,
      requirements.runtime_class,
      platform,
      execution_contract_version,
    )
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
  requirements: &JobRequirements,
  runtime_policy: &JobRuntimePolicy,
  required_factory_enforcement: Option<&[FactoryEnforcementCapabilityV3]>,
) -> bool {
  let platform = PlatformSpec {
    os: requirements.operating_system,
    architecture: requirements.architecture,
  };
  if let JobRuntimePolicy::Current(runtime) = runtime_policy {
    let Some(host_platform) = requirements.host_platform else {
      return false;
    };
    if runtime.target.host_platform != host_platform
      || runtime.target.target_platform != platform
      || runtime.target.required_guarantees != requirements.required_guarantees
    {
      return false;
    }
    return inventory.executions.iter().any(|execution| {
      execution.satisfies(&runtime.target)
        && factory_enforcement_matches(inventory, execution, required_factory_enforcement)
        && snapshot.backends.iter().any(|health| {
          health.backend == execution.provider.as_str()
            && health.execution.as_ref() == Some(execution)
            && !matches!(health.status, BackendHealthStatus::Unavailable)
        })
    });
  }
  inventory.runtimes.iter().any(|runtime| {
    runtime.platform == platform
      && match requirements.runtime_class {
        RuntimeClass::Native => runtime.mode == RuntimeMode::Native && runtime.isolation.is_none(),
        RuntimeClass::OciProcess => {
          runtime.mode == RuntimeMode::Oci && runtime.isolation == Some(OciIsolation::Process)
        }
        RuntimeClass::OciHypervisor => {
          runtime.mode == RuntimeMode::Oci && runtime.isolation == Some(OciIsolation::Hypervisor)
        }
        RuntimeClass::Host | RuntimeClass::Isolation | RuntimeClass::Virtualization => false,
      }
      && snapshot
        .backends
        .iter()
        .any(|health| health.backend == runtime.backend && !matches!(health.status, BackendHealthStatus::Unavailable))
  })
}

fn factory_enforcement_matches(
  inventory: &AgentInventory,
  execution: &ExecutionCapabilityV2,
  required: Option<&[FactoryEnforcementCapabilityV3]>,
) -> bool {
  required.is_none_or(|required| {
    inventory.factory_executions.iter().any(|candidate| {
      candidate.execution == *execution
        && required
          .iter()
          .all(|capability| candidate.enforcement.binary_search(capability).is_ok())
    })
  })
}

fn runtime_policy_matches(
  runtime: &JobRuntimePolicy,
  class: RuntimeClass,
  platform: PlatformSpec,
  execution_contract_version: u16,
) -> bool {
  match (runtime, class) {
    (JobRuntimePolicy::Legacy(runtime), class) => legacy_runtime_policy_matches(runtime, class, platform),
    (JobRuntimePolicy::Current(runtime), class) => {
      execution_contract_version >= EXECUTION_CONTRACT_V2
        && current_mode(class).is_some_and(|mode| {
          runtime.target.mode == mode && runtime.target.target_platform == platform && runtime.target.validate().is_ok()
        })
    }
  }
}

fn legacy_runtime_policy_matches(
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

fn current_mode(class: RuntimeClass) -> Option<ExecutionMode> {
  match class {
    RuntimeClass::Host => Some(ExecutionMode::Host),
    RuntimeClass::Isolation => Some(ExecutionMode::Isolation),
    RuntimeClass::Virtualization => Some(ExecutionMode::Virtualization),
    RuntimeClass::Native | RuntimeClass::OciProcess | RuntimeClass::OciHypervisor => None,
  }
}

fn capabilities_match(
  inventory: &AgentInventory,
  requirements: &JobRequirements,
  include_legacy_runtime: bool,
) -> bool {
  let mut advertised = BTreeSet::<&str>::new();
  advertised.extend(inventory.octa.features.iter().map(String::as_str));
  if include_legacy_runtime {
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
  }
  for plugin in &inventory.octa.plugins {
    advertised.extend(plugin.capabilities.iter().map(String::as_str));
  }
  requirements
    .capabilities
    .iter()
    .all(|capability| advertised.contains(capability.as_str()))
}

fn plugins_match(
  installed: &[TaskPluginInventory],
  required: &octacity_protocol::OctaSpec,
  platform: &str,
  tool_control: Option<&octacity_protocol::FactoryToolControlV3>,
) -> bool {
  required.plugin_digests.iter().all(|(name, digest)| {
    installed.iter().any(|plugin| {
      plugin.name == *name
        && plugin.sha256 == *digest
        && plugin.protocol == required.plugin_protocol
        && plugin.platforms.iter().any(|candidate| candidate == platform)
        && tool_control
          .is_none_or(|control| control.plugin != *name || plugin.capabilities.contains(&control.capability))
    })
  }) && tool_control.is_none_or(|control| required.plugin_digests.contains_key(&control.plugin))
}

#[cfg(test)]
mod tests {
  use std::collections::{BTreeMap, BTreeSet};

  use octacity_protocol::{
    BLOCKING_TOOL_AUTHORIZATION_CAPABILITY, BackendHealth, BackendHealthStatus, ExecutionCapabilityV2,
    ExecutionContractRange, ExecutionMode, ExecutionTargetV2, FactoryEnforcementCapabilityV3,
    FactoryExecutionCapabilityV3, FactoryToolControlModeV3, FactoryToolControlV3, HostCapacity, HostSnapshot,
    JobSpecV3, NetworkPolicy, OctaInventory, OctaSpec, OutputLimits, PlatformArchitecture, PlatformOs,
    RuntimeCapability, RuntimeSpec, RuntimeSpecV2, SourcePluginInventory, guarantees_for,
  };
  use octacity_server_domain::{BuildId, ImmutableRevision, PipelineNodeId, RepositoryLocator};
  use octacity_server_job::{
    JobRuntimePolicy, JobSpecBuildSnapshot, JobSpecPolicySnapshot, JobSpecValidity, ManagedJobSpecIntent,
    SourcePluginPolicy, derive_job_spec_template, derive_managed_job_spec_template,
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

  #[test]
  fn tool_risk_control_requires_the_exact_blocking_plugin_capability() {
    let required = OctaSpec {
      version: "0.5.1".to_owned(),
      runner_sha256: DIGEST.to_owned(),
      runner_protocol: 3,
      event_schema: 4,
      plugin_protocol: 2,
      plugin_digests: BTreeMap::from([("codex".to_owned(), DIGEST.to_owned())]),
    };
    let control = FactoryToolControlV3 {
      plugin: "codex".to_owned(),
      capability: BLOCKING_TOOL_AUTHORIZATION_CAPABILITY.to_owned(),
      mode: FactoryToolControlModeV3::ToolRisk,
    };
    let mut installed = vec![TaskPluginInventory {
      name: "codex".to_owned(),
      version: "0.5.1".to_owned(),
      protocol: 2,
      platforms: vec!["linux-amd64".to_owned()],
      sha256: DIGEST.to_owned(),
      capabilities: vec![],
    }];

    assert!(!plugins_match(&installed, &required, "linux-amd64", Some(&control)));
    assert!(plugins_match(&installed, &required, "linux-amd64", None));
    installed[0]
      .capabilities
      .push(BLOCKING_TOOL_AUTHORIZATION_CAPABILITY.to_owned());
    assert!(plugins_match(&installed, &required, "linux-amd64", Some(&control)));
  }

  #[test]
  fn host_plugins_and_guest_task_plugins_match_their_own_platforms() {
    let mut requirements = requirements();
    requirements.capabilities = BTreeSet::from([
      ExecutionCapability::new("oci.hypervisor").unwrap(),
      ExecutionCapability::new("shell").unwrap(),
    ]);
    requirements.runtime_class = RuntimeClass::OciHypervisor;

    let mut inventory = inventory();
    inventory.host_platform = PlatformSpec {
      os: PlatformOs::Macos,
      architecture: PlatformArchitecture::Arm64,
    };
    inventory.source_plugins[0].platforms = vec![plugin_platform(inventory.host_platform)];
    inventory.runtimes[0] = RuntimeCapability {
      backend: "microsandbox".to_owned(),
      mode: RuntimeMode::Oci,
      platform: platform(),
      isolation: Some(OciIsolation::Hypervisor),
    };

    let mut snapshot = snapshot();
    snapshot.backends[0].backend = "microsandbox".to_owned();
    let template = template_for(RuntimeTarget::Oci {
      image: format!("example.invalid/octa@sha256:{DIGEST}"),
      platform: platform(),
      isolation: OciIsolation::Hypervisor,
    });

    assert!(is_compatible(&inventory, &snapshot, &requirements, &template));
  }

  #[test]
  fn provider_neutral_placement_accepts_equivalent_providers_without_leaking_their_names() {
    let target = ExecutionTargetV2 {
      mode: ExecutionMode::Isolation,
      host_platform: platform(),
      target_platform: platform(),
      required_guarantees: guarantees_for(ExecutionMode::Isolation),
      immutable_image: None,
    };
    let mut requirements = requirements();
    requirements.capabilities = BTreeSet::from([ExecutionCapability::new("shell").unwrap()]);
    requirements.runtime_class = RuntimeClass::Isolation;
    requirements.host_platform = Some(platform());
    requirements.required_guarantees = target.required_guarantees.clone();
    let mut inventory = inventory();
    inventory.execution_contract = ExecutionContractRange { min: 1, max: 2 };
    inventory.runtimes.clear();
    let capability = ExecutionCapabilityV2 {
      provider: octacity_protocol::ExecutionProviderId::new("containerd").unwrap(),
      mode: ExecutionMode::Isolation,
      host_platform: platform(),
      target_platform: platform(),
      guarantees: target.required_guarantees.clone(),
      immutable_images: true,
    };
    inventory.executions = vec![capability.clone()];
    let mut snapshot = snapshot();
    snapshot.backends = vec![BackendHealth {
      backend: capability.provider.to_string(),
      execution: Some(capability.clone()),
      status: BackendHealthStatus::Ready,
      message: None,
    }];
    let template = template_for_policy(JobRuntimePolicy::Current(RuntimeSpecV2 {
      target,
      cpu_millis: 1_500,
      memory_bytes: 2_048,
      writable_disk_bytes: 4_096,
      timeout_seconds: 60,
      network: NetworkPolicy::Disabled,
      workload_identity_profile: None,
    }));

    assert!(is_compatible(&inventory, &snapshot, &requirements, &template));
    assert!(!is_compatible_with_contract(
      &inventory,
      &snapshot,
      &requirements,
      &template,
      octacity_protocol::EXECUTION_CONTRACT_V1,
    ));
    assert!(is_compatible_with_contract(
      &inventory,
      &snapshot,
      &requirements,
      &template,
      octacity_protocol::EXECUTION_CONTRACT_V2,
    ));
    inventory.executions[0].provider = octacity_protocol::ExecutionProviderId::new("apple-vf-isolation").unwrap();
    snapshot.backends[0].backend = "apple-vf-isolation".to_owned();
    snapshot.backends[0].execution = Some(inventory.executions[0].clone());
    assert!(is_compatible(&inventory, &snapshot, &requirements, &template));

    requirements
      .capabilities
      .insert(ExecutionCapability::new("containerd").unwrap());
    assert!(!is_compatible(&inventory, &snapshot, &requirements, &template));
  }

  #[test]
  fn factory_enforcement_matching_is_provider_neutral_and_complete() {
    let mut inventory = inventory();
    let required = FactoryEnforcementCapabilityV3::ALL;
    for provider in ["containerd", "microsandbox"] {
      let execution = ExecutionCapabilityV2 {
        provider: octacity_protocol::ExecutionProviderId::new(provider).unwrap(),
        mode: ExecutionMode::Isolation,
        host_platform: platform(),
        target_platform: platform(),
        guarantees: guarantees_for(ExecutionMode::Isolation),
        immutable_images: true,
      };
      inventory.factory_executions = vec![FactoryExecutionCapabilityV3 {
        execution: execution.clone(),
        enforcement: required.to_vec(),
      }];
      assert!(factory_enforcement_matches(&inventory, &execution, Some(&required)));
    }

    inventory.factory_executions[0].enforcement.pop();
    let execution = inventory.factory_executions[0].execution.clone();
    assert!(!factory_enforcement_matches(&inventory, &execution, Some(&required),));
  }

  #[test]
  fn managed_v3_placement_requires_every_advertised_semantic_control() {
    let canonical: JobSpecV3 = serde_json::from_str(include_str!(
      "../../../../shared/protocol-fixtures/job-spec/job-spec-v3.json"
    ))
    .unwrap();
    let template = managed_template(&canonical);
    let target = &canonical.runtime.target;
    let mut requirements = requirements();
    requirements.capabilities.clear();
    requirements.runtime_class = RuntimeClass::Virtualization;
    requirements.operating_system = target.target_platform.os;
    requirements.architecture = target.target_platform.architecture;
    requirements.host_platform = Some(target.host_platform);
    requirements.required_guarantees = target.required_guarantees.clone();
    requirements.minimum_cpu_millis = canonical.runtime.cpu_millis;
    requirements.minimum_memory_bytes = canonical.runtime.memory_bytes;
    requirements.minimum_disk_bytes = canonical.runtime.writable_disk_bytes;

    let execution = ExecutionCapabilityV2 {
      provider: octacity_protocol::ExecutionProviderId::new("qualified-vm").unwrap(),
      mode: target.mode,
      host_platform: target.host_platform,
      target_platform: target.target_platform,
      guarantees: target.required_guarantees.clone(),
      immutable_images: true,
    };
    let mut inventory = inventory();
    inventory.execution_contract = ExecutionContractRange { min: 1, max: 3 };
    inventory.host_platform = target.host_platform;
    inventory.runtimes.clear();
    inventory.executions = vec![execution.clone()];
    inventory.factory_executions = vec![FactoryExecutionCapabilityV3 {
      execution: execution.clone(),
      enforcement: canonical.required_enforcement.clone(),
    }];
    inventory.octa = OctaInventory {
      version: canonical.octa.version.clone(),
      runner_sha256: canonical.octa.runner_sha256.clone(),
      build_commit: None,
      runner_protocols: vec![canonical.octa.runner_protocol],
      event_schemas: vec![canonical.octa.event_schema],
      plugin_protocols: vec![canonical.octa.plugin_protocol],
      octafile_versions: vec![1],
      features: Vec::new(),
      plugins: canonical
        .octa
        .plugin_digests
        .iter()
        .map(|(name, digest)| TaskPluginInventory {
          name: name.clone(),
          version: "0.5.0".to_owned(),
          protocol: canonical.octa.plugin_protocol,
          platforms: vec![plugin_platform(target.target_platform)],
          sha256: digest.clone(),
          capabilities: Vec::new(),
        })
        .collect(),
    };
    inventory.source_plugins = vec![SourcePluginInventory {
      name: canonical.source.provider.clone(),
      version: canonical.source.plugin_version.clone(),
      protocol_min: 1,
      protocol_max: 1,
      platforms: vec![plugin_platform(target.host_platform)],
      sha256: canonical.source.plugin_sha256.clone(),
    }];
    let snapshot = HostSnapshot {
      available_cpu_millis: 2_000,
      available_memory_bytes: 1_073_741_824,
      work_disk_free_bytes: 2_147_483_648,
      state_disk_free_bytes: 2_147_483_648,
      backends: vec![BackendHealth {
        backend: execution.provider.to_string(),
        execution: Some(execution),
        status: BackendHealthStatus::Ready,
        message: None,
      }],
      ..snapshot()
    };

    assert!(is_compatible(&inventory, &snapshot, &requirements, &template));
    inventory.factory_executions[0].enforcement.pop();
    assert!(!is_compatible(&inventory, &snapshot, &requirements, &template));
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
      host_platform: None,
      required_guarantees: BTreeSet::new(),
    }
  }

  fn inventory() -> AgentInventory {
    AgentInventory {
      agent_id: "agent".to_owned(),
      agent_version: "1.0.0".to_owned(),
      coordinator_protocols: vec![1],
      execution_contract: octacity_protocol::ExecutionContractRange { min: 1, max: 1 },
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
      executions: Vec::new(),
      factory_executions: Vec::new(),
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
        execution: None,
        status: BackendHealthStatus::Ready,
        message: None,
      }],
    }
  }

  fn template() -> JobSpecTemplate {
    template_for(RuntimeTarget::Native { platform: platform() })
  }

  fn template_for(runtime_target: RuntimeTarget) -> JobSpecTemplate {
    template_for_policy(JobRuntimePolicy::Legacy(RuntimeSpec {
      target: runtime_target,
      cpu_millis: 1_500,
      memory_bytes: 2_048,
      writable_disk_bytes: 4_096,
      timeout_seconds: 60,
      network: NetworkPolicy::Disabled,
      workload_identity_profile: None,
    }))
  }

  fn template_for_policy(runtime: JobRuntimePolicy) -> JobSpecTemplate {
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
      runtime,
      None,
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

  fn managed_template(canonical: &JobSpecV3) -> JobSpecTemplate {
    let build = JobSpecBuildSnapshot::new(
      BuildId::from_uuid(Uuid::from_u128(1)).unwrap(),
      ImmutableRevision::new(canonical.source.revision.clone()).unwrap(),
      None,
      RepositoryLocator::new("https://example.test/repository.git").unwrap(),
      BTreeMap::new(),
    )
    .unwrap();
    let policy = JobSpecPolicySnapshot::new(
      SourcePluginPolicy::new(
        canonical.source.provider.clone(),
        canonical.source.plugin_version.clone(),
        canonical.source.plugin_sha256.clone(),
        "url",
      )
      .unwrap(),
      canonical.octa.clone(),
      JobRuntimePolicy::Current(canonical.runtime.clone()),
      None,
      canonical.cache.clone(),
      canonical.outputs.clone(),
      JobSpecValidity::new(canonical.expires_at - canonical.issued_at).unwrap(),
    )
    .unwrap();
    derive_managed_job_spec_template(
      &build,
      PipelineNodeId::new("factory").unwrap(),
      ManagedJobSpecIntent::new(
        canonical.execution.clone(),
        canonical.factory.clone().unwrap(),
        canonical.protected_inputs.clone(),
        canonical.permissions.clone(),
        canonical.required_enforcement.clone(),
      ),
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
