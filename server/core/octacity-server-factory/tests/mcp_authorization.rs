use octacity_server_factory::{
  FactoryDigest, FactoryKey, FactoryMcpAuthorization, FactoryMcpConfiguration, FactoryMcpCredentialPurpose,
  FactoryMcpServer, ImmutableReference,
};
use octacity_server_secrets::{LogicalSecretReference, SecretProviderId};

fn key(name: &str) -> FactoryKey {
  FactoryKey::new(name).unwrap()
}

#[test]
fn mcp_authorization_pins_its_own_secret_purpose_and_resource_without_values() {
  let configuration = FactoryMcpConfiguration::new(
    key("issue-reader"),
    key("v1"),
    FactoryMcpServer::Http {
      url: "https://mcp.example/tools".into(),
      bearer_token_env_var: Some("ISSUE_READ_TOKEN".into()),
      enabled_tools: Some(vec!["read_issue".into()]),
    },
    None,
  )
  .unwrap();
  let authorization = FactoryMcpAuthorization {
    credential_profile: key("issue-mcp-read"),
    secret: LogicalSecretReference::new(SecretProviderId::new("agent-private").unwrap(), "project/issue-read").unwrap(),
    environment: "ISSUE_READ_TOKEN".into(),
    purpose: FactoryMcpCredentialPurpose::McpRead,
    resource: ImmutableReference::new(
      key("reviewed-repository"),
      key("v1"),
      FactoryDigest::from_bytes([7; 32]),
    ),
  };
  let approved = configuration.clone().with_authorization(authorization.clone()).unwrap();
  approved.validate().unwrap();
  assert_eq!(approved.authorization(), Some(&authorization));
  assert_ne!(configuration.reference(), approved.reference());
  let mut other = authorization;
  other.resource = ImmutableReference::new(key("other-repository"), key("v1"), FactoryDigest::from_bytes([8; 32]));
  assert_ne!(
    approved.reference(),
    configuration
      .clone()
      .with_authorization(other.clone())
      .unwrap()
      .reference()
  );
  other.environment = "OTHER_TOKEN".into();
  assert!(
    configuration.with_authorization(other).is_err(),
    "a credential must target the selected transport mapping"
  );
  let mut unsupported = serde_json::to_value(&approved).unwrap();
  unsupported["authorization"]["purpose"] = "reporter_write".into();
  assert!(serde_json::from_value::<FactoryMcpConfiguration>(unsupported).is_err());
}

#[test]
fn runtime_settings_retain_exact_connection_authorization_in_the_transport_pin() {
  use octacity_server_factory::*;
  let resource = ImmutableReference::new(key("repository"), key("v1"), FactoryDigest::from_bytes([7; 32]));
  let authorization = FactoryMcpAuthorization {
    credential_profile: key("issue-read"),
    secret: LogicalSecretReference::new(SecretProviderId::new("agent-private").unwrap(), "issue-read").unwrap(),
    environment: "ISSUE_TOKEN".into(),
    purpose: FactoryMcpCredentialPurpose::McpRead,
    resource: resource.clone(),
  };
  let document = serde_json::json!({
    "toolchain":{"plugin":resource,"executable":resource,"model":resource},
    "permissions":FactoryPermissionSet::deny_all(), "integrations":{},
    "mcp_servers":{"reader":{"server":{"transport":"http","url":"https://mcp.example/tools","bearer_token_env_var":"ISSUE_TOKEN"},"integration":null,"authorization":authorization}},
  });
  let definition = serde_json::from_value(document)
    .expect("reviewed Runtime settings must retain credential-purpose/resource metadata");
  let settings = FactoryRuntimeSettings::new(
    octacity_server_domain::ProjectId::generate(),
    key("runtime"),
    FactoryRuntimeSettingsVersion::INITIAL,
    definition,
  )
  .unwrap();
  let restored: FactoryRuntimeSettings = serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
  assert_eq!(
    restored
      .mcp_configuration(&key("reader"))
      .unwrap()
      .unwrap()
      .authorization(),
    Some(&authorization)
  );
  assert_eq!(
    restored.execution().unwrap().mcp_servers[&key("reader")].configuration,
    *restored.mcp_configuration(&key("reader")).unwrap().unwrap().reference()
  );
}
