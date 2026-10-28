//! Public startup boundary: optional private profiles stay out of ordinary CI.

use std::{fs, os::unix::fs::PermissionsExt as _, path::PathBuf, process::Command};

use octacity_protocol::{FactoryOutputPermissionsV3, FactoryPermissionSetV3, FactoryResourceLimitsV3};

struct Installation {
  temporary: tempfile::TempDir,
  config: PathBuf,
  profile: PathBuf,
  document: toml::Value,
}

impl Installation {
  fn new() -> Self {
    let temporary = tempfile::tempdir().unwrap();
    fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let root = temporary.path();
    let mut document: toml::Value =
      toml::from_str(include_str!("../../../../docs/reference/examples/agent.example.toml")).unwrap();
    for (key, name) in [
      ("work_root", "work"),
      ("state_root", "state"),
      ("octa_release_root", "octa"),
      ("source_plugins_dir", "sources"),
    ] {
      let path = root.join(name);
      fs::create_dir(&path).unwrap();
      document[key] = toml::Value::String(path.to_str().unwrap().to_owned());
    }
    let cache = root.join("cache");
    fs::create_dir(&cache).unwrap();
    fs::set_permissions(&cache, fs::Permissions::from_mode(0o700)).unwrap();
    document["cache"]["root"] = toml::Value::String(cache.to_str().unwrap().to_owned());
    document["workload_identity_profiles"] = toml::Value::Table(toml::map::Map::new());
    document["enabled_runtime_modes"] = toml::Value::Array(vec![]);
    document["allow_native_execution"] = toml::Value::Boolean(false);
    document["native_linux_pids_limit"] = toml::Value::Integer(0);
    for key in [
      "native_linux_cgroup_root",
      "native_linux_bubblewrap_executable",
      "native_environment",
    ] {
      document.as_table_mut().unwrap().remove(key);
    }
    let credential = root.join("enrollment");
    fs::write(&credential, b"fixture-token").unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    document["credential_file"] = toml::Value::String(credential.to_str().unwrap().to_owned());
    document.as_table_mut().unwrap().insert(
      "server_signing_keys".to_owned(),
      toml::Value::Table(toml::map::Map::from_iter([(
        "primary".to_owned(),
        toml::Value::String("11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=".to_owned()),
      )])),
    );
    let release = root.join("octa");
    fs::create_dir(release.join("plugins")).unwrap();
    fs::write(release.join("Octa.lock"), "version: 1\nplugins: {}\n").unwrap();
    let runner = release.join("octa-runner");
    fs::write(&runner, b"inert installation fixture").unwrap();
    fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(release.join("octa-runner-capabilities.json"), format!(
      r#"{{"type":"capabilities","octa_version":"0.3.0","runner_protocols":[3],"event_schemas":[4],"plugin_protocols":[2],"octafile_versions":[1],"platform":"linux-aarch64","features":["{}","{}"]}}"#,
      octacity_protocol::CACHE_FEATURE_V1, octacity_protocol::CACHE_HTTP_FEATURE_V1,
    )).unwrap();
    let profile = root.join("private-values.json");
    document.as_table_mut().unwrap().insert(
      "factory_credential_profiles".to_owned(),
      toml::Value::Table(toml::map::Map::from_iter([(
        "operator-model".to_owned(),
        toml::Value::String(profile.to_str().unwrap().to_owned()),
      )])),
    );
    Self {
      config: root.join("agent.toml"),
      profile,
      document,
      temporary,
    }
  }

  fn enable_factory(&mut self) {
    let permissions = FactoryPermissionSetV3 {
      plugins: vec![],
      executables: vec![],
      tools: vec![],
      commands: vec![],
      max_descendants: 0,
      mounts: vec![],
      network_hosts: vec![],
      secret_profiles: vec!["operator-model".to_owned()],
      workload_identity_profiles: vec![],
      resources: FactoryResourceLimitsV3 {
        cpu_millis: 1,
        memory_bytes: 1,
        disk_bytes: 1,
        process_count: 1,
        elapsed_millis: 1,
      },
      outputs: FactoryOutputPermissionsV3 {
        kinds: vec![],
        max_artifact_count: 0,
        max_artifact_bytes: 0,
        max_report_count: 0,
        max_report_bytes: 0,
      },
    };
    permissions.validate().unwrap();
    self.document.as_table_mut().unwrap().insert(
      "factory_permissions".to_owned(),
      toml::Value::try_from(permissions).unwrap(),
    );
  }

  fn validate(&self) -> std::process::Output {
    assert!(self.temporary.path().exists());
    fs::write(&self.config, toml::to_string(&self.document).unwrap()).unwrap();
    Command::new(env!("CARGO_BIN_EXE_octacity-agent"))
      .arg("validate")
      .arg(&self.config)
      .output()
      .unwrap()
  }
}

#[test]
fn disabled_factory_does_not_load_a_missing_private_profile() {
  let installation = Installation::new();
  let output = installation.validate();
  assert!(
    output.status.success(),
    "status: {}; stdout: {}; stderr: {}",
    output.status,
    String::from_utf8_lossy(&output.stdout),
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(!installation.profile.exists());
}

#[test]
fn validates_an_explicit_standalone_mcp_program_without_a_release_plugin_contract() {
  let mut installation = Installation::new();
  let executable = installation.temporary.path().join("mcp-peer");
  let mut bytes = vec![0_u8; 64];
  bytes[..6].copy_from_slice(b"\x7fELF\x02\x01");
  bytes[18..20].copy_from_slice(&183_u16.to_le_bytes());
  bytes.extend_from_slice(b"finite standalone MCP peer");
  fs::write(&executable, bytes).unwrap();
  fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
  let tools: toml::Value = toml::from_str(&format!(
    "[issue-reader]\npath = {:?}\nversion = \"1.2.0\"\nplatform = \"linux-aarch64\"\nsha256 = \"cffdc6641e7796a7adb21fcc054b904d2f6252bd467cec50dfaba23da62dec3f\"\nstandalone = true\n",
    executable.to_str().unwrap()
  )).unwrap();
  installation
    .document
    .as_table_mut()
    .unwrap()
    .insert("tool_executables".into(), tools);
  let output = installation.validate();
  let diagnostic = format!(
    "{}{}",
    String::from_utf8_lossy(&output.stdout),
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(output.status.success(), "{diagnostic}");
  assert!(diagnostic.contains("1 tool executable(s)"), "{diagnostic}");
}

#[test]
fn disabled_factory_does_not_require_an_optional_private_profile_directory() {
  let mut installation = Installation::new();
  let missing_parent = installation.temporary.path().join("not-provisioned-private-values");
  installation.document["factory_credential_profiles"]["operator-model"] =
    toml::Value::String(missing_parent.join("values.json").to_str().unwrap().to_owned());
  let output = installation.validate();
  assert!(
    output.status.success(),
    "{}{}",
    String::from_utf8_lossy(&output.stdout),
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(!missing_parent.exists());
}

#[test]
fn enabled_factory_refuses_an_invalid_private_profile_without_echoing_contents() {
  let mut installation = Installation::new();
  installation.enable_factory();
  let canary = "private-model-credential-canary";
  fs::write(&installation.profile, format!("{{\"TOKEN\":\"{canary}\",\"broken\":}}")).unwrap();
  fs::set_permissions(&installation.profile, fs::Permissions::from_mode(0o600)).unwrap();
  let output = installation.validate();
  let diagnostic = format!(
    "{}{}",
    String::from_utf8_lossy(&output.stdout),
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(
    !output.status.success(),
    "invalid private installation must prevent enabled startup"
  );
  assert!(!diagnostic.contains(canary));
  assert!(
    diagnostic.contains("Factory credential installation is invalid"),
    "{diagnostic}"
  );
}

#[test]
fn enabled_factory_accepts_private_named_values_without_logging_them() {
  let mut installation = Installation::new();
  installation.enable_factory();
  let canary = "valid-private-provider-canary";
  fs::write(&installation.profile, format!("{{\"PROVIDER_TOKEN\":\"{canary}\"}}")).unwrap();
  fs::set_permissions(&installation.profile, fs::Permissions::from_mode(0o600)).unwrap();
  let output = installation.validate();
  let diagnostic = format!(
    "{}{}",
    String::from_utf8_lossy(&output.stdout),
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(output.status.success(), "{diagnostic}");
  assert!(!diagnostic.contains(canary));
}

#[test]
fn enabled_factory_refuses_ambiguous_duplicate_private_names() {
  let mut installation = Installation::new();
  installation.enable_factory();
  fs::write(
    &installation.profile,
    br#"{"TOKEN":"first-private-canary","TOKEN":"second-private-canary"}"#,
  )
  .unwrap();
  fs::set_permissions(&installation.profile, fs::Permissions::from_mode(0o600)).unwrap();
  let output = installation.validate();
  let diagnostic = format!(
    "{}{}",
    String::from_utf8_lossy(&output.stdout),
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(
    !output.status.success(),
    "ambiguous operator values must prevent startup"
  );
  assert!(
    diagnostic.contains("Factory credential installation is invalid"),
    "{diagnostic}"
  );
  assert!(!diagnostic.contains("private-canary"));
}

#[test]
fn enabled_factory_refuses_a_missing_private_directory_without_disclosing_its_path() {
  let mut installation = Installation::new();
  installation.enable_factory();
  let canary = "private-credential-location-canary";
  let parent = installation.temporary.path().join(canary);
  installation.document["factory_credential_profiles"]["operator-model"] =
    toml::Value::String(parent.join("values.json").to_str().unwrap().to_owned());
  let output = installation.validate();
  let diagnostic = format!(
    "{}{}",
    String::from_utf8_lossy(&output.stdout),
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(
    !output.status.success(),
    "an enabled private installation must be provisioned"
  );
  assert!(diagnostic.contains("Factory credential profile"));
  assert!(
    !diagnostic.contains(canary),
    "private file references must not enter diagnostics"
  );
  assert!(!diagnostic.contains(parent.to_str().unwrap()));
}
