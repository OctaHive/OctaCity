//! Private operator transport for a local pilot mirror; not remote Git qualification.
use octacity_source::SourcePluginRegistry;
use std::{
  fs,
  os::unix::fs::PermissionsExt as _,
  path::{Path, PathBuf},
};

/// Copies a verified plugin unchanged and supplies one private Git transport rewrite.
/// Only this test-owned installation opts into file transport; repository metadata
/// remains an HTTPS locator and the released bundle is never modified.
pub fn install_local_mirror(
  directory: &Path,
  released_plugins: &Path,
  remote: &str,
  repository: &Path,
  git: &Path,
) -> PathBuf {
  let remote_url = url::Url::parse(remote).unwrap();
  assert_eq!(remote_url.scheme(), "https");
  assert!(
    remote_url.username().is_empty()
      && remote_url.password().is_none()
      && remote_url.query().is_none()
      && remote_url.fragment().is_none()
  );
  let registry = SourcePluginRegistry::discover(released_plugins).unwrap();
  let original = registry.get("git").expect("a verified SourceGit release is required");
  let mut manifest = original.manifest.clone();
  octacity_private_fs::create_private_directory(directory).unwrap();
  let directory = directory.canonicalize().unwrap();
  let wrapper = directory.join("git-mirror");
  let local = url::Url::from_directory_path(repository.canonicalize().unwrap()).unwrap();
  // Git's reviewed per-command rewrite leaves its ordinary credential isolation,
  // hooks policy and the Source process protocol intact. No ambient Git config.
  let rewrite = format!("url.{}.insteadOf={remote}", local.as_str().trim_end_matches('/'));
  fs::write(
    &wrapper,
    format!(
      "#!/bin/sh\nexec {} -c {} \"$@\"\n",
      quote(git.canonicalize().unwrap().to_str().unwrap()),
      quote(&rewrite)
    ),
  )
  .unwrap();
  fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
  manifest
    .settings
    .insert("git_path".to_owned(), serde_json::json!(wrapper));
  manifest
    .settings
    .insert("allow_file".to_owned(), serde_json::json!(true));
  let root = directory.join("source-plugins");
  octacity_private_fs::create_private_directory(&root).unwrap();
  let plugin = root.join(&manifest.name);
  octacity_private_fs::create_private_directory(&plugin).unwrap();
  let executable = plugin.join(&manifest.executable);
  if let Some(parent) = executable.parent() {
    fs::create_dir_all(parent).unwrap();
  }
  fs::copy(&original.executable, &executable).unwrap();
  fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
  let manifest_path = plugin.join("plugin.toml");
  fs::write(&manifest_path, toml::to_string(&manifest).unwrap()).unwrap();
  fs::set_permissions(&manifest_path, fs::Permissions::from_mode(0o600)).unwrap();
  SourcePluginRegistry::discover(&root).expect("the copied executable must retain its released digest");
  root
}
fn quote(value: &str) -> String {
  format!("'{}'", value.replace('\'', "'\\''"))
}
