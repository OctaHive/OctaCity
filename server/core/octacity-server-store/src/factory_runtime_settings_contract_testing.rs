//! Reusable journal contract for memory and PostgreSQL; no persistence internals are inspected.
use crate::{testing::*, *};
use octacity_server_domain::EntityKind;
use octacity_server_factory::FactoryRuntimeSettingsVersion;
/// Verifies exact history, optimistic replacement, scoped replay and duplicate-free evidence.
/// The owning Project must already exist and have no settings for this Runtime.
pub async fn verify_factory_runtime_settings_publication<
  S: FactoryRuntimeSettingsStore,
  P: MutationEvidenceProbe + ManagementAuditProbe,
>(
  store: &S,
  evidence: &P,
  intent: FactoryRuntimeSettingsIntent,
) {
  let time = |value| octacity_server_domain::Timestamp::from_unix_millis(value).unwrap();
  assert!(
    store
      .factory_runtime_settings(intent.project_id, &intent.runtime, None)
      .await
      .unwrap()
      .is_none()
  );
  let before = evidence.mutation_evidence_counts().await;
  let mut expected_audits = evidence.management_audit_facts().await;
  let initial = PublishFactoryRuntimeSettings {
    intent: intent.clone(),
    idempotency_key: IdempotencyKey::new("runtime-publication").unwrap(),
    published_at: time(1),
  };
  let first = store
    .publish_factory_runtime_settings(management_mutation(initial.clone()))
    .await
    .unwrap();
  assert_eq!(first.disposition, MutationDisposition::Applied);
  assert_eq!(first.settings.version(), FactoryRuntimeSettingsVersion::INITIAL);
  let first_count = evidence.mutation_evidence_counts().await;
  assert_eq!(
    first_count,
    MutationEvidenceCounts {
      idempotency: before.idempotency + 1,
      audit: before.audit + 1,
      outbox: before.outbox + 1
    }
  );
  let mut replacement = initial.clone();
  replacement.intent.expected_version = Some(first.settings.version());
  replacement.intent.definition.mcp_servers.clear();
  replacement.idempotency_key = IdempotencyKey::new("runtime-replacement").unwrap();
  let second = store
    .publish_factory_runtime_settings(management_mutation_with_request(
      replacement.clone(),
      "runtime-replacement-request",
    ))
    .await
    .unwrap();
  assert_eq!(second.settings.version().get(), 2);
  replacement.idempotency_key = IdempotencyKey::new("runtime-stale").unwrap();
  assert!(matches!(
    store
      .publish_factory_runtime_settings(management_mutation(replacement))
      .await,
    Err(FactoryRuntimeSettingsPublicationError::PreconditionFailed)
  ));
  assert_eq!(
    store
      .factory_runtime_settings(intent.project_id, &intent.runtime, Some(first.settings.version()))
      .await
      .unwrap(),
    Some(first.settings.clone())
  );
  assert_eq!(
    store
      .factory_runtime_settings(intent.project_id, &intent.runtime, None)
      .await
      .unwrap(),
    Some(second.settings)
  );
  let count = evidence.mutation_evidence_counts().await;
  let mut retry = initial.clone();
  retry.published_at = time(100);
  let replay = store
    .publish_factory_runtime_settings(management_mutation(retry))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.settings, first.settings);
  assert_eq!(replay.published_at, time(1));
  let mut conflicting = initial.clone();
  conflicting.intent.definition.mcp_servers.clear();
  assert!(
    store
      .publish_factory_runtime_settings(management_mutation(conflicting))
      .await
      .is_err()
  );
  let foreign = MutationAuditContext::try_new(
    AuditActor {
      kind: AuditActorKind::AuthenticatedManagement,
      identity: Some("other-administrator".into()),
    },
    ManagementSecurityScope::new("other-runtime-scope").unwrap(),
    "foreign-runtime-replay",
  )
  .unwrap();
  assert!(
    store
      .replay_factory_runtime_settings(ManagementMutation::new(
        ReplayFactoryRuntimeSettings {
          intent: initial.intent,
          idempotency_key: initial.idempotency_key
        },
        foreign
      ))
      .await
      .unwrap()
      .is_none()
  );
  assert_eq!(evidence.mutation_evidence_counts().await, count);
  let first_audit = expected_management_audit(
    StoreOperation::PublishFactoryRuntimeSettings,
    EntityKind::Project,
    intent.project_id,
  );
  let mut second_audit = first_audit.clone();
  second_audit.request_identity = "runtime-replacement-request".into();
  expected_audits.extend([first_audit, second_audit]);
  assert_management_audit_facts(evidence, expected_audits).await;
}
