//! Shared publication contract for memory and PostgreSQL adapters.
use crate::{testing::*, *};
use octacity_server_domain::{BuildConfigurationId, BuildConfigurationVersion, ProjectId, Timestamp};
use octacity_server_factory::*;

/// Complete provider-neutral definition; installed compatibility is qualified separately.
pub fn role_profile_contract_definition(project: ProjectId) -> FactoryRoleProfileDefinition {
  let exact = |name| {
    ImmutableReference::new(
      FactoryKey::new(name).unwrap(),
      FactoryKey::new("v1").unwrap(),
      FactoryDigest::from_bytes([7; 32]),
    )
  };
  FactoryRoleProfileDefinition {
    template: None,
    execution: FactoryRoleProfileExecution {
      environment: [
        (
          "LANGUAGE".into(),
          FactoryRoleEnvironmentValue::Public { value: "ru".into() },
        ),
        (
          "MODEL_AUTH".into(),
          FactoryRoleEnvironmentValue::Secret {
            reference: octacity_server_secrets::LogicalSecretReference::new(
              octacity_server_secrets::SecretProviderId::new("operator-secrets").unwrap(),
              "projects/model",
            )
            .unwrap(),
          },
        ),
      ]
      .into(),
      cli_arguments: vec!["--output".into(), "structured".into()],
      mcp_servers: [(
        FactoryKey::new("tools").unwrap(),
        FactoryRoleMcpBinding {
          configuration: exact("mcp.tools"),
          integration: Some(FactoryKey::new("tracker").unwrap()),
        },
      )]
      .into(),
      integrations: [(
        FactoryKey::new("tracker").unwrap(),
        FactoryRoleIntegrationBinding {
          project_id: project,
          connection_id: octacity_server_domain::IntegrationId::generate(),
          connection_version: octacity_server_domain::IntegrationVersion::INITIAL,
          access_version: octacity_server_domain::IntegrationVersion::INITIAL,
          resource: exact("tracker.resource"),
          capability: exact("tracker.read"),
        },
      )]
      .into(),
    },
    name: FactoryText::new("Custom specialist").unwrap(),
    instructions: FactoryInstructions::new(vec!["Use selected skills.".into(), "Keep Project rules.".into()]).unwrap(),
    toolchain: FactoryTaskToolchain {
      plugin: exact("model.plugin"),
      executable: exact("model.harness"),
      model: exact("open.model"),
    },
    model_parameters: [(
      FactoryKey::new("reasoning_effort").unwrap(),
      serde_json::json!("medium"),
    )]
    .into(),
    skills: [(FactoryKey::new("testing").unwrap(), exact("skills.testing"))].into(),
    context_requirements: Default::default(),
    build_configuration: BuildConfigurationRef::new(
      BuildConfigurationId::generate(),
      BuildConfigurationVersion::INITIAL,
      project,
      FactoryDigest::from_bytes([8; 32]),
    ),
    credential_purpose: FactoryCredentialPurpose::Model,
    permissions: FactoryPermissionSet::deny_all(),
    budget: BudgetLimit::new(1, 1000, 200, 100, 1024).unwrap(),
  }
}

/// Frozen role plus two independent profiles, suitable for later template copying.
pub fn role_profile_contract_intent(project: ProjectId) -> FactoryRoleProfilesIntent {
  let role = FactoryRole::publish(
    FactoryRoleId::generate(),
    project,
    FactoryRoleDefinition {
      template: None,
      icon: Some(FactoryKey::new("bot").unwrap()),
      description: "Custom Project reviewer".into(),
      concurrency_limit: std::num::NonZeroU32::new(2),
      name: FactoryText::new("Custom reviewer").unwrap(),
      instructions: "# Review\nFollow Project rules.\n".into(),
    },
  )
  .unwrap();
  FactoryRoleProfilesIntent {
    project_id: project,
    role: Some(FactoryRoleIntent {
      id: role.reference().id,
      expected_current_version: None,
      definition: role.definition().clone(),
    }),
    profiles: (0..2)
      .map(|_| FactoryRoleProfileIntent {
        id: FactoryRoleProfileId::generate(),
        expected_current_version: None,
        role: role.reference().clone(),
        definition: role_profile_contract_definition(project),
      })
      .collect(),
  }
}
fn publication(intent: FactoryRoleProfilesIntent, key: &str, time: i64) -> PublishFactoryRoleProfiles {
  PublishFactoryRoleProfiles {
    intent,
    idempotency_key: IdempotencyKey::new(key).unwrap(),
    published_at: Timestamp::from_unix_millis(time).unwrap(),
  }
}

/// Verifies original receipts, exact retained reads and scoped conflicting reuse.
/// The supplied adapter must contain the Project and no role/profile history.
/// The test-only evidence probe must observe that same adapter backend.
pub async fn verify_factory_role_profile_publication<
  S: FactoryRoleProfileStore,
  P: MutationEvidenceProbe + ManagementAuditProbe,
>(
  store: &S,
  evidence: &P,
  project: ProjectId,
) {
  let intent = role_profile_contract_intent(project);
  let before = evidence.mutation_evidence_counts().await;
  let original = store
    .publish_factory_role_profiles(management_mutation(publication(intent.clone(), "role-copy", 1)))
    .await
    .unwrap();
  assert_eq!(original.disposition, MutationDisposition::Applied);
  assert_eq!(original.published_at, Timestamp::from_unix_millis(1).unwrap());
  let role = original.role.as_ref().unwrap();
  assert_eq!(
    role.definition().instructions.entries(),
    ["# Review\nFollow Project rules.\n"]
  );
  assert_eq!(original.profiles.len(), 2);
  for profile in &original.profiles {
    assert_eq!(profile.role(), role.reference());
    assert_eq!(
      store
        .factory_role_profile(project, profile.reference().id, None)
        .await
        .unwrap(),
      *profile
    );
  }
  assert_eq!(
    store.factory_role(project, role.reference().id, None).await.unwrap(),
    *role
  );
  let after = evidence.mutation_evidence_counts().await;
  assert_eq!(after.idempotency, before.idempotency + 1);
  assert_eq!(after.audit, before.audit + 1);
  assert_eq!(after.outbox, before.outbox + 1);
  let replay = store
    .publish_factory_role_profiles(management_mutation_with_request(
      publication(intent.clone(), "role-copy", 9),
      "lost-response-retry",
    ))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.role, original.role);
  assert_eq!(replay.profiles, original.profiles);
  assert_eq!(replay.published_at, original.published_at);
  let probe = store
    .replay_factory_role_profiles(management_mutation(ReplayFactoryRoleProfiles {
      intent: intent.clone(),
      idempotency_key: IdempotencyKey::new("role-copy").unwrap(),
    }))
    .await
    .unwrap()
    .unwrap();
  assert_eq!(probe, replay);
  assert_eq!(role.definition().icon.as_ref().unwrap().as_str(), "bot");
  assert_eq!(role.definition().description, "Custom Project reviewer");
  assert_eq!(role.definition().concurrency_limit.unwrap().get(), 2);
  assert_eq!(
    original.profiles[0].definition().instructions.entries(),
    ["Use selected skills.", "Keep Project rules."]
  );
  assert_eq!(
    original.profiles[0].definition().execution.cli_arguments,
    ["--output", "structured"]
  );
  let mut changed_capacity = intent.clone();
  changed_capacity.role.as_mut().unwrap().definition.concurrency_limit = std::num::NonZeroU32::new(3);
  let mut changed_environment = intent.clone();
  changed_environment.profiles[0].definition.execution.environment.insert(
    "LANGUAGE".into(),
    FactoryRoleEnvironmentValue::Public { value: "en".into() },
  );
  for changed in [changed_capacity, changed_environment] {
    assert!(matches!(
      store
        .publish_factory_role_profiles(management_mutation(publication(changed, "role-copy", 10)))
        .await,
      Err(FactoryRoleProfilePublicationError::Store(StoreError::Conflict { .. }))
    ));
  }
  let mut changed = intent;
  changed.role.as_mut().unwrap().definition.instructions = "Different instruction".into();
  assert!(matches!(
    store
      .publish_factory_role_profiles(management_mutation(publication(changed.clone(), "role-copy", 10)))
      .await,
    Err(FactoryRoleProfilePublicationError::Store(StoreError::Conflict { .. }))
  ));
  assert!(matches!(
    store
      .replay_factory_role_profiles(management_mutation(ReplayFactoryRoleProfiles {
        intent: changed,
        idempotency_key: IdempotencyKey::new("role-copy").unwrap()
      }))
      .await,
    Err(StoreError::Conflict { .. })
  ));
  assert_eq!(evidence.mutation_evidence_counts().await, after);
  let audits = evidence.management_audit_facts().await;
  let publication_audits = audits
    .iter()
    .filter(|audit| {
      audit.operation == StoreOperation::PublishFactoryRoleProfiles && audit.target_identity == project.to_string()
    })
    .collect::<Vec<_>>();
  assert_eq!(publication_audits.len(), 1);
  assert_eq!(publication_audits[0].request_identity, "contract-request");
  assert_eq!(
    publication_audits[0].target_kind,
    octacity_server_domain::EntityKind::Project
  );
  assert_eq!(
    publication_audits[0].actor_kind,
    AuditActorKind::UnauthenticatedManagement
  );
  assert_eq!(publication_audits[0].actor_identity, None);
  assert!(!format!("{audits:?}").contains("Follow Project rules"));
  // Caller keys are partitioned by the existing accepted management security scope.
  let scoped_intent = role_profile_contract_intent(project);
  let scoped_request = publication(scoped_intent, "role-copy", 11);
  let scoped_audit = MutationAuditContext::try_new(
    AuditActor {
      kind: AuditActorKind::UnauthenticatedManagement,
      identity: None,
    },
    ManagementSecurityScope::new("other-management-scope").unwrap(),
    "other-scope-request",
  )
  .unwrap();
  let scoped = store
    .publish_factory_role_profiles(ManagementMutation::new(scoped_request, scoped_audit))
    .await
    .unwrap();
  assert_eq!(scoped.disposition, MutationDisposition::Applied);
  assert_ne!(scoped.role.as_ref().unwrap().reference().id, role.reference().id);
  assert_eq!(
    evidence.mutation_evidence_counts().await.idempotency,
    after.idempotency + 1
  );
}

/// Verifies independent current heads, stale preconditions and replay after replacement.
/// The supplied adapter must contain the Project and no role/profile history.
/// The test-only evidence probe must observe that same adapter backend.
pub async fn verify_factory_role_profile_replacement<S: FactoryRoleProfileStore, P: MutationEvidenceProbe>(
  store: &S,
  evidence: &P,
  project: ProjectId,
) {
  let initial = role_profile_contract_intent(project);
  let original = store
    .publish_factory_role_profiles(management_mutation(publication(initial.clone(), "initial", 1)))
    .await
    .unwrap();
  let old_role = original.role.as_ref().unwrap();
  let mut change = initial.clone();
  change.profiles.clear();
  change.role.as_mut().unwrap().expected_current_version = Some(FactoryRoleVersion::INITIAL);
  change.role.as_mut().unwrap().definition.instructions = "Revised shared rules".into();
  let updated = store
    .publish_factory_role_profiles(management_mutation_with_request(
      publication(change.clone(), "rename", 2),
      "rename-request",
    ))
    .await
    .unwrap();
  let role = updated.role.as_ref().unwrap();
  assert_eq!(role.reference().version.get(), 2);
  assert_eq!(role.definition().instructions.entries(), ["Revised shared rules"]);
  assert_eq!(
    store
      .factory_role(project, role.reference().id, Some(FactoryRoleVersion::INITIAL))
      .await
      .unwrap(),
    *old_role
  );
  let old_profile = &original.profiles[0];
  assert_eq!(
    store
      .factory_role_profile(project, old_profile.reference().id, None)
      .await
      .unwrap()
      .role(),
    old_role.reference()
  );
  let before = evidence.mutation_evidence_counts().await;
  assert!(matches!(
    store
      .publish_factory_role_profiles(management_mutation(publication(change, "stale", 3)))
      .await,
    Err(FactoryRoleProfilePublicationError::PreconditionFailed)
  ));
  assert_eq!(evidence.mutation_evidence_counts().await, before);
  let mut adoption = FactoryRoleProfilesIntent {
    project_id: project,
    role: None,
    profiles: vec![initial.profiles[0].clone()],
  };
  adoption.profiles[0].expected_current_version = Some(FactoryRoleProfileVersion::INITIAL);
  adoption.profiles[0].role = role.reference().clone();
  adoption.profiles[0].definition.instructions = "Revised profile".into();
  let adopted = store
    .publish_factory_role_profiles(management_mutation_with_request(
      publication(adoption.clone(), "adopt", 4),
      "adoption-request",
    ))
    .await
    .unwrap();
  assert_eq!(adopted.profiles[0].reference().version.get(), 2);
  assert_eq!(adopted.profiles[0].role(), role.reference());
  assert_eq!(
    store
      .factory_role_profile(
        project,
        old_profile.reference().id,
        Some(FactoryRoleProfileVersion::INITIAL)
      )
      .await
      .unwrap(),
    *old_profile
  );
  assert_eq!(
    store
      .factory_role_profile(project, original.profiles[1].reference().id, None)
      .await
      .unwrap(),
    original.profiles[1]
  );
  assert!(matches!(
    store
      .publish_factory_role_profiles(management_mutation(publication(adoption, "stale-profile", 5)))
      .await,
    Err(FactoryRoleProfilePublicationError::PreconditionFailed)
  ));
  let replay = store
    .publish_factory_role_profiles(management_mutation(publication(initial, "initial", 99)))
    .await
    .unwrap();
  assert_eq!(replay.role, original.role);
  assert_eq!(replay.profiles, original.profiles);
  let probe = store
    .replay_factory_role_profiles(management_mutation(ReplayFactoryRoleProfiles {
      intent: role_profile_contract_intent(project),
      idempotency_key: IdempotencyKey::new("unused").unwrap(),
    }))
    .await
    .unwrap();
  assert!(probe.is_none());
}

/// Verifies bounded all-or-nothing publication and unchanged evidence on rejection.
/// The supplied adapter must contain both Projects and no role/profile history.
pub async fn verify_factory_role_profile_atomicity<S: FactoryRoleProfileStore, P: MutationEvidenceProbe>(
  store: &S,
  evidence: &P,
  project: ProjectId,
  foreign: ProjectId,
) {
  let before = evidence.mutation_evidence_counts().await;
  let intent = role_profile_contract_intent(project);
  let role_id = intent.role.as_ref().unwrap().id;
  // A template copy must not retain even its role or first valid profile when a sibling fails.
  let mut invalid = intent.clone();
  invalid.profiles[1].definition.instructions = "bad\0instruction".into();
  assert!(
    store
      .publish_factory_role_profiles(management_mutation(publication(invalid, "atomic-copy", 1)))
      .await
      .is_err()
  );
  assert!(matches!(
    store.factory_role(project, role_id, None).await,
    Err(StoreError::NotFound { .. })
  ));
  for profile in &intent.profiles {
    assert!(matches!(
      store.factory_role_profile(project, profile.id, None).await,
      Err(StoreError::NotFound { .. })
    ));
  }
  assert_eq!(evidence.mutation_evidence_counts().await, before);
  // Failure reserves no idempotency key; correcting the same intent can commit.
  let original = store
    .publish_factory_role_profiles(management_mutation(publication(intent.clone(), "atomic-copy", 2)))
    .await
    .unwrap();
  let before = evidence.mutation_evidence_counts().await;
  let empty = FactoryRoleProfilesIntent {
    project_id: project,
    role: None,
    profiles: vec![],
  };
  assert!(matches!(
    store
      .publish_factory_role_profiles(management_mutation(publication(empty, "empty", 3)))
      .await,
    Err(FactoryRoleProfilePublicationError::Store(
      StoreError::InvalidInput { .. }
    ))
  ));
  let mut duplicate = role_profile_contract_intent(project);
  duplicate.profiles[1].id = duplicate.profiles[0].id;
  assert!(
    store
      .publish_factory_role_profiles(management_mutation(publication(duplicate.clone(), "duplicate", 3)))
      .await
      .is_err()
  );
  assert!(
    store
      .factory_role(project, duplicate.role.unwrap().id, None)
      .await
      .is_err()
  );
  let mut oversized = role_profile_contract_intent(project);
  oversized.profiles = (0..65)
    .map(|_| {
      let mut profile = oversized.profiles[0].clone();
      profile.id = FactoryRoleProfileId::generate();
      profile
    })
    .collect();
  assert!(
    store
      .publish_factory_role_profiles(management_mutation(publication(oversized, "oversized", 3)))
      .await
      .is_err()
  );
  let mut update = intent.clone();
  update.role.as_mut().unwrap().expected_current_version = Some(FactoryRoleVersion::INITIAL);
  update.role.as_mut().unwrap().definition.instructions = "Not committed".into();
  for profile in &mut update.profiles {
    profile.expected_current_version = Some(FactoryRoleProfileVersion::INITIAL);
  }
  update.profiles[1].expected_current_version = Some(FactoryRoleProfileVersion::new(2).unwrap());
  assert!(matches!(
    store
      .publish_factory_role_profiles(management_mutation(publication(update, "stale-copy", 4)))
      .await,
    Err(FactoryRoleProfilePublicationError::PreconditionFailed)
  ));
  assert_eq!(
    store.factory_role(project, role_id, None).await.unwrap(),
    original.role.unwrap()
  );
  for profile in &original.profiles {
    assert_eq!(
      store
        .factory_role_profile(project, profile.reference().id, None)
        .await
        .unwrap(),
      *profile
    );
  }
  assert!(matches!(
    store.factory_role(foreign, role_id, None).await,
    Err(StoreError::NotFound { .. })
  ));
  assert!(matches!(
    store
      .factory_role_profile(foreign, original.profiles[0].reference().id, None)
      .await,
    Err(StoreError::NotFound { .. })
  ));
  let mut foreign_pin = FactoryRoleProfilesIntent {
    project_id: foreign,
    role: None,
    profiles: vec![intent.profiles[0].clone()],
  };
  foreign_pin.profiles[0].id = FactoryRoleProfileId::generate();
  foreign_pin.profiles[0].definition = role_profile_contract_definition(foreign);
  assert!(
    store
      .publish_factory_role_profiles(management_mutation(publication(foreign_pin, "foreign-role", 5)))
      .await
      .is_err()
  );
  let mut substituted = FactoryRoleProfilesIntent {
    project_id: project,
    role: None,
    profiles: vec![intent.profiles[0].clone()],
  };
  substituted.profiles[0].id = FactoryRoleProfileId::generate();
  substituted.profiles[0].role.digest = FactoryDigest::from_bytes([99; 32]);
  assert!(
    store
      .publish_factory_role_profiles(management_mutation(publication(substituted, "wrong-digest", 5)))
      .await
      .is_err()
  );
  assert_eq!(evidence.mutation_evidence_counts().await, before);
}

/// Verifies explicit template copying through the ordinary transaction. Both
/// Projects must exist, and the probe must observe the same adapter backend.
pub async fn verify_factory_role_template_copy<S: FactoryRoleProfileStore, P: MutationEvidenceProbe>(
  store: &S,
  evidence: &P,
  project: ProjectId,
  foreign: ProjectId,
) {
  let template = factory_role_templates().unwrap().remove(0);
  let origin = template.reference().unwrap();
  let copy = |project| {
    let profiles = template
      .profiles
      .iter()
      .map(|(key, seed)| {
        let mut definition = role_profile_contract_definition(project);
        definition.name = seed.name.clone();
        definition.instructions = seed.instructions.clone();
        (
          key.clone(),
          FactoryRoleTemplateBinding {
            id: FactoryRoleProfileId::generate(),
            definition,
          },
        )
      })
      .collect();
    FactoryRoleProfilesIntent::from(
      template
        .instantiate(project, FactoryRoleId::generate(), template.role.clone(), profiles)
        .unwrap(),
    )
  };
  let before = evidence.mutation_evidence_counts().await;
  let intent = copy(project);
  let other_intent = copy(foreign);
  assert_eq!(
    evidence.mutation_evidence_counts().await,
    before,
    "catalog/copying must not persist definitions implicitly"
  );
  let original = store
    .publish_factory_role_profiles(management_mutation(publication(intent.clone(), "template-copy", 1)))
    .await
    .unwrap();
  let other = store
    .publish_factory_role_profiles(management_mutation_with_request(
      publication(other_intent, "template-other", 1),
      "template-other",
    ))
    .await
    .unwrap();
  let role = original.role.as_ref().unwrap();
  assert_eq!(role.definition().name.as_str(), "coder");
  assert_eq!(role.definition().template.as_ref(), Some(&origin));
  assert_eq!(
    original
      .profiles
      .iter()
      .map(|profile| profile.definition().name.as_str())
      .collect::<Vec<_>>(),
    ["coder", "coder-pro"]
  );
  let mut definition = role.definition().clone();
  definition.name = FactoryText::new("Project specialist").unwrap();
  definition.instructions = "Independent Project rules".into();
  let next = role
    .replace(FactoryRoleVersion::new(2).unwrap(), definition.clone())
    .unwrap();
  let mut profile = original.profiles[1].definition().clone();
  profile.name = FactoryText::new("Advanced accessibility").unwrap();
  profile.instructions = "Independent profile instructions".into();
  let mut extra = role_profile_contract_definition(project);
  extra.name = FactoryText::new("Security specialist").unwrap();
  let changed = store
    .publish_factory_role_profiles(management_mutation_with_request(
      publication(
        FactoryRoleProfilesIntent {
          project_id: project,
          role: Some(FactoryRoleIntent {
            id: role.reference().id,
            expected_current_version: Some(FactoryRoleVersion::INITIAL),
            definition,
          }),
          profiles: vec![
            FactoryRoleProfileIntent {
              id: original.profiles[1].reference().id,
              expected_current_version: Some(FactoryRoleProfileVersion::INITIAL),
              role: next.reference().clone(),
              definition: profile,
            },
            FactoryRoleProfileIntent {
              id: FactoryRoleProfileId::generate(),
              expected_current_version: None,
              role: next.reference().clone(),
              definition: extra,
            },
          ],
        },
        "template-edit",
        2,
      ),
      "template-edit",
    ))
    .await
    .unwrap();
  assert_eq!(
    changed.role.as_ref().unwrap().definition().name.as_str(),
    "Project specialist"
  );
  assert_eq!(changed.profiles[0].definition().name.as_str(), "Advanced accessibility");
  assert_eq!(changed.profiles[0].definition().template.as_ref(), Some(&origin));
  assert_eq!(changed.profiles[1].definition().name.as_str(), "Security specialist");
  assert_eq!(
    changed.profiles[1].definition().template,
    None,
    "custom profiles need no template origin"
  );
  assert_eq!(
    store
      .factory_role_profile(project, original.profiles[0].reference().id, None)
      .await
      .unwrap(),
    original.profiles[0]
  );
  assert_eq!(
    store
      .factory_role_profile(
        project,
        original.profiles[1].reference().id,
        Some(FactoryRoleProfileVersion::INITIAL)
      )
      .await
      .unwrap(),
    original.profiles[1]
  );
  let other_role = other.role.as_ref().unwrap();
  assert_eq!(
    store
      .factory_role(foreign, other_role.reference().id, None)
      .await
      .unwrap(),
    *other_role
  );
  for profile in &other.profiles {
    assert_eq!(
      store
        .factory_role_profile(foreign, profile.reference().id, None)
        .await
        .unwrap(),
      *profile
    );
  }
  let replay = store
    .publish_factory_role_profiles(management_mutation(publication(intent, "template-copy", 9)))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.role, original.role);
  assert_eq!(replay.profiles, original.profiles);
  assert_eq!(replay.published_at, original.published_at);
  let after = evidence.mutation_evidence_counts().await;
  assert_eq!(
    (
      after.idempotency - before.idempotency,
      after.audit - before.audit,
      after.outbox - before.outbox
    ),
    (3, 3, 3)
  );
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn memory_factory_role_template_copy_contract() {
    let store = InMemoryFactoryConfigurationStore::new();
    let project = ProjectId::generate();
    let foreign = ProjectId::generate();
    store.seed_project(project).unwrap();
    store.seed_project(foreign).unwrap();
    let mut future = std::pin::pin!(verify_factory_role_template_copy(&store, &store, project, foreign));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(future.as_mut().poll(&mut context).is_ready());
  }
  #[test]
  fn memory_factory_role_profile_atomicity_contract() {
    let store = InMemoryFactoryConfigurationStore::new();
    let project = ProjectId::generate();
    let foreign = ProjectId::generate();
    store.seed_project(project).unwrap();
    store.seed_project(foreign).unwrap();
    let mut future = std::pin::pin!(verify_factory_role_profile_atomicity(&store, &store, project, foreign));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(future.as_mut().poll(&mut context).is_ready());
  }
  #[test]
  fn memory_factory_role_profile_replacement_contract() {
    let store = InMemoryFactoryConfigurationStore::new();
    let project = ProjectId::generate();
    store.seed_project(project).unwrap();
    let mut future = std::pin::pin!(verify_factory_role_profile_replacement(&store, &store, project));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(future.as_mut().poll(&mut context).is_ready());
  }
  #[test]
  fn memory_factory_role_profile_publication_contract() {
    let store = InMemoryFactoryConfigurationStore::new();
    let project = ProjectId::generate();
    store.seed_project(project).unwrap();
    let mut future = std::pin::pin!(verify_factory_role_profile_publication(&store, &store, project));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(future.as_mut().poll(&mut context).is_ready());
  }
}
