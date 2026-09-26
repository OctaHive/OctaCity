//! Adapter from release-scenario environment variables to the verified harness.

use octacity_release_harness::{InstalledRelease, ReleaseBundles, validate_installed};

use super::{ReleaseBackend, host_architecture, required_path};

pub(super) fn load(backend: &ReleaseBackend) -> InstalledRelease {
  let release = validate_installed(&ReleaseBundles {
    server: required_path("OCTACITY_RELEASE_SERVER_ROOT", true),
    agent: required_path("OCTACITY_RELEASE_AGENT_ROOT", true),
    octa: required_path("OCTACITY_CONTRACT_OCTA_RELEASE_ROOT", true),
  })
  .unwrap_or_else(|error| panic!("released installation failed validation: {error}"));

  let expected_product_platform = match backend {
    ReleaseBackend::Native { .. } => format!("linux-{}", host_architecture()),
    ReleaseBackend::Microsandbox { .. } => "macos-arm64".to_owned(),
  };
  assert_eq!(release.server_manifest.platform(), expected_product_platform);
  assert_eq!(release.agent_manifest.platform(), expected_product_platform);

  let expected_runner_platform = format!(
    "linux-{}",
    match backend.guest_architecture() {
      "amd64" => "x86_64",
      "arm64" => "aarch64",
      other => other,
    }
  );
  assert_eq!(release.octa_platform, expected_runner_platform);
  release
}
