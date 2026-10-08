#![cfg(unix)]

use std::{
  collections::BTreeMap,
  fs,
  os::unix::fs::PermissionsExt as _,
  path::{Path, PathBuf},
};

use octacity_release_harness::{
  AgentRuntimeBundles, AgentRuntimePlatforms, BrowserReleaseBundles, ReleaseBundles, derive_job_spec_policy, install,
  install_agent_runtime, install_browser_release, load_job_spec_policy, verify_job_spec_policy,
};
use octacity_server_job::{JobSpecToolchainPolicy, MAX_JOB_SPEC_TOOLCHAIN_POLICY_BYTES};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

fn runtime_platforms() -> AgentRuntimePlatforms {
  AgentRuntimePlatforms::new("linux-amd64", "linux-x86_64")
}

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
  assert_eq!(installed.octa_version, "0.5.0");
  assert_ne!(installed.server_root, bundles.server);
  assert_ne!(installed.agent_root, bundles.agent);
  assert_ne!(installed.octa_root, bundles.octa);
}

#[test]
fn installs_only_a_matching_checksummed_browser_release() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = browser_fixture(temporary.path());
  let destination = temporary.path().join("browser-installation");

  let installed = install_browser_release(&bundles, &destination).unwrap();

  assert_eq!(installed.root, destination);
  assert!(installed.server_binary.starts_with(&installed.server_root));
  assert_eq!(installed.console_entrypoint, installed.console_root.join("index.html"));
  assert_eq!(installed.console_nginx_root, installed.console_root.join("share/nginx"));
  assert_eq!(
    installed.server_manifest.version(),
    installed.console_manifest.version()
  );
  assert_ne!(installed.server_root, bundles.server);
  assert_ne!(installed.console_root, bundles.console);
}

#[test]
fn rejects_a_console_built_from_another_revision() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = browser_fixture(temporary.path());
  let manifest_path = bundles.console.join("release-manifest.json");
  let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
  manifest["build_inputs"]["octacity_revision"] = json!("f".repeat(40));
  write_json(&manifest_path, &manifest);
  write_checksums(&bundles.console);

  let error = install_browser_release(&bundles, &temporary.path().join("rejected-browser")).unwrap_err();

  assert!(error.to_string().contains("built from different OctaCity revisions"));
}

#[test]
fn rejects_a_console_without_the_packaged_proxy_contract() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = browser_fixture(temporary.path());
  fs::remove_file(bundles.console.join("share/nginx/routes.conf")).unwrap();
  write_checksums(&bundles.console);

  let error = install_browser_release(&bundles, &temporary.path().join("rejected-browser")).unwrap_err();

  assert!(error.to_string().contains("absent from SHA256SUMS"));
}

#[test]
fn installs_a_native_macos_agent_runtime_for_host_execution() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let agent_manifest_path = bundles.agent.join("release-manifest.json");
  let mut agent_manifest: Value = serde_json::from_slice(&fs::read(&agent_manifest_path).unwrap()).unwrap();
  agent_manifest["platform"] = json!("macos-arm64");
  write_json(&agent_manifest_path, &agent_manifest);
  let source_manifest_path = bundles.agent.join("source-plugins/git/plugin.toml");
  let source_manifest = fs::read_to_string(&source_manifest_path)
    .unwrap()
    .replace("platforms = [\"linux-x86_64\"]", "platforms = [\"macos-aarch64\"]");
  fs::write(source_manifest_path, source_manifest).unwrap();
  write_checksums(&bundles.agent);

  let capabilities_path = bundles.octa.join("octa-runner-capabilities.json");
  let mut capabilities: Value = serde_json::from_slice(&fs::read(&capabilities_path).unwrap()).unwrap();
  capabilities["platform"] = json!("macos-aarch64");
  write_json(&capabilities_path, &capabilities);
  let lock_path = bundles.octa.join("Octa.lock");
  let lock = fs::read_to_string(&lock_path)
    .unwrap()
    .replace("platforms: [linux-x86_64]", "platforms: [macos-aarch64]");
  fs::write(lock_path, lock).unwrap();
  write_checksums(&bundles.octa);

  let installed = install_agent_runtime(
    &AgentRuntimeBundles {
      agent: bundles.agent,
      octa: bundles.octa,
    },
    &temporary.path().join("native-macos-agent-runtime"),
  )
  .unwrap();

  assert_eq!(installed.agent_manifest.platform(), "macos-arm64");
  assert_eq!(installed.octa_capabilities.platform(), "macos-aarch64");
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
fn rejects_codex_plugin_executable_digest_drift_even_with_fresh_bundle_checksums() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  fs::write(bundles.octa.join("plugins/octa_plugin_codex"), b"replaced plugin").unwrap();
  write_checksums(&bundles.octa);

  let error = install(&bundles, &temporary.path().join("rejected-plugin")).unwrap_err();

  assert!(error.to_string().contains("differs from Octa.lock"));
}

#[test]
fn rejects_codex_executable_compatibility_drift_even_with_fresh_checksums() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let compatibility_path = bundles.octa.join("codex-compatibility.json");
  let mut compatibility: Value = serde_json::from_slice(&fs::read(&compatibility_path).unwrap()).unwrap();
  compatibility["executable"]["supported_versions"] = json!(["0.131.0"]);
  write_json(&compatibility_path, &compatibility);
  write_checksums(&bundles.octa);

  let error = install(&bundles, &temporary.path().join("rejected-codex-version")).unwrap_err();

  assert!(
    error
      .to_string()
      .contains("unsupported Octa Codex compatibility identity")
  );
}

#[test]
fn rejects_codex_plugin_lock_drift_even_with_fresh_checksums() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let lock_path = bundles.octa.join("Octa.lock");
  let lock = fs::read_to_string(&lock_path)
    .unwrap()
    .replace("version: \"0.5.0\"", "version: \"0.5.1\"");
  fs::write(lock_path, lock).unwrap();
  write_checksums(&bundles.octa);

  let error = install(&bundles, &temporary.path().join("rejected-codex-plugin")).unwrap_err();

  assert!(error.to_string().contains("differs from Octa.lock"));
}

#[test]
fn rejects_codex_without_the_blocking_tool_authorization_capability() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let lock_path = bundles.octa.join("Octa.lock");
  let lock = fs::read_to_string(&lock_path)
    .unwrap()
    .replace("    capabilities: [codex.blocking-pre-tool-authorization.v1]\n", "");
  fs::write(lock_path, lock).unwrap();
  write_checksums(&bundles.octa);

  let error = install(&bundles, &temporary.path().join("rejected-codex-capability")).unwrap_err();

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
fn derives_and_verifies_server_policy_from_the_agent_assets() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let runtime = AgentRuntimeBundles {
    agent: bundles.agent,
    octa: bundles.octa,
  };

  let policy = derive_job_spec_policy(&runtime, &runtime_platforms(), 900).unwrap();
  let document = serde_json::to_value(&policy).unwrap();

  assert_eq!(document["source"]["provider"], "git");
  assert_eq!(document["source"]["plugin_version"], "0.1.0");
  assert_eq!(document["octa"]["version"], "0.5.0");
  assert_eq!(document["octa"]["runner_protocol"], 3);
  assert_eq!(document["octa"]["event_schema"], 4);
  assert_eq!(document["octa"]["plugin_protocol"], 2);
  assert_eq!(document["validity"], 900);
  verify_job_spec_policy(&runtime, &runtime_platforms(), &policy, 900).unwrap();
}

#[test]
fn derives_policy_for_a_macos_agent_with_a_linux_arm64_octa_guest() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let agent_manifest_path = bundles.agent.join("release-manifest.json");
  let mut agent_manifest: Value = serde_json::from_slice(&fs::read(&agent_manifest_path).unwrap()).unwrap();
  agent_manifest["platform"] = json!("macos-arm64");
  write_json(&agent_manifest_path, &agent_manifest);
  let source_manifest_path = bundles.agent.join("source-plugins/git/plugin.toml");
  let source_manifest = fs::read_to_string(&source_manifest_path)
    .unwrap()
    .replace("platforms = [\"linux-x86_64\"]", "platforms = [\"macos-aarch64\"]");
  fs::write(source_manifest_path, source_manifest).unwrap();
  write_checksums(&bundles.agent);

  let capabilities_path = bundles.octa.join("octa-runner-capabilities.json");
  let mut capabilities: Value = serde_json::from_slice(&fs::read(&capabilities_path).unwrap()).unwrap();
  capabilities["platform"] = json!("linux-aarch64");
  write_json(&capabilities_path, &capabilities);
  let lock_path = bundles.octa.join("Octa.lock");
  let lock = fs::read_to_string(&lock_path)
    .unwrap()
    .replace("platforms: [linux-x86_64]", "platforms: [linux-aarch64]");
  fs::write(lock_path, lock).unwrap();
  write_checksums(&bundles.octa);
  let runtime = AgentRuntimeBundles {
    agent: bundles.agent,
    octa: bundles.octa,
  };

  let policy = derive_job_spec_policy(
    &runtime,
    &AgentRuntimePlatforms::new("macos-arm64", "linux-aarch64"),
    900,
  )
  .unwrap();

  assert_eq!(serde_json::to_value(policy).unwrap()["octa"]["version"], "0.5.0");
}

#[test]
fn rejects_server_policy_version_protocol_and_digest_drift() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let runtime = AgentRuntimeBundles {
    agent: bundles.agent,
    octa: bundles.octa,
  };
  let policy = derive_job_spec_policy(&runtime, &runtime_platforms(), 900).unwrap();
  let baseline = serde_json::to_value(policy).unwrap();
  let mutations = [
    (
      "source plugin version",
      vec!["source", "plugin_version"],
      json!("9.9.9"),
    ),
    ("Octa version", vec!["octa", "version"], json!("9.9.9")),
    ("runner protocol", vec!["octa", "runner_protocol"], json!(9)),
    ("event schema", vec!["octa", "event_schema"], json!(9)),
    ("plugin protocol", vec!["octa", "plugin_protocol"], json!(9)),
    ("source digest", vec!["source", "plugin_sha256"], json!("f".repeat(64))),
    ("runner digest", vec!["octa", "runner_sha256"], json!("f".repeat(64))),
    (
      "task plugin digest",
      vec!["octa", "plugin_digests", "shell"],
      json!("f".repeat(64)),
    ),
  ];

  for (name, path, replacement) in mutations {
    let mut document = baseline.clone();
    let mut target = &mut document;
    for component in path {
      target = &mut target[component];
    }
    *target = replacement;
    let drifted: JobSpecToolchainPolicy = serde_json::from_value(document).unwrap();
    let error = verify_job_spec_policy(&runtime, &runtime_platforms(), &drifted, 900).expect_err(name);
    assert!(error.to_string().contains("differs from the verified Agent toolchain"));
  }
}

#[test]
fn rejects_policy_verification_after_executor_platform_drift() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let runtime = AgentRuntimeBundles {
    agent: bundles.agent,
    octa: bundles.octa,
  };
  let policy = derive_job_spec_policy(&runtime, &runtime_platforms(), 900).unwrap();
  let capabilities_path = runtime.octa.join("octa-runner-capabilities.json");
  let mut capabilities: Value = serde_json::from_slice(&fs::read(&capabilities_path).unwrap()).unwrap();
  capabilities["platform"] = json!("windows-x86_64");
  write_json(&capabilities_path, &capabilities);
  let lock_path = runtime.octa.join("Octa.lock");
  let lock = fs::read_to_string(&lock_path)
    .unwrap()
    .replace("platforms: [linux-x86_64]", "platforms: [windows-x86_64]");
  fs::write(lock_path, lock).unwrap();
  write_checksums(&runtime.octa);

  let error = verify_job_spec_policy(&runtime, &runtime_platforms(), &policy, 900).unwrap_err();

  assert!(error.to_string().contains("target different runtime platforms"));
}

#[test]
fn rejects_coherent_executor_platform_relabeling_against_the_signer_expectation() {
  let temporary = tempfile::tempdir().unwrap();
  let bundles = fixture(temporary.path());
  let agent_manifest_path = bundles.agent.join("release-manifest.json");
  let mut agent_manifest: Value = serde_json::from_slice(&fs::read(&agent_manifest_path).unwrap()).unwrap();
  agent_manifest["platform"] = json!("windows-amd64");
  write_json(&agent_manifest_path, &agent_manifest);
  let source_manifest_path = bundles.agent.join("source-plugins/git/plugin.toml");
  let source_manifest = fs::read_to_string(&source_manifest_path)
    .unwrap()
    .replace("platforms = [\"linux-x86_64\"]", "platforms = [\"windows-x86_64\"]");
  fs::write(source_manifest_path, source_manifest).unwrap();
  write_checksums(&bundles.agent);

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

  let error = derive_job_spec_policy(
    &AgentRuntimeBundles {
      agent: bundles.agent,
      octa: bundles.octa,
    },
    &runtime_platforms(),
    900,
  )
  .unwrap_err();

  assert!(error.to_string().contains("differ from the signing policy expectation"));
}

#[test]
fn rejects_an_oversized_job_spec_policy_before_deserialization() {
  let temporary = tempfile::tempdir().unwrap();
  let policy = temporary.path().join("job-spec-policy.json");
  fs::write(&policy, vec![b' '; MAX_JOB_SPEC_TOOLCHAIN_POLICY_BYTES as usize + 1]).unwrap();

  let error = load_job_spec_policy(&policy).unwrap_err();

  assert!(error.to_string().contains("exceeds the 1048576-byte limit"));
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
  let codex_plugin = octa.join("plugins/octa_plugin_codex");
  write_executable(&codex_plugin, "#!/bin/sh\nexit 0\n");
  fs::write(
    octa.join("plugins/shell.plugin.yml"),
    "manifest_version: 1\nname: shell\n",
  )
  .unwrap();
  fs::write(
    octa.join("plugins/codex.plugin.yml"),
    "manifest_version: 1\nname: codex\n",
  )
  .unwrap();
  fs::write(
    octa.join("Octa.lock"),
    format!(
      "version: 1\nplugins:\n  codex:\n    version: \"0.5.0\"\n    protocol: 2\n    platforms: [linux-x86_64]\n    entrypoint: octa_plugin_codex\n    sha256: {}\n    capabilities: [codex.blocking-pre-tool-authorization.v1]\n    source: codex.plugin.yml\n  shell:\n    version: \"0.5.0\"\n    protocol: 2\n    platforms: [linux-x86_64]\n    entrypoint: octa_plugin_shell\n    sha256: {}\n    capabilities: [shell]\n    source: shell.plugin.yml\n",
      sha256(&codex_plugin),
      sha256(&octa_plugin)
    ),
  )
  .unwrap();
  write_json(
    &octa.join("codex-compatibility.json"),
    &json!({
      "format_version": 1,
      "plugin": {
        "name": "codex",
        "version": "0.5.0",
        "protocol": 2,
        "manifest": "plugins/codex.plugin.yml"
      },
      "executable": {
        "product": "codex-cli",
        "supported_versions": ["0.161.0"],
        "selection_environment": "OCTA_CODEX_EXECUTABLE"
      }
    }),
  );
  write_json(
    &octa.join("octa-runner-capabilities.json"),
    &json!({
      "type": "capabilities",
      "octa_version": "0.5.0",
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
      "codex_compatibility": "codex-compatibility.json",
      "checksums": "SHA256SUMS",
      "provenance": "github-build-provenance"
    }),
  );
  write_checksums(&octa);
  ReleaseBundles { server, agent, octa }
}

fn browser_fixture(root: &Path) -> BrowserReleaseBundles {
  let release = fixture(root);
  let contract: Value = serde_json::from_str(include_str!("../../../../packaging/release-contract.json")).unwrap();
  let console = root.join("console-bundle");
  fs::create_dir(&console).unwrap();
  fs::write(console.join("index.html"), "<!doctype html><div id=\"root\"></div>\n").unwrap();
  let application = console.join("assets/index-12345678.js");
  fs::create_dir_all(application.parent().unwrap()).unwrap();
  fs::write(&application, "export {};\n").unwrap();
  for name in ["cache-map.conf", "nginx.conf", "routes.conf", "security-headers.conf"] {
    let path = console.join("share/nginx").join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("# {name}\n")).unwrap();
  }
  write_json(
    &console.join("release-manifest.json"),
    &json!({
      "format_version": 1,
      "product": "octacity-console",
      "version": "0.1.0",
      "platform": "any",
      "build_inputs": {"octacity_revision": REVISION},
      "protocols": contract["products"]["octacity-console"]["protocols"],
      "components": {
        "application": {
          "path": "index.html",
          "sha256": sha256(&console.join("index.html"))
        },
        "proxy": {
          "path": "share/nginx/nginx.conf",
          "sha256": sha256(&console.join("share/nginx/nginx.conf"))
        }
      },
      "assets": [{
        "path": "assets/index-12345678.js",
        "sha256": sha256(&application),
        "size": fs::metadata(&application).unwrap().len()
      }]
    }),
  );
  fs::write(
    console.join("release-contract.json"),
    include_bytes!("../../../../packaging/release-contract.json"),
  )
  .unwrap();
  write_checksums(&console);
  BrowserReleaseBundles {
    server: release.server,
    console,
  }
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
  hex::encode(Sha256::digest(fs::read(path).unwrap()))
}
