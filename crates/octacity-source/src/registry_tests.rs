//! Tests for trusted source-plugin discovery and verification.

use std::io::Write as _;

use super::*;

fn plugin_fixture() -> (tempfile::TempDir, PathBuf) {
  let root = tempfile::tempdir().unwrap();
  let directory = root.path().join("git");
  fs::create_dir(&directory).unwrap();
  let executable = directory.join(if cfg!(windows) { "git.exe" } else { "git" });
  File::create(&executable).unwrap().write_all(b"plugin").unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
  }
  let digest = file_sha256(&executable).unwrap();
  let manifest = format!(
    r#"manifest_version = 1
name = "git"
version = "0.1.0"
protocol_min = 1
protocol_max = 1
executable = "{}"
sha256 = "{digest}"
platforms = ["{}-{}"]
"#,
    executable.file_name().unwrap().to_string_lossy(),
    std::env::consts::OS,
    std::env::consts::ARCH
  );
  fs::write(directory.join("plugin.toml"), manifest).unwrap();
  (root, executable)
}

#[test]
fn discovers_a_verified_plugin() {
  let (root, executable) = plugin_fixture();
  let registry = SourcePluginRegistry::discover(root.path()).unwrap();
  assert_eq!(registry.len(), 1);
  assert_eq!(
    registry.get("git").unwrap().executable,
    executable.canonicalize().unwrap()
  );
}

#[cfg(unix)]
#[test]
fn rejects_a_group_writable_registry_root() {
  use std::os::unix::fs::PermissionsExt as _;

  let (root, _) = plugin_fixture();
  fs::set_permissions(root.path(), fs::Permissions::from_mode(0o775)).unwrap();
  assert!(
    SourcePluginRegistry::discover(root.path())
      .unwrap_err()
      .to_string()
      .contains("must not be writable by group or others")
  );
}

#[test]
fn rejects_a_digest_mismatch() {
  let (root, executable) = plugin_fixture();
  File::options()
    .append(true)
    .open(executable)
    .unwrap()
    .write_all(b"changed")
    .unwrap();
  assert!(
    SourcePluginRegistry::discover(root.path())
      .unwrap_err()
      .to_string()
      .contains("SHA-256")
  );
}

#[test]
fn rejects_unexpected_registry_files() {
  let root = tempfile::tempdir().unwrap();
  fs::write(root.path().join("README"), "not a plugin").unwrap();
  assert!(
    SourcePluginRegistry::discover(root.path())
      .unwrap_err()
      .to_string()
      .contains("real directories")
  );
}

#[test]
fn resolves_only_the_exact_signed_plugin_requirement() {
  let (root, _) = plugin_fixture();
  let registry = SourcePluginRegistry::discover(root.path()).unwrap();
  let installed = registry.get("git").unwrap();
  let mut requirement = SourceSpec {
    provider: "git".to_owned(),
    plugin_version: installed.manifest.version.clone(),
    plugin_sha256: installed.manifest.sha256.clone(),
    revision: "revision".to_owned(),
    reference: None,
    parameters: BTreeMap::new(),
  };
  assert!(registry.resolve(&requirement).is_ok());

  requirement.plugin_sha256 = "0".repeat(64);
  assert!(
    registry
      .resolve(&requirement)
      .unwrap_err()
      .to_string()
      .contains("required digest")
  );

  requirement.plugin_sha256 = installed.manifest.sha256.clone();
  requirement.plugin_version = "9.9.9".to_owned();
  assert!(
    registry
      .resolve(&requirement)
      .unwrap_err()
      .to_string()
      .contains("required version")
  );

  requirement.provider = "mercurial".to_owned();
  assert!(matches!(
    registry.resolve(&requirement),
    Err(RegistryError::NotInstalled { .. })
  ));
}

#[test]
fn accepts_an_empty_registry_for_an_unschedulable_agent() {
  let root = tempfile::tempdir().unwrap();
  let registry = SourcePluginRegistry::discover(root.path()).unwrap();
  assert!(registry.is_empty());
  assert_eq!(registry.len(), 0);
}

#[test]
fn rejects_a_plugin_for_another_platform() {
  let (root, _) = plugin_fixture();
  let manifest = root.path().join("git/plugin.toml");
  let contents = fs::read_to_string(&manifest).unwrap();
  fs::write(
    &manifest,
    contents.replace(
      &format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
      "unsupported-platform",
    ),
  )
  .unwrap();
  assert!(
    SourcePluginRegistry::discover(root.path())
      .unwrap_err()
      .to_string()
      .contains("does not support platform")
  );
}

#[test]
fn rejects_each_invalid_manifest_identity_boundary() {
  let (root, _) = plugin_fixture();
  let directory = root.path().join("git");
  let source = fs::read_to_string(directory.join("plugin.toml")).unwrap();
  let valid = SourcePluginManifest::from_toml(&source).unwrap();

  let mut invalid = valid.clone();
  invalid.manifest_version += 1;
  assert!(
    validate_manifest(&directory, &invalid)
      .unwrap_err()
      .to_string()
      .contains("version")
  );
  invalid = valid.clone();
  invalid.name = "other".to_owned();
  assert!(
    validate_manifest(&directory, &invalid)
      .unwrap_err()
      .to_string()
      .contains("name")
  );
  invalid = valid.clone();
  invalid.version.clear();
  assert!(
    validate_manifest(&directory, &invalid)
      .unwrap_err()
      .to_string()
      .contains("version")
  );
  invalid = valid.clone();
  invalid.protocol_min = 0;
  assert!(
    validate_manifest(&directory, &invalid)
      .unwrap_err()
      .to_string()
      .contains("protocol")
  );
  invalid = valid.clone();
  invalid.executable = "../git".to_owned();
  assert!(
    validate_manifest(&directory, &invalid)
      .unwrap_err()
      .to_string()
      .contains("relative path")
  );
  invalid = valid;
  invalid.sha256 = "A".repeat(64);
  assert!(
    validate_manifest(&directory, &invalid)
      .unwrap_err()
      .to_string()
      .contains("lowercase")
  );

  assert!(logical_name("git_2"));
  assert!(!logical_name("Git"));
  assert!(relative_wire_path("bin/source-git"));
  assert!(!relative_wire_path("bin//source-git"));
  assert!(sha256_digest(&"a".repeat(64)));
  assert!(!sha256_digest(&"z".repeat(64)));
}

#[test]
fn rejects_missing_malformed_oversized_and_non_regular_plugin_files() {
  let missing = tempfile::tempdir().unwrap().path().join("missing");
  assert!(matches!(
    SourcePluginRegistry::discover(&missing),
    Err(RegistryError::InvalidEntry { .. })
  ));

  let non_directory_root = tempfile::NamedTempFile::new().unwrap();
  assert!(
    SourcePluginRegistry::discover(non_directory_root.path())
      .unwrap_err()
      .to_string()
      .contains("real directory")
  );

  let root = tempfile::tempdir().unwrap();
  let directory = root.path().join("git");
  fs::create_dir(&directory).unwrap();
  fs::write(directory.join("plugin.toml"), "not = [valid").unwrap();
  assert!(matches!(
    SourcePluginRegistry::discover(root.path()),
    Err(RegistryError::ParseManifest { .. })
  ));

  fs::write(
    directory.join("plugin.toml"),
    vec![b'x'; MAX_MANIFEST_BYTES as usize + 1],
  )
  .unwrap();
  assert!(
    SourcePluginRegistry::discover(root.path())
      .unwrap_err()
      .to_string()
      .contains("exceeds")
  );

  fs::remove_file(directory.join("plugin.toml")).unwrap();
  fs::create_dir(directory.join("plugin.toml")).unwrap();
  assert!(
    SourcePluginRegistry::discover(root.path())
      .unwrap_err()
      .to_string()
      .contains("regular file")
  );
}

#[cfg(unix)]
#[test]
fn rejects_a_non_executable_plugin_binary() {
  use std::os::unix::fs::PermissionsExt as _;

  let (root, executable) = plugin_fixture();
  fs::set_permissions(executable, fs::Permissions::from_mode(0o644)).unwrap();
  assert!(
    SourcePluginRegistry::discover(root.path())
      .unwrap_err()
      .to_string()
      .contains("execute bit")
  );
}
