//! Operator-owned local Git transport for the packaged PostgreSQL pilot.
#![cfg(unix)]
use octacity_protocol::SourceSpec;
use octacity_source::{SourceMaterializationRequest, SourceMaterializer, SourcePluginRegistry};
use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
  process::Command,
  time::Duration,
};
use tokio_util::sync::CancellationToken;
#[path = "factory_pilot/source.rs"]
mod source;
fn required(name: &str) -> PathBuf {
  PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("{name} is required")))
}
fn git(git: &Path, repository: &Path, args: &[&str]) -> String {
  let output = Command::new(git)
    .env_clear()
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_CONFIG_GLOBAL", "/dev/null")
    .arg("-C")
    .arg(repository)
    .args([
      "-c",
      "core.hooksPath=/dev/null",
      "-c",
      "user.name=Pilot",
      "-c",
      "user.email=pilot@example.test",
    ])
    .args(args)
    .output()
    .unwrap();
  assert!(output.status.success(), "local fixture Git operation failed");
  String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
#[tokio::test]
#[ignore = "requires a verified released SourceGit package and real system Git"]
async fn operator_local_mirror_materializes_the_pinned_https_source_without_changing_its_release() {
  let root = tempfile::tempdir().unwrap();
  let repository = root.path().join("repository");
  fs::create_dir(&repository).unwrap();
  let system_git = required("OCTACITY_CONTRACT_GIT_EXECUTABLE");
  git(
    &system_git,
    &repository,
    &["init", "--quiet", "--initial-branch=main", "--template="],
  );
  fs::write(repository.join("source.txt"), b"immutable pilot source\n").unwrap();
  git(&system_git, &repository, &["add", "source.txt"]);
  git(&system_git, &repository, &["commit", "--quiet", "-m", "declared input"]);
  let revision = git(&system_git, &repository, &["rev-parse", "HEAD"]);
  let released = required("OCTACITY_RELEASE_AGENT_ROOT").join("source-plugins");
  let original = SourcePluginRegistry::discover(&released).unwrap();
  let original = original.get("git").unwrap();
  let original_manifest = fs::read(original.executable.parent().unwrap().join("plugin.toml")).unwrap();
  let logical = format!("https://pilot.invalid/{}.git", uuid::Uuid::new_v4().simple());
  let installed = source::install_local_mirror(
    &root.path().join("operator"),
    &released,
    &logical,
    &repository,
    &system_git,
  );
  let registry = SourcePluginRegistry::discover(&installed).unwrap();
  let plugin = registry.get("git").unwrap();
  assert_eq!(plugin.manifest.sha256, original.manifest.sha256);
  assert_eq!(
    fs::read(original.executable.parent().unwrap().join("plugin.toml")).unwrap(),
    original_manifest
  );
  assert_eq!(
    fs::read(&plugin.executable).unwrap(),
    fs::read(&original.executable).unwrap()
  );
  let requirement = SourceSpec {
    provider: plugin.manifest.name.clone(),
    plugin_version: plugin.manifest.version.clone(),
    plugin_sha256: plugin.manifest.sha256.clone(),
    revision: revision.clone(),
    reference: None,
    parameters: BTreeMap::from([("url".to_owned(), serde_json::json!(logical))]),
  };
  registry.verify(&requirement).unwrap();
  let request = |name: &str, revision: String, url: &str| {
    let destination = root.path().join(name);
    fs::create_dir(&destination).unwrap();
    SourceMaterializationRequest {
      request_id: name.to_owned(),
      destination,
      revision,
      reference: None,
      parameters: BTreeMap::from([("url".to_owned(), serde_json::json!(url))]),
      credential_files: BTreeMap::new(),
      max_workspace_bytes: 4 * 1024 * 1024,
    }
  };
  let result = registry
    .materialize(
      &requirement,
      request("checkout", revision.clone(), &logical),
      Duration::from_secs(20),
      Duration::from_secs(1),
      CancellationToken::new(),
    )
    .await
    .unwrap();
  assert_eq!(result.revision, revision);
  assert_eq!(
    fs::read(root.path().join("checkout/source.txt")).unwrap(),
    b"immutable pilot source\n"
  );
  let error = registry
    .materialize(
      &requirement,
      request("missing-revision", "f".repeat(40), &logical),
      Duration::from_secs(20),
      Duration::from_secs(1),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();
  assert!(!error.to_string().contains(&logical));
  let error = registry
    .materialize(
      &requirement,
      request(
        "credential-rejected",
        revision,
        "https://private-user:private-credential-canary@pilot.invalid/forbidden.git",
      ),
      Duration::from_secs(20),
      Duration::from_secs(1),
      CancellationToken::new(),
    )
    .await
    .unwrap_err();
  assert!(!error.to_string().contains("private-credential-canary"));
  assert!(!error.to_string().contains("private-user"));
}
