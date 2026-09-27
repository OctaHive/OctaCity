//! Provider-neutral virtualization configuration tests.

use std::fs::File;

use super::*;

#[test]
fn validates_microsandbox_independently_of_legacy_oci() {
  let mut fixture = Fixture::new();
  let executable = fixture._temp.path().join("msb");
  let libkrunfw = fixture._temp.path().join("libkrunfw");
  File::create(&executable).unwrap();
  File::create(&libkrunfw).unwrap();
  fixture.config.virtualization_providers = vec![VirtualizationProviderConfig::Microsandbox {
    environment_identity: ExecutionEnvironmentId::new("microsandbox-linux-guest-v1").unwrap(),
    executable: executable.clone(),
    libkrunfw: libkrunfw.clone(),
    metrics_sample_interval_seconds: 1,
  }];

  let validated = fixture.config.clone().validate().unwrap();
  assert!(
    validated
      .runtimes
      .iter()
      .any(|runtime| matches!(runtime, ValidatedRuntimeConfig::Virtualization { .. }))
  );

  fixture.config.enabled_runtime_modes.push(RuntimeMode::Oci);
  fixture.config.oci_engines = vec![OciEngineConfig::Microsandbox {
    executable,
    libkrunfw,
    metrics_sample_interval_seconds: 1,
  }];
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("legacy OCI and v2 virtualization")
  );
}
