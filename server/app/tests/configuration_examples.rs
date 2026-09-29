use std::collections::BTreeSet;

use octacity_server::ServerConfig;

const REVERSE_PROXY: &str = include_str!("../../../docs/server.reverse-proxy.example.toml");
const TRUSTED_NETWORK: &str = include_str!("../../../docs/server.trusted-network.example.toml");

#[test]
fn documented_server_configurations_satisfy_the_production_topology_contract() {
  let reverse_proxy = ServerConfig::parse_toml(REVERSE_PROXY).expect("reverse-proxy example must remain valid");
  assert!(reverse_proxy.management_bind().ip().is_loopback());
  assert!(!reverse_proxy.unauthenticated_management_acknowledged());
  assert_separate_ingress(&reverse_proxy);
  assert_public_origins_are_tls(&reverse_proxy);

  let trusted_network = ServerConfig::parse_toml(TRUSTED_NETWORK).expect("trusted-network example must remain valid");
  assert!(trusted_network.management_externally_reachable());
  assert!(trusted_network.unauthenticated_management_acknowledged());
  assert_separate_ingress(&trusted_network);
  assert_public_origins_are_tls(&trusted_network);
}

#[test]
fn documented_public_webhook_origin_cannot_be_downgraded_to_cleartext() {
  let downgraded = REVERSE_PROXY.replace("https://hooks.example.test", "http://hooks.example.test");
  let error = ServerConfig::parse_toml(&downgraded).expect_err("public webhook HTTP must be rejected");
  assert!(error.to_string().contains("HTTPS origin"));
}

fn assert_separate_ingress(config: &ServerConfig) {
  let listeners = [
    config.management_bind(),
    config.agent_bind().expect("Agent listener must be documented"),
    config.cache_bind().expect("cache listener must be documented"),
    config.webhook_bind().expect("webhook listener must be documented"),
  ];
  assert_eq!(listeners.into_iter().collect::<BTreeSet<_>>().len(), listeners.len());
}

fn assert_public_origins_are_tls(config: &ServerConfig) {
  let webhook = config
    .webhook_callback_origin()
    .expect("webhook origin must be documented");
  assert!(webhook.as_str().starts_with("https://"));
}
