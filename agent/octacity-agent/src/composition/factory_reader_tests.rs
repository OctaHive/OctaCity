#[cfg(all(unix, any(target_os = "macos", target_os = "linux")))]
#[tokio::test]
async fn controlled_reader_components_pin_both_binaries_before_registration() {
  use std::os::unix::fs::PermissionsExt as _;
  let _guard = component_graph_guard().await;
  let fixture = installed_agent_fixture("https://coordinator.example");
  let installation = tempfile::tempdir().unwrap();
  let root = installation.path().join("reader");
  octacity_private_fs::create_private_directory(&root).unwrap();
  for name in ["worker", "bridge"] {
    let mut bytes = vec![0u8; 64];
    bytes[..6].copy_from_slice(b"\x7fELF\x02\x01");
    bytes[18..20].copy_from_slice(&183u16.to_le_bytes());
    bytes.extend_from_slice(name.as_bytes());
    fs::write(root.join(name), bytes).unwrap();
    fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o500)).unwrap();
  }
  let tool = serde_json::json!({"identity":"bounded-source-read","version":"1","sha256":"a".repeat(64)});
  let executable = serde_json::json!({"identity":"installed-reader","version":"1","sha256":"3641fb4e36b1ae74979c0aab7af6ad258d31b88ef4251955e031052c09884fde"});
  let resources = serde_json::json!({"cpu_millis":1000,"memory_bytes":268435456,"disk_bytes":16777216,"process_count":8,"elapsed_millis":5000});
  let outputs = serde_json::json!({"kinds":[],"max_artifact_count":0,"max_artifact_bytes":0,"max_report_count":0,"max_report_bytes":0});
  let mut document: toml::Value = toml::from_str(&fs::read_to_string(&fixture.config).unwrap()).unwrap();
  let state = tempfile::Builder::new().prefix("ar-").tempdir_in("/tmp").unwrap();
  document["state_root"] = toml::Value::String(state.path().canonicalize().unwrap().to_str().unwrap().to_owned());
  document.as_table_mut().unwrap().insert("factory_permissions".to_owned(),toml::Value::try_from(serde_json::json!({
      "plugins":[],"executables":[executable.clone()],"tools":[tool.clone()],
      "commands":[{"executable":executable.clone(),"arguments":[{"kind":"any","max_bytes":256}]}],
      "max_descendants":7,"mounts":[{"root":"/workspace/source","mode":"read_only"}],
      "network_hosts":[],"secret_profiles":[],"workload_identity_profiles":[],"resources":resources.clone(),"outputs":outputs.clone()
    })).unwrap());
  document.as_table_mut().unwrap().insert("protected_reader".to_owned(),toml::Value::try_from(serde_json::json!({
      "release_root":root,"entrypoint":"worker","platform":"linux-aarch64","tool":tool,"executable":executable,"resources":resources,"outputs":outputs,
      "bridge":{"entrypoint":"bridge","sha256":"da0f0809071e46681aef869a5a5e0eadc6727c1d47eb974b5c793b8808a20e37","hook_tool_name":"mcp__installed_source__read_file"}
    })).unwrap());
  fs::write(&fixture.config, toml::to_string(&document).unwrap()).unwrap();
  let components = Components::load(&fixture.config).await.unwrap();
  assert!(components.factory_tools.is_some());
  assert_eq!(components.inventory.factory_executions.len(), 1);
  assert_eq!(
    components.inventory.factory_executions[0].controlled_tools,
    components.runner.capabilities.platform == "linux-aarch64",
    "only the exact compatible guest route may advertise the installed reader"
  );
  assert_eq!(
    components.tool_executables["installed-reader"].sha256,
    "3641fb4e36b1ae74979c0aab7af6ad258d31b88ef4251955e031052c09884fde"
  );
  drop(components);

  let bridge = document["protected_reader"]
    .as_table_mut()
    .unwrap()
    .remove("bridge")
    .unwrap();
  fs::write(&fixture.config, toml::to_string(&document).unwrap()).unwrap();
  let metadata_only = Components::load(&fixture.config).await.unwrap();
  assert!(metadata_only.factory_tools.is_none());
  assert_eq!(metadata_only.inventory.factory_executions.len(), 1);
  assert!(!metadata_only.inventory.factory_executions[0].controlled_tools);
  drop(metadata_only);
  document["protected_reader"]
    .as_table_mut()
    .unwrap()
    .insert("bridge".to_owned(), bridge);

  let qualified_state = document["state_root"].clone();
  let long_state = fixture._temporary.path().join("s".repeat(120));
  octacity_private_fs::create_private_directory(&long_state).unwrap();
  document["state_root"] = toml::Value::String(long_state.to_str().unwrap().to_owned());
  fs::write(&fixture.config, toml::to_string(&document).unwrap()).unwrap();
  let failure = Components::load(&fixture.config)
    .await
    .err()
    .expect("an unusable control address must fail before registration");
  assert!(!failure.to_string().contains(long_state.to_str().unwrap()));
  document["state_root"] = qualified_state;
  fs::write(&fixture.config, toml::to_string(&document).unwrap()).unwrap();
  fs::set_permissions(root.join("bridge"), fs::Permissions::from_mode(0o700)).unwrap();
  fs::write(root.join("bridge"), b"substituted reader bridge").unwrap();
  fs::set_permissions(root.join("bridge"), fs::Permissions::from_mode(0o500)).unwrap();
  let failure = Components::load(&fixture.config)
    .await
    .err()
    .expect("substitution must fail before registration");
  assert!(!failure.to_string().contains("substituted"));
  assert!(!failure.to_string().contains(root.to_str().unwrap()));
}
