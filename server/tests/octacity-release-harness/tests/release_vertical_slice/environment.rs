//! Environment and private-file helpers shared by released-product scenarios.
use super::*;

pub(super) fn host_architecture() -> &'static str {
  if cfg!(target_arch = "aarch64") {
    "arm64"
  } else {
    "amd64"
  }
}

pub(super) fn required_string(name: &str) -> String {
  env::var(name)
    .unwrap_or_else(|_| panic!("{name} must be set"))
    .trim()
    .to_owned()
}
pub(super) fn required_u64(name: &str) -> u64 {
  required_string(name)
    .parse()
    .unwrap_or_else(|_| panic!("{name} must be a positive integer"))
}
pub(super) fn required_path(name: &str, must_exist: bool) -> PathBuf {
  let path = PathBuf::from(required_string(name));
  assert!(path.is_absolute(), "{name} must be absolute");
  if must_exist {
    assert!(path.exists(), "{name} does not exist: {path:?}");
  }
  path
}
pub(super) fn private_file(directory: &Path, name: &str, contents: &str) -> PathBuf {
  let path = directory.join(name);
  fs::write(&path, contents).unwrap();
  set_mode(&path, 0o600);
  path
}
pub(super) fn set_mode(path: &Path, mode: u32) {
  use std::os::unix::fs::PermissionsExt as _;
  fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
pub(super) fn toml_string(path: &Path) -> String {
  toml_text(path.to_str().expect("release paths must be UTF-8"))
}
pub(super) fn toml_text(value: &str) -> String {
  serde_json::to_string(value).unwrap()
}
