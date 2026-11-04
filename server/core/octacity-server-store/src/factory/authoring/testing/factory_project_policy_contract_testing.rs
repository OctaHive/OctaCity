//! Backend-neutral policy journal contract exercised against the real store.
use crate::{testing::*, *};
use octacity_server_domain::{EntityKind, Timestamp};

/// Checks immutable publication, exact reads and scoped, evidence-free replay.
/// The owning Project must exist and the selected policy identity must be unused.
pub async fn verify_factory_project_policy_publication<
  S: FactoryProjectPolicyStore,
  P: MutationEvidenceProbe + ManagementAuditProbe,
>(
  store: &S,
  evidence: &P,
  policy: FactoryProjectPolicyRecord,
) {
  let reference = policy.reference();
  let before = evidence.mutation_evidence_counts().await;
  let mut audits = evidence.management_audit_facts().await;
  assert!(
    store
      .factory_project_policy(reference.project_id(), &reference)
      .await
      .unwrap()
      .is_none()
  );
  let request = PublishFactoryProjectPolicy {
    intent: FactoryProjectPolicyIntent {
      policy: policy.clone(),
      expected_current: None,
    },
    idempotency_key: IdempotencyKey::new("policy-create").unwrap(),
    published_at: Timestamp::from_unix_millis(1).unwrap(),
  };
  let first = store
    .publish_factory_project_policy(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(first.disposition, MutationDisposition::Applied);
  assert_eq!(first.policy, policy);
  assert_eq!(
    store
      .factory_project_policy(reference.project_id(), &reference)
      .await
      .unwrap(),
    Some(policy)
  );
  let accepted = evidence.mutation_evidence_counts().await;
  assert_eq!(
    accepted,
    MutationEvidenceCounts {
      idempotency: before.idempotency + 1,
      audit: before.audit + 1,
      outbox: before.outbox + 1
    }
  );
  let mut retry = request.clone();
  retry.published_at = Timestamp::from_unix_millis(500).unwrap();
  let replay = store
    .publish_factory_project_policy(management_mutation(retry))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.policy, first.policy);
  assert_eq!(replay.published_at, first.published_at);
  assert_eq!(evidence.mutation_evidence_counts().await, accepted);
  audits.push(expected_management_audit(
    StoreOperation::PublishFactoryProjectPolicy,
    EntityKind::Project,
    reference.project_id(),
  ));
  assert_management_audit_facts(evidence, audits).await;
}
