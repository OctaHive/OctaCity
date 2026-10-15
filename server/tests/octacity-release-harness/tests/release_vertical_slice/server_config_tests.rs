use super::*;

#[test]
fn factory_server_configuration_keeps_private_installation_at_the_composition_boundary() {
  let directory = tempfile::tempdir().unwrap();
  let profiles = directory.path().join("private profiles.json");
  let connectors = directory.path().join("private connectors.json");
  fs::write(&profiles, "private-profile-body-canary").unwrap();
  fs::write(&connectors, "private-connector-body-canary").unwrap();
  let toolchain = Toolchain {
    source_version: "0.1.0".into(),
    source_digest: "11".repeat(32),
    octa_version: "0.5.1".into(),
    runner_digest: "22".repeat(32),
    runner_protocol: 1,
    event_schema: 1,
    plugin_protocol: 1,
    plugin_digests: std::collections::BTreeMap::from([("shell".into(), "33".repeat(32))]),
  };
  let path = write_factory_server_config(
    directory.path(),
    &ServerConfigInput {
      postgres_url: "postgres://fixture:private-database-canary@127.0.0.1/test",
      object_endpoint: "http://127.0.0.1:9000",
      cache_endpoint: "https://cache.example",
      toolchain: &toolchain,
      management_addr: "127.0.0.1:18080".parse().unwrap(),
      agent_addr: "127.0.0.1:18081".parse().unwrap(),
      cache_addr: None,
    },
    &FactoryServerFiles {
      profiles: &profiles,
      connectors: &connectors,
    },
  );
  let contents = fs::read_to_string(path).unwrap();
  let config: toml::Value = toml::from_str(&contents).unwrap();
  assert_eq!(config["factory_profiles_file"].as_str(), profiles.to_str());
  assert_eq!(config["factory_signal_connectors_file"].as_str(), connectors.to_str());
  assert!(config["factory"]["poll_interval_milliseconds"].as_integer().unwrap() > 0);
  assert_eq!(config["management_bind"].as_str(), Some("127.0.0.1:18080"));
  assert_eq!(config["agent_bind"].as_str(), Some("127.0.0.1:18081"));
  let policy: Value =
    serde_json::from_slice(&fs::read(config["job_spec"]["policy_file"].as_str().unwrap()).unwrap()).unwrap();
  assert_eq!(policy["octa"]["runner_sha256"], "22".repeat(32));
  for secret in [
    "private-profile-body-canary",
    "private-connector-body-canary",
    "private-database-canary",
  ] {
    assert!(!contents.contains(secret));
  }
}
