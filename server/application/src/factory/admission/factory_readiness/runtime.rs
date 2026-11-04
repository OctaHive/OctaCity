//! Safe exact Runtime facts; metadata compatibility cannot certify remote authentication.
use super::*;
use octacity_server_store::FactoryRuntimeSettingsStore;
pub(super) const MCP_ASPECTS: &[&str] = &[
  "mcp_transport",
  "mcp_secret_reference",
  "mcp_authentication",
  "mcp_scope",
  "mcp_read_only",
  "mcp_capture",
  "octacity_mcp",
];
pub(super) async fn observe<S: FactoryRuntimeSettingsStore>(
  store: &S,
  project: ProjectId,
  choices: Option<&FactoryConfigurationChoices>,
  profile: &FactoryRoleProfile,
  now: Timestamp,
  end: Timestamp,
  subject: FactoryReadinessSubject,
) -> Vec<FactoryReadinessCheck> {
  use FactoryReadinessState::*;
  let definition = profile.definition();
  let support = choices.and_then(|choices| {
    choices
      .role_profile_capabilities()
      .iter()
      .find(|(_, capability)| capability.validate(profile).is_ok())
      .and_then(|(alias, _)| choices.runtime_settings_support(alias))
  });
  let references: Vec<_> = definition
    .execution
    .mcp_servers
    .values()
    .map(|binding| binding.configuration.clone())
    .collect();
  let retained = if references.is_empty() {
    Default::default()
  } else {
    store
      .factory_runtime_mcp_configurations(project, &references)
      .await
      .unwrap_or_default()
  };
  let mut seen = Vec::new();
  let mut model = check("model_authentication", Unknown, "credential_unverified", true, now, end);
  model.subject = Some(subject.clone());
  let mut facts = vec![model];
  for (alias, binding) in &definition.execution.mcp_servers {
    let configuration = retained.get(&binding.configuration).or_else(|| {
      support
        .and_then(|support| support.authenticated_mcp.get(alias))
        .filter(|configuration| configuration.reference() == &binding.configuration)
    });
    let qualified = configuration.zip(support).is_some_and(|(configuration, support)| {
      support.mcp_transports.contains(&configuration.server().transport())
        && match configuration.server() {
          FactoryMcpServer::Http { .. } => true,
          FactoryMcpServer::Stdio { command, .. } => support.mcp_executables.get(command).is_some_and(|executable| {
            definition
              .permissions
              .executables()
              .any(|allowed| allowed == executable)
          }),
        }
    });
    let secret = configuration
      .and_then(|configuration| configuration.authorization())
      .is_some_and(|authorization| {
        let unique = !seen.iter().any(|(credential, secret)| {
          credential == &authorization.credential_profile || secret == &authorization.secret
        });
        seen.push((authorization.credential_profile.clone(), authorization.secret.clone()));
        unique
          && definition.execution.environment.get(&authorization.environment)
            == Some(&FactoryRoleEnvironmentValue::Secret {
              reference: authorization.secret.clone(),
            })
      });
    for aspect in MCP_ASPECTS {
      let (state, reason) = match *aspect {
        "mcp_transport" if qualified => (Ready, "qualified_transport"),
        "mcp_transport" if configuration.is_some() && support.is_some() => (Blocked, "transport_unsupported"),
        "mcp_secret_reference" if secret => (Ready, "connection_binding"),
        "mcp_secret_reference" if configuration.is_some() => (Blocked, "credential_binding_incompatible"),
        "mcp_authentication" => (Unknown, "credential_unverified"),
        "mcp_scope" => (Unknown, "peer_scope_unverified"),
        "mcp_read_only" => (Unknown, "enforcement_unobserved"),
        "mcp_capture" => (Unknown, "capture_unqualified"),
        "octacity_mcp" => (Unknown, "scoped_authentication_unobserved"),
        _ => (Unknown, "configuration_unavailable"),
      };
      let mut fact = check(aspect, state, reason, true, now, end);
      fact.subject = Some(FactoryReadinessSubject {
        dependency: Some(alias.clone()),
        ..subject.clone()
      });
      facts.push(fact);
    }
  }
  facts
}
