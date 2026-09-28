use super::*;

#[derive(Clone)]
pub(super) enum ReleaseBackend {
  Native {
    cgroup_root: PathBuf,
    work_root: PathBuf,
    cache_root: PathBuf,
    bubblewrap: PathBuf,
    path: String,
    environment_identity: String,
    workspace_bytes: u64,
  },
  Microsandbox {
    work_root: PathBuf,
    state_root: PathBuf,
    executable: PathBuf,
    libkrunfw: PathBuf,
    environment_identity: String,
    image: String,
    workspace_bytes: u64,
  },
  Containerd {
    work_root: PathBuf,
    cache_root: PathBuf,
    state_root: PathBuf,
    endpoint: PathBuf,
    namespace: String,
    snapshotter: String,
    runtime: String,
    registry_config_dir: Option<PathBuf>,
    environment_identity: String,
    image: String,
    workspace_bytes: u64,
  },
  AppleVf {
    work_root: PathBuf,
    cache_root: PathBuf,
    state_root: PathBuf,
    executable: PathBuf,
    environment_identity: String,
    image: String,
    workspace_bytes: u64,
  },
}

impl ReleaseBackend {
  pub(super) fn from_environment() -> Self {
    match required_string("OCTACITY_RELEASE_BACKEND").as_str() {
      "native" => {
        assert_eq!(env::consts::OS, "linux", "Native release gate requires Linux");
        Self::Native {
          cgroup_root: required_path("OCTACITY_CONTRACT_NATIVE_CGROUP_ROOT", true),
          work_root: required_path("OCTACITY_CONTRACT_NATIVE_WORK_ROOT", true),
          cache_root: required_path("OCTACITY_RELEASE_NATIVE_CACHE_ROOT", true),
          bubblewrap: required_path("OCTACITY_CONTRACT_NATIVE_BWRAP", true),
          path: required_string("OCTACITY_CONTRACT_NATIVE_PATH"),
          environment_identity: required_string("OCTACITY_CONTRACT_NATIVE_ENVIRONMENT_IDENTITY"),
          workspace_bytes: required_u64("OCTACITY_CONTRACT_WORKSPACE_BYTES"),
        }
      }
      "microsandbox" | "linux-microsandbox" => {
        if env::consts::OS == "macos" {
          assert_eq!(
            env::consts::ARCH,
            "aarch64",
            "Microsandbox macOS gate requires Apple Silicon"
          );
        } else {
          assert_eq!(
            env::consts::OS,
            "linux",
            "Microsandbox release gate requires Linux or macOS"
          );
        }
        Self::Microsandbox {
          work_root: required_path("OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT", true),
          state_root: required_path("OCTACITY_CONTRACT_MICROSANDBOX_STATE_ROOT", true),
          executable: required_path("OCTACITY_CONTRACT_MICROSANDBOX_EXECUTABLE", true),
          libkrunfw: required_path("OCTACITY_CONTRACT_MICROSANDBOX_LIBKRUNFW", true),
          environment_identity: required_string("OCTACITY_CONTRACT_MICROSANDBOX_ENVIRONMENT_IDENTITY"),
          image: required_string("OCTACITY_CONTRACT_MICROSANDBOX_IMAGE"),
          workspace_bytes: required_u64("OCTACITY_CONTRACT_WORKSPACE_BYTES"),
        }
      }
      "containerd" => {
        assert_eq!(env::consts::OS, "linux", "containerd release gate requires Linux");
        Self::Containerd {
          work_root: required_path("OCTACITY_CONTRACT_CONTAINERD_WORK_ROOT", true),
          cache_root: required_path("OCTACITY_CONTRACT_CONTAINERD_CACHE_ROOT", true),
          state_root: required_path("OCTACITY_CONTRACT_CONTAINERD_STATE_ROOT", true),
          endpoint: required_path("OCTACITY_CONTRACT_CONTAINERD_ENDPOINT", true),
          namespace: required_string("OCTACITY_CONTRACT_CONTAINERD_NAMESPACE"),
          snapshotter: required_string("OCTACITY_CONTRACT_CONTAINERD_SNAPSHOTTER"),
          runtime: required_string("OCTACITY_CONTRACT_CONTAINERD_RUNTIME"),
          registry_config_dir: env::var_os("OCTACITY_CONTRACT_CONTAINERD_REGISTRY_CONFIG_DIR").map(PathBuf::from),
          environment_identity: required_string("OCTACITY_CONTRACT_CONTAINERD_ENVIRONMENT_IDENTITY"),
          image: required_string("OCTACITY_CONTRACT_CONTAINERD_IMAGE"),
          workspace_bytes: required_u64("OCTACITY_CONTRACT_WORKSPACE_BYTES"),
        }
      }
      "apple-vf-isolation" => {
        assert_eq!(env::consts::OS, "macos", "Apple VF release gate requires macOS");
        assert_eq!(env::consts::ARCH, "aarch64", "Apple VF release gate requires ARM64");
        Self::AppleVf {
          work_root: required_path("OCTACITY_CONTRACT_APPLE_VF_WORK_ROOT", true),
          cache_root: required_path("OCTACITY_CONTRACT_APPLE_VF_CACHE_ROOT", true),
          state_root: required_path("OCTACITY_CONTRACT_APPLE_VF_STATE_ROOT", true),
          executable: required_path("OCTACITY_CONTRACT_APPLE_VF_EXECUTABLE", true),
          environment_identity: required_string("OCTACITY_CONTRACT_APPLE_VF_ENVIRONMENT_IDENTITY"),
          image: required_string("OCTACITY_CONTRACT_APPLE_VF_IMAGE"),
          workspace_bytes: required_u64("OCTACITY_CONTRACT_WORKSPACE_BYTES"),
        }
      }
      value => panic!("unsupported OCTACITY_RELEASE_BACKEND '{value}'"),
    }
  }

  pub(super) fn name(&self) -> &'static str {
    match self {
      Self::Native { .. } => "native",
      Self::Microsandbox { .. } => "microsandbox",
      Self::Containerd { .. } => "containerd",
      Self::AppleVf { .. } => "apple-vf-isolation",
    }
  }

  pub(super) fn agent_platform(&self) -> (&'static str, &'static str) {
    match self {
      Self::Native { .. } | Self::Containerd { .. } => ("linux", host_architecture()),
      Self::Microsandbox { .. } => (env::consts::OS, host_architecture()),
      Self::AppleVf { .. } => ("macos", "arm64"),
    }
  }

  pub(super) fn guest_architecture(&self) -> &'static str {
    match self {
      Self::AppleVf { .. } => "arm64",
      Self::Native { .. } | Self::Microsandbox { .. } | Self::Containerd { .. } => host_architecture(),
    }
  }

  pub(super) fn runtime_class(&self) -> &'static str {
    match self {
      Self::Native { .. } => "native",
      Self::Microsandbox { .. } => "virtualization",
      Self::Containerd { .. } => "isolation",
      Self::AppleVf { .. } => "isolation",
    }
  }

  pub(super) fn legacy_capability(&self) -> Option<&'static str> {
    match self {
      Self::Native { .. } => Some("native"),
      Self::Microsandbox { .. } => None,
      Self::Containerd { .. } => None,
      Self::AppleVf { .. } => None,
    }
  }

  pub(super) fn required_capabilities(&self) -> Vec<&'static str> {
    self.legacy_capability().into_iter().chain(["shell"]).collect()
  }

  pub(super) fn host_platform(&self) -> Value {
    match self {
      Self::Microsandbox { .. } => json!({
        "os": env::consts::OS,
        "architecture": host_architecture()
      }),
      Self::Containerd { .. } => json!({
        "os": "linux",
        "architecture": host_architecture()
      }),
      Self::AppleVf { .. } => json!({
        "os": "macos",
        "architecture": "arm64"
      }),
      Self::Native { .. } => Value::Null,
    }
  }

  pub(super) fn required_guarantees(&self) -> Value {
    match self {
      Self::Microsandbox { .. } => json!([
        "filesystem_isolation",
        "process_isolation",
        "network_isolation",
        "resource_isolation",
        "hardware_virtualization"
      ]),
      Self::Containerd { .. } | Self::AppleVf { .. } => json!([
        "filesystem_isolation",
        "process_isolation",
        "network_isolation",
        "resource_isolation"
      ]),
      Self::Native { .. } => json!([]),
    }
  }

  pub(super) fn execution_target(&self) -> Option<Value> {
    match self {
      Self::Microsandbox { .. } | Self::Containerd { .. } | Self::AppleVf { .. } => Some(json!({
        "mode": self.runtime_class(),
        "host_platform": self.host_platform(),
        "target_platform": {
          "os": "linux",
          "architecture": self.guest_architecture()
        },
        "required_guarantees": self.required_guarantees()
      })),
      Self::Native { .. } => None,
    }
  }

  pub(super) fn pool_admission_policy(&self) -> Value {
    let (operating_system, architecture) = self.agent_platform();
    let platforms = json!([{
      "operating_system": operating_system,
      "architecture": architecture
    }]);
    match self.execution_target() {
      Some(target) => json!({
        "mode": "execution_allowlist",
        "platforms": platforms,
        "execution_targets": [target]
      }),
      None => json!({"mode": "allowlist", "platforms": platforms}),
    }
  }

  pub(super) fn immutable_image(&self) -> Value {
    match self {
      Self::Native { .. } => Value::Null,
      Self::Microsandbox { image, .. } | Self::Containerd { image, .. } | Self::AppleVf { image, .. } => {
        Value::String(image.clone())
      }
    }
  }

  pub(super) fn workspace_bytes(&self) -> u64 {
    match self {
      Self::Native { workspace_bytes, .. }
      | Self::Microsandbox { workspace_bytes, .. }
      | Self::Containerd { workspace_bytes, .. }
      | Self::AppleVf { workspace_bytes, .. } => *workspace_bytes,
    }
  }

  pub(super) fn resource_sample_timeout_seconds(&self) -> u64 {
    match self {
      // Apple's CLI collects two samples for a non-streaming stats request.
      Self::AppleVf { .. } => 5,
      Self::Native { .. } | Self::Microsandbox { .. } | Self::Containerd { .. } => 1,
    }
  }
}
