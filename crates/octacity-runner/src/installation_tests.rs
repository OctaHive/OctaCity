//! Runner release inventory and trust-boundary tests.

use super::*;
use std::collections::BTreeMap;

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
