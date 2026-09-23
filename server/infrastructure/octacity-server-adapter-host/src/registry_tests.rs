use std::{collections::BTreeMap, fs, path::Path};

#[cfg(unix)]
use sha2::Sha256;
use tempfile::TempDir;

use super::*;

#[derive(Debug)]
struct FixtureAdapter {
  executable: VerifiedExecutable,
}

impl RegistryAdapter for FixtureAdapter {
  fn load(_directory: &Path) -> Result<Self, RegistryError> {
    unreachable!("these tests construct the common registry directly")
  }

  fn adapter_id(&self) -> &str {
    self.executable.adapter_id()
  }

  fn executable_sha256(&self) -> &str {
    self.executable.sha256()
  }

  fn protocol_family() -> &'static str {
    "fixture"
  }
}

#[test]
fn exact_resolution_rejects_missing_adapters_and_digest_mismatches() {
  let adapter = FixtureAdapter {
    executable: VerifiedExecutable {
      adapter_id: "fixture".to_owned(),
      path: PathBuf::from("fixture"),
      sha256: "a".repeat(64),
    },
  };
  let registry = AdapterRegistry {
    adapters: BTreeMap::from([("fixture".to_owned(), adapter)]),
  };

  assert!(registry.resolve("fixture", &"a".repeat(64)).is_ok());
  assert!(matches!(
    registry.resolve("fixture", &"b".repeat(64)),
    Err(RegistryError::DigestMismatch { .. })
  ));
  assert!(matches!(
    registry.resolve("missing", &"a".repeat(64)),
    Err(RegistryError::NotInstalled { .. })
  ));
}

#[test]
fn executable_verification_rejects_digest_mismatches() {
  let root = TempDir::new().unwrap();
  let directory = root.path().join("fixture");
  fs::create_dir(&directory).unwrap();
  let executable = directory.join("adapter");
  fs::write(&executable, b"adapter").unwrap();
  make_executable(&executable);

  assert!(matches!(
    verify_executable(&directory, "fixture".to_owned(), "adapter", "0".repeat(64)),
    Err(RegistryError::InvalidEntry { .. })
  ));
}

#[cfg(unix)]
#[test]
fn executable_verification_rejects_symlinks() {
  use std::os::unix::fs::symlink;

  let root = TempDir::new().unwrap();
  let directory = root.path().join("fixture");
  fs::create_dir(&directory).unwrap();
  let target = root.path().join("target");
  fs::write(&target, b"adapter").unwrap();
  make_executable(&target);
  symlink(&target, directory.join("adapter")).unwrap();

  assert!(matches!(
    verify_executable(
      &directory,
      "fixture".to_owned(),
      "adapter",
      format!("{:x}", Sha256::digest(b"adapter")),
    ),
    Err(RegistryError::InvalidEntry { .. })
  ));
}

#[cfg(unix)]
fn make_executable(path: &Path) {
  use std::os::unix::fs::PermissionsExt as _;

  fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}
