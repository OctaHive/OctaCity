use std::{
  collections::BTreeMap,
  fs::{self, File},
  io::Read as _,
  path::{Component, Path, PathBuf},
};

use octa_plugin_lock::PluginLock;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::{ReleaseBackend, host_architecture, string};

pub(super) struct ReleaseInstallation {
  pub(super) agent_binary: PathBuf,
  pub(super) source_plugins: PathBuf,
  pub(super) octa_root: PathBuf,
  pub(super) agent_manifest: Value,
  pub(super) toolchain: Toolchain,
}

pub(super) struct Toolchain {
  pub(super) source_version: String,
  pub(super) source_digest: String,
  pub(super) octa_version: String,
  pub(super) runner_digest: String,
  pub(super) runner_protocol: u16,
  pub(super) event_schema: u16,
  pub(super) plugin_protocol: u16,
  pub(super) plugin_digests: BTreeMap<String, String>,
  pub(super) capabilities: Value,
}

impl ReleaseInstallation {
  pub(super) fn load(agent_root: &Path, octa_root: &Path, backend: &ReleaseBackend) -> Self {
    verify_checksums(agent_root);
    verify_checksums(octa_root);

    let agent_manifest: Value = read_json(&agent_root.join("release-manifest.json"));
    assert_eq!(agent_manifest["product"], "octacity-agent");
    let expected_platform = match backend {
      ReleaseBackend::Native { .. } => format!("linux-{}", host_architecture()),
      ReleaseBackend::Microsandbox { .. } => "macos-arm64".to_owned(),
    };
    assert_eq!(agent_manifest["platform"], expected_platform);
    let agent_binary = checked_component(agent_root, &agent_manifest, "agent");
    let source_binary = checked_component(agent_root, &agent_manifest, "source_git");
    let version = string(&agent_manifest, "version");
    let output = std::process::Command::new(&agent_binary)
      .arg("--version")
      .output()
      .unwrap();
    assert!(output.status.success());
    assert_eq!(
      String::from_utf8(output.stdout).unwrap().trim(),
      format!("octacity-agent {version}")
    );
    let source_digest = sha256(&source_binary);
    let source_plugins = agent_root.join("source-plugins");
    let plugin_manifest: toml::Value =
      toml::from_str(&fs::read_to_string(source_plugins.join("git/plugin.toml")).unwrap()).unwrap();
    assert_eq!(plugin_manifest["settings"]["allow_file"].as_bool(), Some(false));
    assert!(plugin_manifest["settings"]["git_path"].as_str().is_some());

    let contract: Value = read_json(&octa_root.join("octa-release-contract.json"));
    assert_eq!(contract["format_version"], 1);
    assert_eq!(contract["runner_capabilities"], "octa-runner-capabilities.json");
    assert_eq!(contract["checksums"], "SHA256SUMS");
    let capabilities: Value = read_json(&octa_root.join("octa-runner-capabilities.json"));
    let expected_runner_platform = format!(
      "linux-{}",
      match backend.guest_architecture() {
        "amd64" => "x86_64",
        "arm64" => "aarch64",
        other => other,
      }
    );
    assert_eq!(capabilities["platform"], expected_runner_platform);
    let runner_digest = sha256(&octa_root.join("octa-runner"));
    let lock: PluginLock = serde_yaml_ng::from_str(&fs::read_to_string(octa_root.join("Octa.lock")).unwrap()).unwrap();
    assert!(!lock.plugins.is_empty());
    let plugin_protocol = lock.plugins.values().next().unwrap().protocol;
    assert!(lock.plugins.values().all(|plugin| plugin.protocol == plugin_protocol));
    let plugin_digests = lock
      .plugins
      .iter()
      .map(|(name, plugin)| (name.clone(), plugin.sha256.clone()))
      .collect();

    Self {
      agent_binary,
      source_plugins,
      octa_root: octa_root.to_owned(),
      agent_manifest,
      toolchain: Toolchain {
        source_version: version,
        source_digest,
        octa_version: string(&capabilities, "octa_version"),
        runner_digest,
        runner_protocol: first_u16(&capabilities, "runner_protocols"),
        event_schema: first_u16(&capabilities, "event_schemas"),
        plugin_protocol,
        plugin_digests,
        capabilities,
      },
    }
  }
}

fn verify_checksums(root: &Path) {
  let contents = fs::read_to_string(root.join("SHA256SUMS")).unwrap();
  assert!(!contents.trim().is_empty(), "release checksum inventory is empty");
  for line in contents.lines() {
    let (expected, relative) = line.split_once("  ").expect("checksum lines must contain two spaces");
    assert_eq!(expected.len(), 64);
    let path = safe_join(root, relative);
    assert_eq!(sha256(&path), expected, "release checksum mismatch for {relative}");
  }
}

fn checked_component(root: &Path, manifest: &Value, name: &str) -> PathBuf {
  let component = &manifest["components"][name];
  let path = safe_join(root, component["path"].as_str().expect("component path must be text"));
  assert_eq!(sha256(&path), component["sha256"].as_str().unwrap());
  path
}

fn safe_join(root: &Path, relative: &str) -> PathBuf {
  let relative = Path::new(relative);
  assert!(!relative.is_absolute());
  assert!(
    relative
      .components()
      .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
  );
  let path = root.join(relative);
  assert!(
    path.is_file() && !path.is_symlink(),
    "release entry must be a regular non-symlink file: {path:?}"
  );
  path
}

fn sha256(path: &Path) -> String {
  let mut source = File::open(path).unwrap();
  let mut digest = Sha256::new();
  let mut buffer = [0_u8; 64 * 1024];
  loop {
    let count = source.read(&mut buffer).unwrap();
    if count == 0 {
      break;
    }
    digest.update(&buffer[..count]);
  }
  format!("{:x}", digest.finalize())
}

fn first_u16(value: &Value, field: &str) -> u16 {
  u16::try_from(value[field].as_array().unwrap().first().unwrap().as_u64().unwrap()).unwrap()
}

fn read_json(path: &Path) -> Value {
  serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
