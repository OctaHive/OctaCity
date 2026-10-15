//! Public configuration contract for a separately installed reader worker.

use super::*;
use octacity_protocol::{FactoryImmutableReferenceV3, FactoryOutputPermissionsV3, FactoryResourceLimitsV3};

fn configured_reader(fixture: &Fixture) -> ProtectedReaderConfig {
  let release_root = fixture._temp.path().join("reader-release");
  octacity_private_fs::create_private_directory(&release_root).unwrap();
  let executable = release_root.join("bounded-reader");
  fs::write(&executable, b"reader fixture").unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o500)).unwrap();
  }
  ProtectedReaderConfig {
    bridge: None,
    release_root,
    entrypoint: PathBuf::from("bounded-reader"),
    platform: "linux-aarch64".to_owned(),
    tool: FactoryImmutableReferenceV3 {
      identity: "source-reader".to_owned(),
      version: "1".to_owned(),
      sha256: "a".repeat(64),
    },
    executable: FactoryImmutableReferenceV3 {
      identity: "bounded-reader".to_owned(),
      version: "1".to_owned(),
      sha256: "b".repeat(64),
    },
    resources: FactoryResourceLimitsV3 {
      cpu_millis: 1000,
      memory_bytes: 256 * 1024 * 1024,
      disk_bytes: 16 * 1024 * 1024,
      process_count: 8,
      elapsed_millis: 5000,
    },
    outputs: FactoryOutputPermissionsV3 {
      kinds: vec![],
      max_artifact_count: 0,
      max_artifact_bytes: 0,
      max_report_count: 0,
      max_report_bytes: 0,
    },
  }
}

#[test]
fn keeps_standalone_reader_authority_separate_from_task_plugin_tools() {
  let mut fixture = Fixture::new();
  let reader = configured_reader(&fixture);
  let root = reader.release_root.clone();
  fixture.config.protected_reader = Some(reader);

  let validated = fixture.config.validate().unwrap();

  let installed = validated.config.protected_reader.unwrap();
  assert_eq!(installed.release_root, root.canonicalize().unwrap());
  assert_eq!(installed.entrypoint, PathBuf::from("bounded-reader"));
  assert_eq!(installed.platform, "linux-aarch64");
  assert!(validated.config.tool_executables.is_empty());
}

#[test]
fn reader_distribution_cannot_contain_private_factory_profiles() {
  let mut fixture = Fixture::new();
  let reader = configured_reader(&fixture);
  fixture.config.factory_credential_profiles.insert(
    "operator-model".to_owned(),
    reader.release_root.join("private-values.json"),
  );
  fixture.config.protected_reader = Some(reader);
  assert!(
    fixture.config.validate().is_err(),
    "reader distribution must not expose private profiles"
  );
}

#[test]
fn reader_distribution_cannot_project_job_storage_or_other_installed_authority() {
  for index in 0..6 {
    let mut fixture = Fixture::new();
    let mut reader = configured_reader(&fixture);
    reader.release_root = match index {
      0 => fixture.config.work_root.clone(),
      1 => fixture.config.state_root.clone(),
      2 => fixture.config.cache.root.clone(),
      3 => fixture.config.octa_release_root.clone(),
      4 => fixture.config.source_plugins_dir.clone(),
      _ => fixture.config.credential_file.parent().unwrap().to_owned(),
    };
    fs::write(reader.release_root.join("bounded-reader"), b"reader fixture").unwrap();
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      fs::set_permissions(
        reader.release_root.join("bounded-reader"),
        fs::Permissions::from_mode(0o500),
      )
      .unwrap();
    }
    fixture.config.protected_reader = Some(reader);

    assert!(
      fixture.config.validate().is_err(),
      "reader root must be isolated from root {index}"
    );
  }
}

#[test]
fn reader_entrypoint_must_be_a_portable_relative_path() {
  for path in ["../outside", "/bounded-reader", "bin\\reader", "C:reader", ""] {
    let mut fixture = Fixture::new();
    let mut reader = configured_reader(&fixture);
    reader.entrypoint = PathBuf::from(path);
    fixture.config.protected_reader = Some(reader);
    assert!(fixture.config.validate().is_err(), "entrypoint must be refused: {path}");
  }
}

#[test]
fn refuses_invalid_reader_identity_and_unbounded_technical_envelopes() {
  for index in 0..9 {
    let mut fixture = Fixture::new();
    let mut reader = configured_reader(&fixture);
    match index {
      0 => reader.executable.sha256 = "invalid".to_owned(),
      1 => reader.tool.identity.clear(),
      2 => reader.platform = "portable-script".to_owned(),
      3 => reader.resources.cpu_millis = 0,
      4 => reader.resources.memory_bytes = 0,
      5 => reader.resources.disk_bytes = 0,
      6 => reader.resources.process_count = 0,
      7 => reader.resources.elapsed_millis = 0,
      _ => reader.outputs.max_artifact_count = 1,
    }
    fixture.config.protected_reader = Some(reader);
    assert!(
      fixture.config.validate().is_err(),
      "invalid installed reader intent {index}"
    );
  }
}

#[test]
fn reader_distribution_cannot_contain_workload_identity_tokens() {
  let mut fixture = Fixture::new();
  let reader = configured_reader(&fixture);
  let token = reader.release_root.join("private-token");
  fs::write(&token, b"rotating-token").unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
  }
  fixture
    .config
    .workload_identity_profiles
    .insert("review".to_owned(), token);
  fixture.config.protected_reader = Some(reader);

  assert!(fixture.config.validate().is_err());
}
#[test]
fn controlled_reader_adapter_is_explicit_pinned_and_separate_from_worker_authority() {
  let mut fixture = Fixture::new();
  let mut reader = configured_reader(&fixture);
  let adapter = reader.release_root.join("coding-reader-adapter");
  fs::write(&adapter, b"adapter fixture").unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&adapter, fs::Permissions::from_mode(0o500)).unwrap();
  }
  reader.bridge = Some(ProtectedReaderBridgeConfig {
    entrypoint: PathBuf::from("coding-reader-adapter"),
    sha256: "c".repeat(64),
    hook_tool_name: "mcp__qualified_fs__read_file".to_owned(),
  });
  fixture.config.protected_reader = Some(reader);
  let configured = fixture.config.clone();
  let validated = configured.validate().unwrap();
  let installed = validated.config.protected_reader.unwrap();
  assert_eq!(
    installed.bridge.unwrap().entrypoint,
    PathBuf::from("coding-reader-adapter")
  );
  assert!(validated.config.tool_executables.is_empty());
  for invalid_path in ["../coding-reader-adapter", "/private/adapter", "./adapter"] {
    let mut config = fixture.config.clone();
    config
      .protected_reader
      .as_mut()
      .unwrap()
      .bridge
      .as_mut()
      .unwrap()
      .entrypoint = PathBuf::from(invalid_path);
    assert!(config.validate().is_err());
  }
}
