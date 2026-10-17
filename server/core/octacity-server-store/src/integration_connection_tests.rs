use super::*;
use crate::integration_connection_testing::{integration_connection_fixture, verify_connection_publication};
use octacity_server_factory::{FactoryDigest, FactoryKey, ImmutableReference};

fn exact(name: &str) -> ImmutableReference {
  ImmutableReference::new(
    FactoryKey::new(name).unwrap(),
    FactoryKey::new("v1").unwrap(),
    FactoryDigest::from_bytes([7; 32]),
  )
}

#[test]
fn connection_metadata_resolves_open_provider_identity_without_rebinding_installed_source() {
  for provider in ["github", "gitlab", "jira", "custom-tracker"] {
    let (installed, draft) = integration_connection_fixture(provider);
    let definition = IntegrationConnectionDefinition::resolve(&draft, &installed).unwrap();
    assert_eq!(definition.provider.as_str(), provider);
    assert_eq!(definition.adapter, exact("verified-adapter"));
    let binding = definition
      .work_binding(&exact("ticket-container"), IntegrationPurpose::TaskRead)
      .unwrap();
    assert_eq!(
      binding,
      octacity_server_factory::WorkSourceBinding {
        installation: exact("existing-installation"),
        capability: exact("issue-read"),
        configuration: exact("source-profile"),
        resource: exact("ticket-container")
      }
    );
    assert_eq!(definition.resources[0].native_id.as_str(), "external-project-42");
    let document = serde_json::to_value(&definition).unwrap();
    assert!(document.get("project_id").is_none());
    assert!(document.get("repository_id").is_none());
    assert!(document.get("token").is_none());
  }
}

#[test]
fn connection_metadata_rejects_unsafe_endpoints_and_uninstalled_selections() {
  for endpoint in [
    "http://tracker.invalid/",
    "https://token@tracker.invalid/",
    "https://tracker.invalid/?token=private",
    "https://tracker.invalid/#private",
    "https://tracker.invalid/a/../",
    "file:///private",
  ] {
    let (mut installed, mut draft) = integration_connection_fixture("jira");
    installed.endpoints = vec![endpoint.into()];
    draft.endpoint = endpoint.into();
    assert!(
      IntegrationConnectionDefinition::resolve(&draft, &installed).is_err(),
      "{endpoint}"
    );
  }
  for case in [
    "endpoint",
    "authentication",
    "purpose",
    "resource",
    "duplicate",
    "empty",
  ] {
    let (installed, mut draft) = integration_connection_fixture("gitlab");
    match case {
      "endpoint" => draft.endpoint = "https://another.invalid/".into(),
      "authentication" => draft.authentication = FactoryKey::new("unsupported").unwrap(),
      "purpose" => draft.resources[0].purposes = vec![IntegrationPurpose::TaskUpdate],
      "resource" => draft.resources[0].reference = exact("other-resource"),
      "duplicate" => draft.resources.push(draft.resources[0].clone()),
      "empty" => draft.resources[0].purposes.clear(),
      _ => unreachable!(),
    }
    assert!(
      IntegrationConnectionDefinition::resolve(&draft, &installed).is_err(),
      "{case}"
    );
  }
}

#[test]
fn connection_publication_is_immutable_replayable_and_preconditioned() {
  crate::test_support::run_ready(
    async {
      let store = crate::testing::InMemoryFactoryConfigurationStore::new();
      verify_connection_publication(&store).await;
    },
    "connection publication unexpectedly yielded",
  );
}

#[test]
fn connection_selection_cannot_submit_credentials_or_replace_installed_pins() {
  let (installed, draft) = integration_connection_fixture("custom-tracker");
  for field in ["token", "credential", "executable", "provider", "adapter"] {
    let mut document = serde_json::to_value(&draft).unwrap();
    document[field] = serde_json::json!("client-supplied");
    assert!(
      serde_json::from_value::<IntegrationConnectionDraft>(document).is_err(),
      "{field}"
    );
  }
  let mut substituted = draft.clone();
  substituted.installation = exact("different-installation");
  assert!(IntegrationConnectionDefinition::resolve(&substituted, &installed).is_err());
  let mut unselected = installed.clone();
  let mut update = unselected.resources[0].capabilities[0].clone();
  update.purpose = IntegrationPurpose::TaskUpdate;
  update.capability = exact("issue-write");
  unselected.resources[0].capabilities.push(update);
  let definition = IntegrationConnectionDefinition::resolve(&draft, &unselected).unwrap();
  assert_eq!(definition.resources[0].capabilities.len(), 1);
  assert!(
    definition
      .work_binding(&draft.resources[0].reference, IntegrationPurpose::TaskUpdate)
      .is_none()
  );
  assert!(
    definition
      .work_binding(&draft.resources[0].reference, IntegrationPurpose::Delivery)
      .is_none()
  );
}
