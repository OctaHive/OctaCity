use super::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
use octacity_work_source_protocol::{ConfigurationReference, CredentialClass, CredentialHandle, ResourceAccess};
fn key(s: &str) -> FactoryKey {
  FactoryKey::new(s).unwrap()
}
fn reference(s: &str) -> ImmutableReference {
  ImmutableReference::new(key(s), key("v1"), FactoryDigest::from_bytes([1; 32]))
}
pub(super) fn draft(provider: &str) -> FactoryReporterProfileDraft {
  FactoryReporterProfileDraft {
    project_id: ProjectId::generate(),
    installation: reference(provider),
    capability: reference("source-status"),
    resource: reference("ticket-space"),
    plugin: reference(provider),
    provider_configuration: FactoryDigest::from_bytes([2; 32]),
    access: ResourceAccess {
      resource_id: "native-container".into(),
      resource_locator: "private-container".into(),
      credential: CredentialHandle {
        class: CredentialClass::WorkReporterWrite,
        handle: "private-writer".into(),
      },
    },
    mapping: ConfigurationReference {
      identity: "reviewed-mapping".into(),
      sha256: "c".repeat(64),
    },
    credential_profile: key("writer"),
    network_hosts: vec!["tracker.invalid".into()],
  }
}
#[test]
fn reporter_profiles_freeze_every_provider_and_authority_selection_without_business_vocabulary() {
  for provider in ["github", "gitlab", "jira", "custom-tracker"] {
    let original = draft(provider);
    let profile = FactoryReporterProfile::new(key("effects"), key("v1"), original.clone()).unwrap();
    assert_eq!(profile.binding().installation, original.installation);
    assert_eq!(profile.binding().capability, original.capability);
    assert_eq!(profile.binding().resource, original.resource);
    assert_eq!(
      FactoryReporterProfile::new(key("effects"), key("v1"), original.clone())
        .unwrap()
        .binding(),
      profile.binding()
    );
    for changed in [0, 1, 2, 3] {
      let mut replacement = original.clone();
      match changed {
        0 => replacement.mapping.sha256 = "d".repeat(64),
        1 => replacement.provider_configuration = FactoryDigest::from_bytes([3; 32]),
        2 => replacement.access.resource_id = "another-container".into(),
        3 => replacement.access.credential.handle = "another-private-writer".into(),
        _ => unreachable!(),
      }
      assert_ne!(
        FactoryReporterProfile::new(key("effects"), key("v1"), replacement)
          .unwrap()
          .binding(),
        profile.binding()
      );
    }
  }
}

#[test]
fn reporter_profiles_reject_borrowed_read_authority_and_invalid_native_configuration() {
  for case in [
    "read-grant",
    "empty-hosts",
    "duplicate-hosts",
    "invalid-host",
    "mapping",
    "locator",
  ] {
    let mut draft = draft("arbitrary-provider");
    match case {
      "read-grant" => draft.access.credential.class = CredentialClass::WorkSourceRead,
      "empty-hosts" => draft.network_hosts.clear(),
      "duplicate-hosts" => draft.network_hosts.push("tracker.invalid".into()),
      "invalid-host" => draft.network_hosts = vec!["https://tracker.invalid/secret".into()],
      "mapping" => draft.mapping.sha256 = "not-a-digest".into(),
      "locator" => draft.access.resource_locator = "\n".into(),
      _ => unreachable!(),
    }
    assert!(
      FactoryReporterProfile::new(key("effects"), key("v1"), draft).is_err(),
      "{case}"
    );
  }
}

#[test]
fn hosted_reporter_installation_rejects_unbounded_calls_and_ambiguous_profile_bindings() {
  use std::{sync::Arc, time::Duration};
  let store = Arc::new(octacity_server_store::testing::InMemoryFactoryConfigurationStore::new());
  let root = tempfile::tempdir().unwrap();
  let registry = Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(root.path()).unwrap());
  let profile = FactoryReporterProfile::new(key("effects"), key("v1"), draft("tracker")).unwrap();
  for (timeout, grace, profiles) in [
    (Duration::ZERO, Duration::from_millis(10), vec![profile.clone()]),
    (Duration::from_secs(1), Duration::ZERO, vec![profile.clone()]),
    (
      Duration::from_secs(1),
      Duration::from_millis(10),
      vec![profile.clone(), profile.clone()],
    ),
    (Duration::from_secs(1), Duration::from_millis(10), vec![profile; 257]),
  ] {
    assert!(
      HostedFactoryWorkStatusProjector::new(
        store.clone(),
        registry.clone(),
        profiles,
        Arc::new(octacity_server_application::SystemFactoryExecutionClock),
        timeout,
        grace,
        tokio_util::sync::CancellationToken::new()
      )
      .is_err()
    );
  }
}

#[tokio::test]
async fn reporter_metadata_reads_share_the_original_deadline_even_when_the_store_is_stalled() {
  use octacity_server_application::{
    FactoryExecutionClock, FactoryWorkStatusProjector, FactoryWorkStatusRequest, SystemFactoryExecutionClock,
  };
  use std::{sync::Arc, time::Duration};
  // An owned TCP peer deliberately never speaks PostgreSQL; this is injected dependency latency.
  let peer = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let pool = sqlx::postgres::PgPoolOptions::new()
    .acquire_timeout(Duration::from_secs(30))
    .connect_lazy(&format!(
      "postgres://fixture:fixture@{}/fixture",
      peer.local_addr().unwrap()
    ))
    .unwrap();
  let store = Arc::new(octacity_server_store_postgres::PostgresStore::new(pool.clone()));
  let root = tempfile::tempdir().unwrap();
  let registry = Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(root.path()).unwrap());
  let clock = Arc::new(SystemFactoryExecutionClock);
  let now = clock.now().unwrap();
  let projector = HostedFactoryWorkStatusProjector::new(
    store,
    registry,
    vec![],
    clock,
    Duration::from_secs(5),
    Duration::from_millis(10),
    tokio_util::sync::CancellationToken::new(),
  )
  .unwrap();
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let work = fixture.admission.work;
  let request = FactoryWorkStatusRequest {
    operation_id: FactoryDigest::from_bytes([7; 32]),
    outbox_claim: None,
    work_id: work.id(),
    external_identity: work.external_identity().clone(),
    subject: work.subject().clone(),
    status: FactorySafeText::new("configured-status").unwrap(),
    permissions: FactoryPermissionSet::default(),
    budget: BudgetLimit::new(1, 1000, 0, 0, 10000).unwrap(),
    deadline: octacity_server_domain::Timestamp::from_unix_millis(now.unix_millis() + 100).unwrap(),
  };
  let result = tokio::time::timeout(Duration::from_secs(2), projector.set_status(request)).await;
  assert!(
    result.is_ok(),
    "metadata reads cannot outlive the original projection deadline"
  );
  assert!(result.unwrap().is_err());
  pool.close().await;
}
