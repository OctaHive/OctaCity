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
    image: String,
    workspace_bytes: u64,
  },
  Containerd {
    work_root: PathBuf,
    state_root: PathBuf,
    endpoint: PathBuf,
    namespace: String,
    snapshotter: String,
    runtime: String,
    registry_config_dir: Option<PathBuf>,
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
          image: required_string("OCTACITY_CONTRACT_MICROSANDBOX_IMAGE"),
          workspace_bytes: required_u64("OCTACITY_CONTRACT_WORKSPACE_BYTES"),
        }
      }
      "containerd" => {
        assert_eq!(env::consts::OS, "linux", "containerd release gate requires Linux");
        Self::Containerd {
          work_root: required_path("OCTACITY_CONTRACT_CONTAINERD_WORK_ROOT", true),
          state_root: required_path("OCTACITY_CONTRACT_CONTAINERD_STATE_ROOT", true),
          endpoint: required_path("OCTACITY_CONTRACT_CONTAINERD_ENDPOINT", true),
          namespace: required_string("OCTACITY_CONTRACT_CONTAINERD_NAMESPACE"),
          snapshotter: required_string("OCTACITY_CONTRACT_CONTAINERD_SNAPSHOTTER"),
          runtime: required_string("OCTACITY_CONTRACT_CONTAINERD_RUNTIME"),
          registry_config_dir: env::var_os("OCTACITY_CONTRACT_CONTAINERD_REGISTRY_CONFIG_DIR").map(PathBuf::from),
          image: required_string("OCTACITY_CONTRACT_CONTAINERD_IMAGE"),
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
    }
  }

  pub(super) fn agent_platform(&self) -> (&'static str, &'static str) {
    (env::consts::OS, host_architecture())
  }

  pub(super) fn guest_architecture(&self) -> &'static str {
    host_architecture()
  }

  pub(super) fn runtime_class(&self) -> &'static str {
    match self {
      Self::Native { .. } => "native",
      Self::Microsandbox { .. } => "oci_hypervisor",
      Self::Containerd { .. } => "oci_process",
    }
  }

  pub(super) fn capability(&self) -> &'static str {
    match self {
      Self::Native { .. } => "native",
      Self::Microsandbox { .. } => "oci.hypervisor",
      Self::Containerd { .. } => "oci.process",
    }
  }

  pub(super) fn immutable_image(&self) -> Value {
    match self {
      Self::Native { .. } => Value::Null,
      Self::Microsandbox { image, .. } | Self::Containerd { image, .. } => Value::String(image.clone()),
    }
  }

  pub(super) fn workspace_bytes(&self) -> u64 {
    match self {
      Self::Native { workspace_bytes, .. }
      | Self::Microsandbox { workspace_bytes, .. }
      | Self::Containerd { workspace_bytes, .. } => *workspace_bytes,
    }
  }
}
