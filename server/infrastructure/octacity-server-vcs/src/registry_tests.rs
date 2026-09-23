use std::{fs, path::Path};

use sha2::{Digest as _, Sha256};
use tempfile::TempDir;

use super::*;

#[test]
fn discovers_a_protocol_valid_adapter() {
  let root = TempDir::new().unwrap();
  let digest = install_adapter(root.path(), "fixture", b"adapter executable", "");

  let registry = VcsAdapterRegistry::discover(root.path()).unwrap();
  assert_eq!(registry.len(), 1);
  assert_eq!(registry.iter().next().unwrap().0, "fixture");
  assert_eq!(
    registry.resolve("fixture", &digest).unwrap().manifest.adapter_id,
    "fixture"
  );
}

#[test]
fn rejects_unknown_manifest_fields() {
  let root = TempDir::new().unwrap();
  install_adapter(root.path(), "unknown", b"adapter", "unexpected = true\n");
  assert!(matches!(
    VcsAdapterRegistry::discover(root.path()),
    Err(RegistryError::ParseManifest { .. })
  ));
}

fn install_adapter(root: &Path, id: &str, executable_bytes: &[u8], extra: &str) -> String {
  let directory = root.join(id);
  fs::create_dir(&directory).unwrap();
  let executable = directory.join("adapter");
  fs::write(&executable, executable_bytes).unwrap();
  make_executable(&executable);
  let digest = digest(executable_bytes);
  fs::write(directory.join("adapter.toml"), manifest(id, &digest, extra)).unwrap();
  digest
}

fn manifest(id: &str, digest: &str, extra: &str) -> String {
  format!(
    "manifest_version = 1\nadapter_id = \"{id}\"\nexecutable = \"adapter\"\nexecutable_sha256 = \"{digest}\"\n{extra}\n[protocol]\nmin = 1\nmax = 1\n\n[capabilities]\nlist_references = true\nread_commit = true\nlist_tree = true\nread_file = true\nresolve_revision = true\n"
  )
}

fn digest(bytes: &[u8]) -> String {
  format!("{:x}", Sha256::digest(bytes))
}

#[cfg(unix)]
fn make_executable(path: &Path) {
  use std::os::unix::fs::PermissionsExt as _;

  fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}
