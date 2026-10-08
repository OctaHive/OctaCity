//! Runner release inventory and trust-boundary tests.

use super::*;
use octacity_private_fs::test_support::PrivateDirectoryFixture;
use std::collections::BTreeMap;

fn write_executable(path: &Path, contents: &[u8]) {
  fs::write(path, contents).unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
  }
}

fn linux_x86_64_executable_with_version(version: &str) -> Vec<u8> {
  let mut bytes = vec![0_u8; 64];
  bytes[..6].copy_from_slice(b"\x7fELF\x02\x01");
  bytes[18..20].copy_from_slice(&62_u16.to_le_bytes());
  bytes.extend_from_slice(version.as_bytes());
  bytes
}

fn codex_release() -> (PrivateDirectoryFixture, RunnerInstallation, PathBuf) {
  let release = PrivateDirectoryFixture::new().unwrap();
  fs::create_dir(release.path().join("plugins")).unwrap();
  let plugin = release.path().join("plugins/octa_plugin_codex");
  write_executable(&plugin, b"codex plugin fixture");
  let plugin_digest = file_sha256(&plugin).unwrap();
  fs::write(
    release.path().join("plugins/codex.plugin.yml"),
    "manifest_version: 1\nname: codex\n",
  )
  .unwrap();
  fs::write(
    release.path().join("Octa.lock"),
    format!(
      "version: 1\nplugins:\n  codex:\n    version: '0.5.1'\n    protocol: 2\n    platforms: [linux-x86_64]\n    entrypoint: octa_plugin_codex\n    sha256: {plugin_digest}\n    capabilities: [codex.blocking-pre-tool-authorization.v1]\n    source: codex.plugin.yml\n"
    ),
  )
  .unwrap();
  let runner = release.path().join(runner_filename());
  write_executable(&runner, b"runner fixture");
  fs::write(
    release.path().join(RUNNER_CAPABILITIES_FILE),
    r#"{"type":"capabilities","octa_version":"0.5.1","runner_protocols":[1],"event_schemas":[3],"plugin_protocols":[2],"octafile_versions":[1],"platform":"linux-x86_64","features":[]}"#,
  )
  .unwrap();
  fs::write(
    release.path().join(CODEX_COMPATIBILITY_FILE),
    serde_json::to_vec_pretty(&serde_json::json!({
      "format_version": 1,
      "plugin": {
        "name": "codex",
        "version": "0.5.1",
        "protocol": 2,
        "manifest": "plugins/codex.plugin.yml"
      },
      "executable": {
        "product": "codex-cli",
        "supported_versions": ["0.161.0"],
        "selection_environment": "OCTA_CODEX_EXECUTABLE"
      }
    }))
    .unwrap(),
  )
  .unwrap();
  let installation = RunnerInstallation::load(release.path()).unwrap();
  let executable_root = release.path().join("operator-tools");
  octacity_private_fs::create_private_directory(&executable_root).unwrap();
  let executable = executable_root.join(if cfg!(windows) { "codex.exe" } else { "codex" });
  write_executable(&executable, &linux_x86_64_executable_with_version("codex-cli 0.161.0"));
  (release, installation, executable)
}

fn capabilities() -> RunnerCapabilities {
  RunnerCapabilities {
    octa_version: "0.3.0".to_owned(),
    runner_protocols: vec![1],
    event_schemas: vec![3],
    plugin_protocols: vec![1],
    octafile_versions: vec![1],
    platform: "linux-x86_64".to_owned(),
    features: vec!["versioned-events".to_owned()],
    build_commit: None,
  }
}

#[test]
fn validates_capability_sets() {
  assert!(validate_capabilities(&capabilities()).is_ok());
  let mut invalid = capabilities();
  invalid.runner_protocols = vec![1, 1];
  assert!(validate_capabilities(&invalid).is_err());
  invalid = capabilities();
  invalid.features = vec![String::new()];
  assert!(validate_capabilities(&invalid).is_err());
  invalid = capabilities();
  invalid.octafile_versions = vec![0];
  assert!(validate_capabilities(&invalid).is_err());
}

#[test]
fn validates_distribution_identifiers_and_paths() {
  assert!(logical_name("shell_2"));
  assert!(!logical_name("Shell"));
  assert!(relative_path(Path::new("bin/shell")));
  assert!(!relative_path(Path::new("../shell")));
  assert!(!relative_path(Path::new("bin\\shell")));
  assert!(sha256_digest(&"a".repeat(64)));
  assert!(!sha256_digest(&"A".repeat(64)));
}

#[test]
fn rejects_non_capability_and_oversized_manifests() {
  let temporary = tempfile::tempdir().unwrap();
  let manifest = temporary.path().join(RUNNER_CAPABILITIES_FILE);
  fs::write(
      &manifest,
      r#"{"type":"hello","protocol_version":1,"octa_version":"0.3.0","event_schema_version":3,"plugin_protocol_version":1}"#,
    )
    .unwrap();
  assert!(
    load_capabilities(&manifest)
      .unwrap_err()
      .to_string()
      .contains("not a capabilities message")
  );

  fs::write(&manifest, vec![b' '; MAX_CAPABILITIES_BYTES + 1]).unwrap();
  assert!(
    load_capabilities(&manifest)
      .unwrap_err()
      .to_string()
      .contains("exceeds")
  );
}

#[cfg(unix)]
#[test]
fn rejects_operator_files_writable_by_other_users() {
  use std::os::unix::fs::PermissionsExt as _;

  let temporary = tempfile::tempdir().unwrap();
  let file = temporary.path().join("runner");
  fs::write(&file, "runner").unwrap();
  fs::set_permissions(&file, fs::Permissions::from_mode(0o777)).unwrap();
  assert!(
    validate_regular_file("runner", &file, true)
      .unwrap_err()
      .to_string()
      .contains("writable by group")
  );
}

#[test]
fn verifies_external_executable_against_release_compatibility() {
  let (_release, installation, executable) = codex_release();
  let digest = file_sha256(&executable).unwrap();
  let configured = ConfiguredExternalExecutable {
    product: "codex-cli".to_owned(),
    version: "0.161.0".to_owned(),
    platform: "linux-x86_64".to_owned(),
    executable: executable.clone(),
    sha256: digest.clone(),
  };

  let verified = installation.verify_external_executables(&[configured]).unwrap();
  assert_eq!(verified["codex-cli"].executable, executable.canonicalize().unwrap());
  assert_eq!(verified["codex-cli"].sha256, digest);
  assert_eq!(verified["codex-cli"].selection_environment, "OCTA_CODEX_EXECUTABLE");
  assert_eq!(
    installation
      .program_with_external_executables(&verified)
      .external_executables,
    BTreeMap::from([("OCTA_CODEX_EXECUTABLE".to_owned(), executable.canonicalize().unwrap())])
  );
}

#[test]
fn revalidation_rejects_plugin_and_executable_drift_before_spawn() {
  let (_release, installation, executable) = codex_release();
  let configured = ConfiguredExternalExecutable {
    product: "codex-cli".to_owned(),
    version: "0.161.0".to_owned(),
    platform: "linux-x86_64".to_owned(),
    executable: executable.clone(),
    sha256: file_sha256(&executable).unwrap(),
  };
  let verified = installation.verify_external_executables(&[configured]).unwrap();
  installation.revalidate_files(&verified).unwrap();

  fs::write(
    &executable,
    linux_x86_64_executable_with_version("codex-cli 0.161.0 drift"),
  )
  .unwrap();
  assert!(installation.revalidate_files(&verified).is_err());

  let (_release, installation, _executable) = codex_release();
  let plugin = installation.plugins["codex"].executable.clone();
  fs::write(plugin, b"changed plugin fixture").unwrap();
  assert!(installation.revalidate_files(&BTreeMap::new()).is_err());
}

#[test]
fn permits_ordinary_jobs_without_an_external_executable_configuration() {
  let (_release, installation, _executable) = codex_release();

  assert!(installation.verify_external_executables(&[]).unwrap().is_empty());
  assert!(installation.program().external_executables.is_empty());
}

#[test]
fn rejects_compatibility_metadata_drift_after_release_inventory() {
  let (release, installation, _executable) = codex_release();
  fs::write(
    release.path().join(CODEX_COMPATIBILITY_FILE),
    r#"{"format_version":1,"plugin":{"name":"codex","version":"0.5.1","protocol":1,"manifest":"plugins/codex.plugin.yml"},"executable":{"product":"codex-cli","supported_versions":["0.161.0"],"selection_environment":"OCTA_CODEX_EXECUTABLE"}}"#,
  )
  .unwrap();

  assert!(
    installation
      .verify_external_executables(&[])
      .unwrap_err()
      .to_string()
      .contains("differs from the installed plugin")
  );
}

#[test]
fn rejects_absent_wrong_version_digest_and_platform_external_executables() {
  let (release, installation, executable) = codex_release();
  let digest = file_sha256(&executable).unwrap();
  let configured = ConfiguredExternalExecutable {
    product: "codex-cli".to_owned(),
    version: "0.161.0".to_owned(),
    platform: "linux-x86_64".to_owned(),
    executable,
    sha256: digest,
  };

  let mut absent = configured.clone();
  absent.executable = release.path().join("missing-codex");
  assert!(
    installation
      .verify_external_executables(&[absent])
      .unwrap_err()
      .to_string()
      .contains("external executable 'codex-cli'")
  );

  let mut wrong_version = configured.clone();
  wrong_version.version = "0.129.0".to_owned();
  assert!(
    installation
      .verify_external_executables(&[wrong_version])
      .unwrap_err()
      .to_string()
      .contains("is not supported")
  );

  let mut wrong_digest = configured.clone();
  wrong_digest.sha256 = "f".repeat(64);
  assert!(
    installation
      .verify_external_executables(&[wrong_digest])
      .unwrap_err()
      .to_string()
      .contains("digest differs")
  );

  let mut wrong_platform = configured;
  wrong_platform.platform = "linux-aarch64".to_owned();
  assert!(
    installation
      .verify_external_executables(&[wrong_platform])
      .unwrap_err()
      .to_string()
      .contains("differs from Octa runner platform")
  );
}

#[test]
fn rejects_binary_platform_and_version_claims_not_present_in_the_executable() {
  let (_release, installation, executable) = codex_release();
  write_executable(&executable, &linux_x86_64_executable_with_version("codex-cli 0.129.0"));
  let digest = file_sha256(&executable).unwrap();
  let wrong_version_bytes = ConfiguredExternalExecutable {
    product: "codex-cli".to_owned(),
    version: "0.161.0".to_owned(),
    platform: "linux-x86_64".to_owned(),
    executable: executable.clone(),
    sha256: digest,
  };
  assert!(
    installation
      .verify_external_executables(&[wrong_version_bytes])
      .unwrap_err()
      .to_string()
      .contains("version identity")
  );

  let mut arm = linux_x86_64_executable_with_version("codex-cli 0.161.0");
  arm[18..20].copy_from_slice(&183_u16.to_le_bytes());
  write_executable(&executable, &arm);
  let wrong_platform_bytes = ConfiguredExternalExecutable {
    product: "codex-cli".to_owned(),
    version: "0.161.0".to_owned(),
    platform: "linux-x86_64".to_owned(),
    sha256: file_sha256(&executable).unwrap(),
    executable,
  };
  assert!(
    installation
      .verify_external_executables(&[wrong_platform_bytes])
      .unwrap_err()
      .to_string()
      .contains("binary platform")
  );
}

#[cfg(unix)]
#[test]
fn rejects_external_executable_writable_by_other_users() {
  use std::os::unix::fs::PermissionsExt as _;

  let (_release, installation, executable) = codex_release();
  let digest = file_sha256(&executable).unwrap();
  fs::set_permissions(&executable, fs::Permissions::from_mode(0o777)).unwrap();
  let error = installation
    .verify_external_executables(&[ConfiguredExternalExecutable {
      product: "codex-cli".to_owned(),
      version: "0.161.0".to_owned(),
      platform: "linux-x86_64".to_owned(),
      executable,
      sha256: digest,
    }])
    .unwrap_err();
  assert!(error.to_string().contains("writable by group"));
}

#[cfg(unix)]
#[test]
fn inventories_and_matches_a_cross_platform_release_without_importing_octa() {
  use std::os::unix::fs::PermissionsExt as _;

  let release = tempfile::tempdir().unwrap();
  fs::create_dir(release.path().join("plugins")).unwrap();
  let plugin = release.path().join("plugins/shell");
  fs::write(&plugin, "fixture plugin").unwrap();
  fs::set_permissions(&plugin, fs::Permissions::from_mode(0o755)).unwrap();
  let plugin_digest = file_sha256(&plugin).unwrap();
  fs::write(
      release.path().join("Octa.lock"),
      format!(
        "version: 1\nplugins:\n  shell:\n    version: '0.3.0'\n    protocol: 1\n    platforms: [linux-x86_64]\n    entrypoint: shell\n    sha256: {plugin_digest}\n    capabilities: [shell]\n    source: shell.plugin.yml\n"
      ),
    )
    .unwrap();
  let runner = release.path().join(runner_filename());
  fs::write(&runner, "foreign-platform runner fixture").unwrap();
  fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
  fs::write(
      release.path().join(RUNNER_CAPABILITIES_FILE),
      r#"{"type":"capabilities","octa_version":"0.3.0","runner_protocols":[1],"event_schemas":[3],"plugin_protocols":[1],"octafile_versions":[1],"platform":"linux-x86_64","features":["versioned-events"]}"#,
    )
    .unwrap();

  let installation = RunnerInstallation::load(release.path()).unwrap();
  let requirement = OctaSpec {
    version: "0.3.0".to_owned(),
    runner_sha256: installation.sha256.clone(),
    runner_protocol: 1,
    event_schema: 3,
    plugin_protocol: 1,
    plugin_digests: BTreeMap::from([("shell".to_owned(), plugin_digest.clone())]),
  };
  installation.verify(&requirement).unwrap();
  assert_eq!(installation.plugins.len(), 1);
  assert_eq!(
    installation.program(),
    RunnerProgram {
      release_root: installation.root.clone(),
      executable: installation.executable.clone(),
      plugins_dir: installation.plugins_dir.clone(),
      plugin_lock: installation.default_plugin_lock.clone(),
      external_executables: BTreeMap::new(),
    }
  );

  let mut wrong = requirement;
  wrong.event_schema = 2;
  assert!(installation.verify(&wrong).is_err());
  wrong = OctaSpec {
    version: "0.2.0".to_owned(),
    runner_sha256: installation.sha256.clone(),
    runner_protocol: 1,
    event_schema: 3,
    plugin_protocol: 1,
    plugin_digests: BTreeMap::from([("shell".to_owned(), plugin_digest.clone())]),
  };
  assert!(installation.verify(&wrong).is_err());
  wrong.version = "0.3.0".to_owned();
  wrong.runner_sha256 = "f".repeat(64);
  assert!(installation.verify(&wrong).is_err());
  wrong.runner_sha256 = installation.sha256.clone();
  wrong.plugin_digests.clear();
  assert!(installation.verify(&wrong).is_err());
  wrong.plugin_digests.insert("shell".to_owned(), "f".repeat(64));
  assert!(installation.verify(&wrong).is_err());
  wrong.plugin_digests.insert("shell".to_owned(), plugin_digest);
  wrong.plugin_protocol = 2;
  assert!(installation.verify(&wrong).is_err());
}
