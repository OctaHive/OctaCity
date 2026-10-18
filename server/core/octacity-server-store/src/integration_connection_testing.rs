//! Shared public connection fixtures and memory adapter.
use crate::*;
use octacity_server_factory::{FactoryDigest, FactoryKey, ImmutableReference};
use serde_json::json;
fn exact(name: &str) -> ImmutableReference {
  ImmutableReference::new(
    FactoryKey::new(name).unwrap(),
    FactoryKey::new("v1").unwrap(),
    FactoryDigest::from_bytes([7; 32]),
  )
}
/// Builds one existing installed connection and its credential-free selection.
pub fn integration_connection_fixture(provider: &str) -> (InstalledIntegration, IntegrationConnectionDraft) {
  let resource = exact("ticket-container");
  let installation = exact("existing-installation");
  let installed = serde_json::from_value(json!({
    "installation":installation,"provider":provider,"adapter":exact("verified-adapter"),
    "endpoints":["https://tracker.invalid/api/"],"authentication":["token"],
    "resources":[{"reference":resource,"native_id":"external-project-42","locator":"owner/project",
      "capabilities":[{"purpose":"task_read","capability":exact("issue-read"),
        "configuration":exact("source-profile"),"credential":{"provider":"installed","reference":"existing-reader"}}]}]
  }))
  .unwrap();
  let draft = serde_json::from_value(json!({"name":"Team tracker","installation":installation,
    "endpoint":"https://tracker.invalid/api/","authentication":"token",
    "resources":[{"reference":resource,"purposes":["task_read"]}]}))
  .unwrap();
  (installed, draft)
}

/// Exercises append-only publication, exact replay and strong current-version preconditions.
pub async fn verify_connection_publication<S: IntegrationConnectionStore>(store: &S) {
  use octacity_server_domain::{IntegrationId, IntegrationVersion, Timestamp};
  let id = IntegrationId::generate();
  let (installed, draft) = integration_connection_fixture("custom-tracker");
  let mut request = PublishIntegrationConnection {
    intent: IntegrationConnectionIntent {
      id,
      expected_current_version: None,
      draft,
    },
    installed,
    idempotency_key: IdempotencyKey::new("connection-create").unwrap(),
    published_at: Timestamp::from_unix_millis(10).unwrap(),
  };
  let original = store
    .publish_integration_connection(crate::testing::management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(original.disposition, MutationDisposition::Applied);
  assert_eq!(original.connection.version, IntegrationVersion::INITIAL);
  request.published_at = Timestamp::from_unix_millis(20).unwrap();
  let replay = store
    .publish_integration_connection(crate::testing::management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.connection, original.connection);
  let creation = request.clone();
  let foreign = MutationAuditContext::try_new(
    AuditActor {
      kind: AuditActorKind::AuthenticatedManagement,
      identity: Some("another-administrator".into()),
    },
    ManagementSecurityScope::new("another-management-scope").unwrap(),
    "foreign-replay",
  )
  .unwrap();
  assert!(
    store
      .replay_integration_connection(ManagementMutation::new(
        ReplayIntegrationConnection {
          intent: creation.intent.clone(),
          idempotency_key: creation.idempotency_key.clone(),
        },
        foreign
      ))
      .await
      .unwrap()
      .is_none()
  );
  let first = original.connection;
  request.intent.expected_current_version = Some(IntegrationVersion::INITIAL);
  request.intent.draft.name = octacity_server_domain::IntegrationName::new("Renamed tracker").unwrap();
  request.idempotency_key = IdempotencyKey::new("connection-replace").unwrap();
  let second = store
    .publish_integration_connection(crate::testing::management_mutation(request.clone()))
    .await
    .unwrap()
    .connection;
  assert_eq!(second.version.get(), 2);
  assert_eq!(second.id, first.id);
  assert_eq!(second.definition.installation, first.definition.installation);
  assert_eq!(
    store
      .integration_connection(id, Some(IntegrationVersion::INITIAL))
      .await
      .unwrap(),
    first
  );
  assert_eq!(store.integration_connection(id, None).await.unwrap(), second);
  // A later version or missing installation must not change an accepted receipt.
  let mut historical = creation.clone();
  historical.installed.endpoints.clear();
  let replay = store
    .publish_integration_connection(crate::testing::management_mutation(historical))
    .await
    .unwrap();
  assert_eq!(replay.connection, first);
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  let probe = store
    .replay_integration_connection(crate::testing::management_mutation(ReplayIntegrationConnection {
      intent: creation.intent.clone(),
      idempotency_key: creation.idempotency_key.clone(),
    }))
    .await
    .unwrap()
    .unwrap();
  assert_eq!(probe.connection, first);
  let mut changed = creation;
  changed.intent.draft.name = octacity_server_domain::IntegrationName::new("Different intent").unwrap();
  assert!(
    store
      .publish_integration_connection(crate::testing::management_mutation(changed))
      .await
      .is_err()
  );
  let mut invalid = request.clone();
  invalid.intent.expected_current_version = Some(second.version);
  invalid.idempotency_key = IdempotencyKey::new("invalid-update").unwrap();
  invalid.intent.draft.endpoint = "https://uninstalled.invalid/".into();
  assert!(
    store
      .publish_integration_connection(crate::testing::management_mutation(invalid))
      .await
      .is_err()
  );
  request.idempotency_key = IdempotencyKey::new("stale-replacement").unwrap();
  assert!(
    store
      .publish_integration_connection(crate::testing::management_mutation(request))
      .await
      .is_err()
  );
  assert_eq!(store.integration_connection(id, None).await.unwrap(), second);
}

#[derive(Default)]
pub(super) struct IntegrationMemoryState {
  access: std::collections::BTreeMap<
    (octacity_server_domain::IntegrationId, octacity_server_domain::ProjectId),
    PublishedIntegrationProjectAccess,
  >,
  access_mutations: std::collections::BTreeMap<
    ManagementIdempotencyKey,
    (IntegrationProjectAccessIntent, IntegrationProjectAccessOutcome),
  >,
  administration: std::collections::BTreeMap<octacity_server_domain::IntegrationId, IntegrationAdministrationState>,
  operations: std::collections::BTreeMap<
    ManagementIdempotencyKey,
    (IntegrationAdministrationIntent, IntegrationAdministrationReceipt, bool),
  >,
  credential_owners:
    std::collections::BTreeMap<octacity_server_secrets::LogicalSecretReference, octacity_server_domain::IntegrationId>,
  current:
    std::collections::BTreeMap<octacity_server_domain::IntegrationId, octacity_server_domain::IntegrationVersion>,
  versions: std::collections::BTreeMap<
    (
      octacity_server_domain::IntegrationId,
      octacity_server_domain::IntegrationVersion,
    ),
    PublishedIntegrationConnection,
  >,
  mutations: std::collections::BTreeMap<
    ManagementIdempotencyKey,
    (IntegrationConnectionIntent, IntegrationConnectionMutationOutcome),
  >,
}

impl IntegrationMemoryState {
  pub(super) fn validate_source_input(
    &self,
    input: &octacity_server_factory::WorkSourceInput,
  ) -> Result<(), StoreError> {
    let Some(connection) = &input.definition().connection else {
      return Ok(());
    };
    if self.current.get(&connection.id) != Some(&connection.version) {
      return Err(crate::source_conflict());
    }
    let metadata = self
      .versions
      .get(&(connection.id, connection.version))
      .ok_or_else(crate::source_conflict)?;
    let access = self
      .access
      .get(&(connection.id, input.project_id()))
      .ok_or_else(crate::source_conflict)?;
    let administration = self
      .administration
      .get(&connection.id)
      .ok_or_else(crate::source_conflict)?;
    crate::validate_managed_work_source(input, metadata, access, administration)
  }
}
fn replay_connection(
  state: &IntegrationMemoryState,
  key: &ManagementIdempotencyKey,
  intent: &IntegrationConnectionIntent,
) -> Result<Option<IntegrationConnectionMutationOutcome>, StoreError> {
  match state.mutations.get(key) {
    None => Ok(None),
    Some((original, _)) if original != intent => Err(connection_conflict()),
    Some((_, outcome)) => {
      let mut outcome = outcome.clone();
      outcome.disposition = MutationDisposition::Replayed;
      Ok(Some(outcome))
    }
  }
}
#[async_trait::async_trait]
impl IntegrationConnectionStore for crate::testing::InMemoryFactoryConfigurationStore {
  async fn list_integration_connections(
    &self,
    request: ReadIntegrationConnections,
  ) -> Result<IntegrationConnectionPage, StoreError> {
    request.validate()?;
    let state = self.lock()?;
    let mut items: Vec<_> = state
      .integrations
      .current
      .iter()
      .filter(|(id, _)| request.visibility.allows(id) && request.after.is_none_or(|after| **id > after))
      .filter_map(|(id, version)| state.integrations.versions.get(&(*id, *version)).cloned())
      .take(usize::from(request.limit) + 1)
      .collect();
    let next_cursor = (items.len() > usize::from(request.limit)).then(|| items[usize::from(request.limit) - 1].id);
    items.truncate(usize::from(request.limit));
    Ok(IntegrationConnectionPage { items, next_cursor })
  }
  async fn replay_integration_connection(
    &self,
    request: ManagementMutation<ReplayIntegrationConnection>,
  ) -> Result<Option<IntegrationConnectionMutationOutcome>, StoreError> {
    let (request, audit) = request.into_parts();
    replay_connection(
      &self.lock()?.integrations,
      &audit.scoped_idempotency_key(&request.idempotency_key),
      &request.intent,
    )
  }
  async fn publish_integration_connection(
    &self,
    request: ManagementMutation<PublishIntegrationConnection>,
  ) -> Result<IntegrationConnectionMutationOutcome, StoreError> {
    use octacity_server_domain::EntityKind;
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some(outcome) = replay_connection(&state.integrations, &key, &request.intent)? {
      return Ok(outcome);
    }
    if state.integrations.current.get(&request.intent.id).copied() != request.intent.expected_current_version {
      return Err(connection_conflict());
    }
    let connection = request.resolve()?;
    let outcome = IntegrationConnectionMutationOutcome {
      disposition: MutationDisposition::Applied,
      connection: connection.clone(),
    };
    state.integrations.current.insert(connection.id, connection.version);
    state
      .integrations
      .versions
      .insert((connection.id, connection.version), connection);
    state
      .integrations
      .mutations
      .insert(key.clone(), (request.intent.clone(), outcome.clone()));
    state.evidence.insert(format!(
      "publish-integration-connection:{}:{}",
      key.security_scope().as_str(),
      key.caller_key()
    ));
    state.audit.insert(crate::testing::recorded_management_audit(
      &audit,
      StoreOperation::PublishIntegrationConnection,
      EntityKind::Integration,
      request.intent.id,
    ));
    Ok(outcome)
  }
  async fn integration_connection(
    &self,
    id: octacity_server_domain::IntegrationId,
    version: Option<octacity_server_domain::IntegrationVersion>,
  ) -> Result<PublishedIntegrationConnection, StoreError> {
    let state = self.lock()?;
    let version = version.or_else(|| state.integrations.current.get(&id).copied());
    version
      .and_then(|v| state.integrations.versions.get(&(id, v)).cloned())
      .ok_or(StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Integration,
      })
  }
}

fn connection_conflict() -> StoreError {
  StoreError::Conflict {
    entity: octacity_server_domain::EntityKind::Integration,
  }
}

#[async_trait::async_trait]
impl IntegrationAdministrationStore for crate::testing::InMemoryFactoryConfigurationStore {
  async fn begin_integration_administration(
    &self,
    request: ManagementMutation<BeginIntegrationAdministration>,
  ) -> Result<IntegrationAdministrationReceipt, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    let integrations = &mut state.integrations;
    if let Some((intent, receipt, _)) = integrations.operations.get(&key) {
      if *intent != request.intent {
        return Err(connection_conflict());
      }
      return Ok(receipt.clone());
    }
    let metadata_version = integrations
      .current
      .get(&request.intent.id)
      .ok_or_else(connection_conflict)?;
    let metadata = integrations
      .versions
      .get(&(request.intent.id, *metadata_version))
      .ok_or_else(connection_conflict)?;
    let mut admin = integrations
      .administration
      .get(&request.intent.id)
      .cloned()
      .unwrap_or_else(|| IntegrationAdministrationState::new(request.intent.id));
    if let IntegrationAdministrationAction::Credential { reference, .. } = &request.intent.action
      && integrations
        .credential_owners
        .get(reference)
        .is_some_and(|id| *id != request.intent.id)
    {
      return Err(connection_conflict());
    }
    let previous_pending = admin.pending.as_ref().map(|p| p.operation);
    let receipt = admin.begin(&request, metadata, audit.security_scope())?;
    if let Some(pending) = previous_pending {
      for (_, old, _) in integrations.operations.values_mut() {
        if old.operation == pending {
          old.status = IntegrationAdministrationStatus::Abandoned;
        }
      }
    }
    if let IntegrationAdministrationAction::Credential { reference, .. } = &request.intent.action {
      integrations
        .credential_owners
        .insert(reference.clone(), request.intent.id);
    }
    integrations.administration.insert(request.intent.id, admin);
    integrations
      .operations
      .insert(key.clone(), (request.intent.clone(), receipt.clone(), false));
    state.evidence.insert(format!(
      "begin-integration-administration:{}:{}",
      key.security_scope().as_str(),
      key.caller_key()
    ));
    state.audit.insert(crate::testing::recorded_management_audit(
      &audit,
      StoreOperation::BeginIntegrationAdministration,
      octacity_server_domain::EntityKind::Integration,
      request.intent.id,
    ));
    Ok(receipt)
  }
  async fn complete_integration_administration(
    &self,
    request: ManagementMutation<IntegrationAdministrationReceipt>,
  ) -> Result<IntegrationAdministrationReceipt, StoreError> {
    let (request, audit) = request.into_parts();
    let mut state = self.lock()?;
    let integrations = &mut state.integrations;
    let key = integrations
      .operations
      .iter()
      .find(|(key, (_, r, _))| r.operation == request.operation && key.security_scope() == audit.security_scope())
      .map(|(key, _)| key.clone())
      .ok_or_else(connection_conflict)?;
    let mut receipt = integrations.operations.get(&key).unwrap().1.clone();
    if receipt.intent != request.intent || receipt.publication != request.publication {
      return Err(connection_conflict());
    }
    if receipt.status == IntegrationAdministrationStatus::Completed {
      return Ok(receipt);
    }
    if integrations.current.get(&receipt.intent.id) != Some(&receipt.intent.expected_metadata_version) {
      return Err(connection_conflict());
    }
    let metadata_version = integrations
      .current
      .get(&request.intent.id)
      .ok_or_else(connection_conflict)?;
    let metadata = integrations
      .versions
      .get(&(request.intent.id, *metadata_version))
      .ok_or_else(connection_conflict)?;
    integrations
      .administration
      .get_mut(&request.intent.id)
      .ok_or_else(connection_conflict)?
      .complete(&mut receipt, metadata)?;
    integrations.operations.get_mut(&key).unwrap().1 = receipt.clone();
    state.evidence.insert(format!(
      "complete-integration-administration:{}:{}",
      key.security_scope().as_str(),
      key.caller_key()
    ));
    state.audit.insert(crate::testing::recorded_management_audit(
      &audit,
      StoreOperation::CompleteIntegrationAdministration,
      octacity_server_domain::EntityKind::Integration,
      request.intent.id,
    ));
    Ok(receipt)
  }
  async fn integration_administration(
    &self,
    id: octacity_server_domain::IntegrationId,
  ) -> Result<IntegrationAdministrationState, StoreError> {
    let state = self.lock()?;
    if !state.integrations.current.contains_key(&id) {
      return Err(connection_conflict());
    }
    Ok(
      state
        .integrations
        .administration
        .get(&id)
        .cloned()
        .unwrap_or_else(|| IntegrationAdministrationState::new(id)),
    )
  }
  async fn integration_preparation_cleanup(
    &self,
    request: ManagementMutation<IntegrationPreparationCleanup>,
  ) -> Result<Vec<octacity_server_secrets::CredentialPublication>, StoreError> {
    let (request, audit) = request.into_parts();
    if request.limit == 0 || request.limit > 100 {
      return Err(connection_conflict());
    }
    let mut state = self.lock()?;
    let integrations = &mut state.integrations;
    let mut result = Vec::new();
    let mut abandoned = Vec::new();
    for (_, receipt, cleaned) in integrations.operations.values_mut() {
      if receipt.intent.id != request.id || *cleaned || receipt.requested_at > request.older_than {
        continue;
      }
      if let Some(publication) = receipt.publication.clone() {
        if receipt.status == IntegrationAdministrationStatus::Prepared {
          receipt.status = IntegrationAdministrationStatus::Abandoned;
          abandoned.push(publication.clone());
        }
        result.push(publication);
        if result.len() == usize::from(request.limit) {
          break;
        }
      }
    }
    if let Some(admin) = integrations.administration.get_mut(&request.id) {
      for publication in &abandoned {
        admin.abandon(publication);
      }
    }
    for publication in abandoned {
      state.evidence.insert(format!(
        "abandon-integration-preparation:{}",
        octacity_server_factory::FactoryDigest::from_bytes(publication.operation)
      ));
      state.audit.insert(crate::testing::recorded_management_audit(
        &audit,
        StoreOperation::AbandonIntegrationPreparation,
        octacity_server_domain::EntityKind::Integration,
        request.id,
      ));
    }
    Ok(result)
  }
  async fn acknowledge_integration_preparation_cleanup(
    &self,
    request: ManagementMutation<octacity_server_secrets::CredentialPublication>,
  ) -> Result<(), StoreError> {
    let (request, _) = request.into_parts();
    let mut state = self.lock()?;
    let (_, receipt, cleaned) = state
      .integrations
      .operations
      .values_mut()
      .find(|(_, r, _)| r.publication.as_ref() == Some(&request))
      .ok_or_else(connection_conflict)?;
    if receipt.status == IntegrationAdministrationStatus::Prepared {
      return Err(connection_conflict());
    }
    *cleaned = true;
    Ok(())
  }
}

#[async_trait::async_trait]
impl IntegrationProjectAccessStore for crate::testing::InMemoryFactoryConfigurationStore {
  async fn publish_integration_project_access(
    &self,
    request: ManagementMutation<PublishIntegrationProjectAccess>,
  ) -> Result<IntegrationProjectAccessOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some((intent, outcome)) = state.integrations.access_mutations.get(&key) {
      if *intent != request.intent {
        return Err(connection_conflict());
      }
      let mut replay = outcome.clone();
      replay.disposition = MutationDisposition::Replayed;
      return Ok(replay);
    }
    let intent = &request.intent;
    if !state.projects.contains(&intent.project_id) {
      return Err(connection_conflict());
    }
    let scope = (intent.connection_id, intent.project_id);
    if state.integrations.current.get(&intent.connection_id) != Some(&intent.connection_version)
      || state.integrations.access.get(&scope).map(|a| a.version) != intent.expected_current_version
    {
      return Err(connection_conflict());
    }
    let metadata = state
      .integrations
      .versions
      .get(&(intent.connection_id, intent.connection_version))
      .ok_or_else(connection_conflict)?;
    let access = request.resolve(metadata)?;
    let outcome = IntegrationProjectAccessOutcome {
      disposition: MutationDisposition::Applied,
      access: access.clone(),
    };
    state.integrations.access.insert(scope, access);
    state
      .integrations
      .access_mutations
      .insert(key.clone(), (request.intent.clone(), outcome.clone()));
    state.evidence.insert(format!(
      "publish-integration-project-access:{}:{}",
      key.security_scope().as_str(),
      key.caller_key()
    ));
    state.audit.insert(crate::testing::recorded_management_audit(
      &audit,
      StoreOperation::PublishIntegrationProjectAccess,
      octacity_server_domain::EntityKind::Integration,
      intent.connection_id,
    ));
    Ok(outcome)
  }
  async fn integration_project_access(
    &self,
    connection_id: octacity_server_domain::IntegrationId,
    project_id: octacity_server_domain::ProjectId,
  ) -> Result<Option<PublishedIntegrationProjectAccess>, StoreError> {
    Ok(
      self
        .lock()?
        .integrations
        .access
        .get(&(connection_id, project_id))
        .cloned(),
    )
  }
}
