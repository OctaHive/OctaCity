//! Deletion contracts shared by memory and PostgreSQL, including existing Run history.
use crate::{testing::*, *};
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::*;
fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
fn publication(intent: FactoryRoleProfilesIntent, key: &str, at: i64) -> PublishFactoryRoleProfiles {
  PublishFactoryRoleProfiles {
    intent,
    idempotency_key: IdempotencyKey::new(key).unwrap(),
    published_at: time(at),
  }
}
/// Deletion hides current choices while preserving immutable references and replay evidence.
pub async fn verify_factory_role_deletion<
  S: FactoryRoleProfileStore + FactoryConfigurationStore,
  P: MutationEvidenceProbe,
>(
  store: &S,
  evidence: &P,
  project: ProjectId,
) {
  let intent = role_profile_contract_intent(project);
  let original = store
    .publish_factory_role_profiles(management_mutation(publication(intent.clone(), "before-delete", 1)))
    .await
    .unwrap();
  let role = original.role.as_ref().unwrap();
  let command = DeleteFactoryRole {
    project_id: project,
    id: role.reference().id,
    expected_current_version: role.reference().version,
    idempotency_key: IdempotencyKey::new("delete-agent").unwrap(),
    deleted_at: Timestamp::from_unix_millis(2).unwrap(),
  };
  let before = evidence.mutation_evidence_counts().await;
  let mut stale = command.clone();
  stale.expected_current_version = FactoryRoleVersion::new(2).unwrap();
  assert!(matches!(
    store.delete_factory_role(management_mutation(stale)).await,
    Err(FactoryRoleProfilePublicationError::PreconditionFailed)
  ));
  assert_eq!(evidence.mutation_evidence_counts().await, before);
  let deleted = store
    .delete_factory_role(management_mutation(command.clone()))
    .await
    .unwrap();
  assert_eq!(deleted.disposition, MutationDisposition::Applied);
  let FactoryConfigurationAvailability::Available(choices) =
    store.factory_configuration_availability(project).await.unwrap()
  else {
    panic!("fixture must enable Factory choices");
  };
  assert!(
    !choices.role_is_available(command.id),
    "deleted profiles must be rejected by the common candidate qualification"
  );
  assert_eq!(store.list_factory_roles(project, None, 25).await.unwrap().total, 0);
  assert!(matches!(
    store.factory_role(project, command.id, None).await,
    Err(StoreError::NotFound { .. })
  ));
  assert!(
    store
      .list_factory_role_profiles(project, command.id, None, 25)
      .await
      .is_err()
  );
  assert_eq!(
    store
      .factory_role(project, command.id, Some(command.expected_current_version))
      .await
      .unwrap(),
    *role
  );
  for profile in &original.profiles {
    assert!(
      store
        .factory_role_profile(project, profile.reference().id, None)
        .await
        .is_err()
    );
    assert_eq!(
      store
        .factory_role_profile(project, profile.reference().id, Some(profile.reference().version))
        .await
        .unwrap(),
      *profile
    );
  }
  let after = evidence.mutation_evidence_counts().await;
  assert_eq!(
    (
      after.idempotency - before.idempotency,
      after.audit - before.audit,
      after.outbox - before.outbox
    ),
    (1, 1, 1)
  );
  let replay = store.delete_factory_role(management_mutation(command)).await.unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.role, deleted.role);
  assert_eq!(replay.deleted_at, deleted.deleted_at);
  assert_eq!(evidence.mutation_evidence_counts().await, after);
  // Original publications still replay; new writes cannot resurrect an agent.
  assert_eq!(
    store
      .publish_factory_role_profiles(management_mutation(publication(intent.clone(), "before-delete", 4)))
      .await
      .unwrap()
      .role,
    original.role
  );
  assert!(
    store
      .publish_factory_role_profiles(management_mutation(publication(intent, "resurrect", 4)))
      .await
      .is_err()
  );
}

/// Applies deletion through the public catalog port and rejects stale live-control forms atomically.
pub async fn verify_factory_role_deletion_blocks_live_selection<
  S: FactoryRoleProfileStore + FactoryRunStore + FactoryRunControlStore,
>(
  store: &S,
  admission: &PublishedFactoryAdmission,
  input: &FlowNodeInput,
  ownership: &FactoryClaimOwnership,
) {
  let binding = admission
    .flow
    .closure()
    .definition(input.definition())
    .unwrap()
    .node(input.node())
    .unwrap()
    .profile_selection()
    .unwrap();
  publish_selection_role(store, binding).await;
  let (pending, _) = freeze_profile_selection(store, admission, input, ownership).await;
  store
    .delete_factory_role(management_mutation(DeleteFactoryRole {
      project_id: binding.role.project_id,
      id: binding.role.id,
      expected_current_version: binding.role.version,
      idempotency_key: IdempotencyKey::new("delete-live-agent").unwrap(),
      deleted_at: time(12),
    }))
    .await
    .unwrap();
  assert_eq!(store.factory_run_snapshot(admission.run.id()).await.unwrap(), pending);
  let request = ApplyFactoryRunControl {
    run_id: admission.run.id(),
    expected_version: pending.run.version(),
    idempotency_key: IdempotencyKey::new("choose-deleted-agent").unwrap(),
    requested_at: time(13),
    intent: FactoryRunControlIntent::SelectProfile {
      input_digest: input.digest().unwrap(),
      profile: binding.candidates.values().next().unwrap().profile.reference().clone(),
    },
  };
  assert!(
    store
      .apply_factory_run_control(management_mutation(request))
      .await
      .is_err(),
    "a stale browser must not select a deleted agent"
  );
  assert_eq!(store.factory_run_snapshot(admission.run.id()).await.unwrap(), pending);
}
async fn publish_selection_role<S: FactoryRoleProfileStore>(store: &S, binding: &FlowProfileSelectionBinding) {
  let published = store
    .publish_factory_role_profiles(management_mutation(PublishFactoryRoleProfiles {
      intent: FactoryRoleProfilesIntent {
        project_id: binding.role.project_id,
        role: Some(FactoryRoleIntent {
          id: binding.role.id,
          expected_current_version: None,
          definition: FactoryRoleDefinition {
            avatar: None,
            skills: Default::default(),
            name: FactoryText::new("Arbitrary AI specialist").unwrap(),
            instructions: "Follow Project rules".into(),
            template: None,
            icon: None,
            description: String::new(),
            concurrency_limit: None,
          },
        }),
        // Existing admitted Flows can carry exact profiles without separate catalog publication.
        profiles: vec![],
      },
      idempotency_key: IdempotencyKey::new("seed-live-agent").unwrap(),
      published_at: time(1),
    }))
    .await
    .unwrap();
  assert_eq!(published.role.as_ref().unwrap().reference(), &binding.role);
}
/// A previously admitted Flow cannot freeze a new automatic choice after deletion.
pub async fn verify_factory_role_deletion_blocks_automatic_selection<S: FactoryRoleProfileStore + FactoryRunStore>(
  store: &S,
  admission: &PublishedFactoryAdmission,
  input: &FlowNodeInput,
  ownership: &FactoryClaimOwnership,
) {
  let binding = admission
    .flow
    .closure()
    .definition(input.definition())
    .unwrap()
    .node(input.node())
    .unwrap()
    .profile_selection()
    .unwrap();
  publish_selection_role(store, binding).await;
  let run = admission.run.id();
  let claim = FactoryRunClaimRecord::new(run, ownership.owner().clone(), ownership.claim());
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run,
      expected_version: admission.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run,
        AuditActorKind::Worker,
        None,
        FactoryKey::new("factory.claimed").unwrap(),
        FactoryDigest::from_bytes([1; 32]),
        FactoryKey::new("accepted").unwrap(),
        time(10),
      ),
    })
    .await
    .unwrap();
  let before = store.factory_run_snapshot(run).await.unwrap();
  let mut freeze = factory_flow_contract_transition(&before, &claim, 11);
  freeze.append.flow.data.inputs.push(input.clone());
  let selection = FlowProfileSelectionRecord::select(input, &admission.flow, time(11)).unwrap();
  assert!(selection.candidate().is_some());
  freeze.append.flow.data.profile_selections.push(selection);
  store
    .delete_factory_role(management_mutation(DeleteFactoryRole {
      project_id: binding.role.project_id,
      id: binding.role.id,
      expected_current_version: binding.role.version,
      idempotency_key: IdempotencyKey::new("delete-before-choice").unwrap(),
      deleted_at: time(11),
    }))
    .await
    .unwrap();
  assert!(
    store.commit_factory_run_transition(freeze).await.is_err(),
    "deletion must fence an already prepared automatic choice"
  );
  assert_eq!(store.factory_run_snapshot(run).await.unwrap(), before);
}
#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn memory_deleted_agent_rejects_automatic_selection() {
    crate::test_support::run_ready(
      async {
        let (admission, input, ownership) = factory_profile_selection_contract_fixture(Some("baseline"));
        let store = InMemoryFactoryConfigurationStore::new();
        store.seed_project(admission.work.subject().project_id()).unwrap();
        let audit = management_mutation(()).audit().clone();
        store
          .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
          .unwrap();
        verify_factory_role_deletion_blocks_automatic_selection(&store, &admission, &input, &ownership).await;
      },
      "automatic deleted-agent choice is ready",
    );
  }
  #[test]
  fn memory_factory_role_deletion_contract() {
    let store = InMemoryFactoryConfigurationStore::new();
    let project = ProjectId::generate();
    store.seed_project(project).unwrap();
    store
      .enable_project(
        project,
        FactoryConfigurationChoices::try_new(Default::default()).unwrap(),
      )
      .unwrap();
    let mut future = std::pin::pin!(verify_factory_role_deletion(&store, &store, project));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(future.as_mut().poll(&mut context).is_ready());
  }
  #[test]
  fn memory_deleted_agent_rejects_live_selection() {
    crate::test_support::run_ready(
      async {
        let (admission, input, ownership) = factory_profile_selection_contract_fixture(None);
        let store = InMemoryFactoryConfigurationStore::new();
        store.seed_project(admission.work.subject().project_id()).unwrap();
        let audit = management_mutation(()).audit().clone();
        store
          .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
          .unwrap();
        verify_factory_role_deletion_blocks_live_selection(&store, &admission, &input, &ownership).await;
      },
      "deleted-agent control is ready",
    );
  }
}
