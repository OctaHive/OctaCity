#![cfg(unix)]

use std::{
  collections::BTreeMap,
  fs,
  os::unix::fs::PermissionsExt as _,
  path::{Path, PathBuf},
};

use octacity_release_harness::{ReleaseBundles, install};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

#[test]
fn installs_only_verified_released_bundles() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let destination = temporary.path().join("scenario-installation");
  let installed = install(&bundles, &destination).unwrap();

  assert_eq!(installed.root, destination);
  assert!(installed.server_binary.starts_with(&installed.server_root));
  assert!(installed.agent_binary.starts_with(&installed.agent_root));
  assert!(installed.octa_runner.starts_with(&installed.octa_root));
  assert_eq!(installed.octa_platform, "linux-x86_64");
  assert_eq!(installed.octa_version, "0.4.0");
  assert_ne!(installed.server_root, bundles.server);
  assert_ne!(installed.agent_root, bundles.agent);
  assert_ne!(installed.octa_root, bundles.octa);
}

#[test]
fn selects_protocol_versions_supported_by_both_agent_and_runner() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let capabilities_path = bundles.octa.join("octa-runner-capabilities.json");
  let mut capabilities: Value = serde_json::from_slice(&fs::read(&capabilities_path).unwrap()).unwrap();
  capabilities["runner_protocols"] = json!([9, 3]);
  capabilities["event_schemas"] = json!([9, 4]);
  write_json(&capabilities_path, &capabilities);
  write_checksums(&bundles.octa);

  let installed = install(&bundles, &temporary.path().join("compatible-protocols")).unwrap();

  assert_eq!(installed.toolchain.runner_protocol, 3);
  assert_eq!(installed.toolchain.event_schema, 4);
}

#[test]
fn rejects_digest_tampering_before_creating_the_installation() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  fs::write(bundles.agent.join("bin/octacity-agent"), b"tampered").unwrap();
  let destination = temporary.path().join("rejected-installation");

  let error = install(&bundles, &destination).unwrap_err();

  assert!(error.to_string().contains("checksum mismatch"));
  assert!(!destination.exists());
}

#[test]
fn rejects_manifest_protocol_drift_even_with_fresh_checksums() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let manifest_path = bundles.agent.join("release-manifest.json");
  let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
  manifest["protocols"]["coordinator"]["min"] = json!(2);
  manifest["protocols"]["coordinator"]["max"] = json!(2);
  write_json(&manifest_path, &manifest);
  write_checksums(&bundles.agent);

  let error = install(&bundles, &temporary.path().join("rejected-protocol")).unwrap_err();

  assert!(error.to_string().contains("protocol ranges differ"));
}

#[test]
fn rejects_octa_plugin_digest_drift_even_with_fresh_bundle_checksums() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  fs::write(bundles.octa.join("plugins/octa_plugin_shell"), b"replaced plugin").unwrap();
  write_checksums(&bundles.octa);

  let error = install(&bundles, &temporary.path().join("rejected-plugin")).unwrap_err();

  assert!(error.to_string().contains("differs from Octa.lock"));
}

#[test]
fn rejects_source_plugin_identity_drift_even_with_fresh_checksums() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let manifest = bundles.agent.join("source-plugins/git/plugin.toml");
  let contents = fs::read_to_string(&manifest)
    .unwrap()
    .replace("version = \"0.1.0\"", "version = \"9.9.9\"");
  fs::write(&manifest, contents).unwrap();
  write_checksums(&bundles.agent);

  let error = install(&bundles, &temporary.path().join("rejected-source-version")).unwrap_err();

  assert!(error.to_string().contains("source-plugin version differs"));
}

#[test]
fn rejects_octa_platform_drift_even_when_its_plugins_match() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let capabilities_path = bundles.octa.join("octa-runner-capabilities.json");
  let mut capabilities: Value = serde_json::from_slice(&fs::read(&capabilities_path).unwrap()).unwrap();
  capabilities["platform"] = json!("windows-x86_64");
  write_json(&capabilities_path, &capabilities);
  let lock_path = bundles.octa.join("Octa.lock");
  let lock = fs::read_to_string(&lock_path)
    .unwrap()
    .replace("platforms: [linux-x86_64]", "platforms: [windows-x86_64]");
  fs::write(lock_path, lock).unwrap();
  write_checksums(&bundles.octa);

  let error = install(&bundles, &temporary.path().join("rejected-octa-platform")).unwrap_err();

  assert!(error.to_string().contains("target different runtime platforms"));
}

#[test]
fn rejects_octa_without_exact_source_revision() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let capabilities_path = bundles.octa.join("octa-runner-capabilities.json");
  let mut capabilities: Value = serde_json::from_slice(&fs::read(&capabilities_path).unwrap()).unwrap();
  capabilities.as_object_mut().unwrap().remove("build_commit");
  write_json(&capabilities_path, &capabilities);
  write_checksums(&bundles.octa);

  let error = install(&bundles, &temporary.path().join("rejected-octa-revision")).unwrap_err();

  assert!(error.to_string().contains("do not identify their source revision"));
}

#[test]
fn rejects_a_non_capabilities_octa_message() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let capabilities_path = bundles.octa.join("octa-runner-capabilities.json");
  let mut capabilities: Value = serde_json::from_slice(&fs::read(&capabilities_path).unwrap()).unwrap();
  capabilities["type"] = json!("hello");
  write_json(&capabilities_path, &capabilities);
  write_checksums(&bundles.octa);

  let error = install(&bundles, &temporary.path().join("rejected-octa-message")).unwrap_err();

  assert!(error.to_string().contains("invalid message type"));
}

fn fixture(root: &Path) -> ReleaseBundles {
  let contract: Value = serde_json::from_str(include_str!("../../../../packaging/release-contract.json")).unwrap();
  let server = root.join("server-bundle");
  let agent = root.join("agent-bundle");
  let octa = root.join("octa-bundle");
  for directory in [&server, &agent, &octa] {
    fs::create_dir(directory).unwrap();
  }
  write_executable(
    &server.join("bin/octacity-server"),
    "#!/bin/sh\necho 'octacity-server 0.1.0'\n",
  );
  write_json(
    &server.join("release-manifest.json"),
    &json!({
      "format_version": 1,
      "product": "octacity-server",
      "version": "0.1.0",
      "platform": "linux-amd64",
      "build_inputs": {"octacity_revision": REVISION},
      "protocols": contract["products"]["octacity-server"]["protocols"],
      "components": {"server": {
        "path": "bin/octacity-server",
        "sha256": sha256(&server.join("bin/octacity-server"))
      }}
    }),
  );
  fs::write(
    server.join("release-contract.json"),
    include_bytes!("../../../../packaging/release-contract.json"),
  )
  .unwrap();
  write_checksums(&server);

  write_executable(
    &agent.join("bin/octacity-agent"),
    "#!/bin/sh\necho 'octacity-agent 0.1.0'\n",
  );
  let source = agent.join("source-plugins/git/octacity-source-git");
  write_executable(&source, "#!/bin/sh\nexit 0\n");
  fs::write(
    agent.join("source-plugins/git/plugin.toml"),
    format!(
      "manifest_version = 1\nname = \"git\"\nversion = \"0.1.0\"\nprotocol_min = 1\nprotocol_max = 1\nexecutable = \"octacity-source-git\"\nsha256 = \"{}\"\nplatforms = [\"linux-x86_64\"]\n",
      sha256(&source)
    ),
  )
  .unwrap();
  write_json(
    &agent.join("release-manifest.json"),
    &json!({
      "format_version": 1,
      "product": "octacity-agent",
      "version": "0.1.0",
      "platform": "linux-amd64",
      "build_inputs": {"octacity_revision": REVISION, "octa_revision": REVISION},
      "protocols": contract["products"]["octacity-agent"]["protocols"],
      "components": {
        "agent": {"path": "bin/octacity-agent", "sha256": sha256(&agent.join("bin/octacity-agent"))},
        "source_git": {"path": "source-plugins/git/octacity-source-git", "sha256": sha256(&source)}
      }
    }),
  );
  fs::write(
    agent.join("release-contract.json"),
    include_bytes!("../../../../packaging/release-contract.json"),
  )
  .unwrap();
  write_checksums(&agent);

  write_executable(&octa.join("octa"), "#!/bin/sh\nexit 0\n");
  write_executable(&octa.join("octa-runner"), "#!/bin/sh\nexit 0\n");
  let octa_plugin = octa.join("plugins/octa_plugin_shell");
  write_executable(&octa_plugin, "#!/bin/sh\nexit 0\n");
  fs::write(
    octa.join("plugins/shell.plugin.yml"),
    "manifest_version: 1\nname: shell\n",
  )
  .unwrap();
  fs::write(
    octa.join("Octa.lock"),
    format!(
      "version: 1\nplugins:\n  shell:\n    version: 0.4.0\n    protocol: 2\n    platforms: [linux-x86_64]\n    entrypoint: octa_plugin_shell\n    sha256: {}\n    capabilities: [shell]\n    source: shell.plugin.yml\n",
      sha256(&octa_plugin)
    ),
  )
  .unwrap();
  write_json(
    &octa.join("octa-runner-capabilities.json"),
    &json!({
      "type": "capabilities",
      "octa_version": "0.4.0",
      "runner_protocols": [3],
      "event_schemas": [4],
      "plugin_protocols": [2],
      "octafile_versions": [1, 3],
      "platform": "linux-x86_64",
      "features": ["task_result_cache_v1"],
      "build_commit": REVISION
    }),
  );
  write_json(
    &octa.join("octa-release-contract.json"),
    &json!({
      "format_version": 1,
      "runner_capabilities": "octa-runner-capabilities.json",
      "checksums": "SHA256SUMS",
      "provenance": "github-build-provenance"
    }),
  );
  write_checksums(&octa);
  ReleaseBundles { server, agent, octa }
}

fn write_executable(path: &Path, contents: &str) {
  fs::create_dir_all(path.parent().unwrap()).unwrap();
  fs::write(path, contents).unwrap();
  fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn write_json(path: &Path, value: &Value) {
  fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn write_checksums(root: &Path) {
  let mut files = BTreeMap::new();
  collect(root, root, &mut files);
  let contents = files
    .into_iter()
    .filter(|(path, _)| path != Path::new("SHA256SUMS"))
    .map(|(path, digest)| format!("{digest}  {}\n", path.display()))
    .collect::<String>();
  fs::write(root.join("SHA256SUMS"), contents).unwrap();
}

fn collect(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, String>) {
  for entry in fs::read_dir(directory).unwrap() {
    let path = entry.unwrap().path();
    if path.is_dir() {
      collect(root, &path, files);
    } else {
      files.insert(path.strip_prefix(root).unwrap().to_owned(), sha256(&path));
    }
  }
}

fn sha256(path: &Path) -> String {
  format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
