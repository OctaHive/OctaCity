use crate::{testing::InMemoryFactoryConfigurationStore, *};
use octacity_server_domain::{EntityKind, ProjectId};
use octacity_server_factory::ImmutableReference;
#[async_trait::async_trait]
impl ProjectSkillImportStore for InMemoryFactoryConfigurationStore {
  async fn list_project_skill_imports(
    &self,
    project: ProjectId,
    after: Option<ImmutableReference>,
    limit: u16,
  ) -> Result<ProjectSkillImportPage, StoreError> {
    let limit = check_factory_role_catalog_limit(limit, StoreOperation::PublishProjectSkillImport)?;
    let state = self.lock()?;
    if !state.projects.contains(&project) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
    let mut items: Vec<_> = state
      .skill_imports
      .values()
      .filter(|r| r.intent.project_id == project && r.published_at.is_some())
      .map(|r| r.intent.bundle.reference().clone())
      .collect();
    items.sort_by(|a, b| (a.identity(), a.version()).cmp(&(b.identity(), b.version())));
    let total = items.len() as u64;
    items.retain(|reference| {
      after
        .as_ref()
        .is_none_or(|after| (reference.identity(), reference.version()) > (after.identity(), after.version()))
    });
    let more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = more.then(|| items.last().unwrap().clone());
    Ok(FactoryRoleCatalogPage {
      total,
      items,
      next_cursor,
    })
  }
  async fn reserve_project_skill_import(
    &self,
    request: ManagementMutation<PublishProjectSkillImport>,
  ) -> Result<ProjectSkillImportRecord, StoreError> {
    let (request, audit) = request.into_parts();
    let mut state = self.lock()?;
    if !state.projects.contains(&request.intent.project_id) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
    let key = (
      request.intent.project_id,
      ManagementIdempotencyKey::new(audit.security_scope().clone(), request.intent.idempotency_key.clone()),
    );
    if let Some(original) = state.skill_imports.get(&key) {
      original.require_live(&request.intent, request.now)?;
      return Ok(original.clone());
    }
    let record = ProjectSkillImportRecord::reserve(request.intent, request.now)?;
    record_audit(&mut state, &audit, &key, false);
    state.skill_imports.insert(key, record.clone());
    Ok(record)
  }
  async fn publish_project_skill_import(
    &self,
    request: ManagementMutation<PublishProjectSkillImport>,
  ) -> Result<ProjectSkillImportRecord, StoreError> {
    let (request, audit) = request.into_parts();
    let key = (
      request.intent.project_id,
      ManagementIdempotencyKey::new(audit.security_scope().clone(), request.intent.idempotency_key.clone()),
    );
    let mut state = self.lock()?;
    let record = state
      .skill_imports
      .get_mut(&key)
      .ok_or_else(crate::project_skill_import::skill_import_conflict)?;
    record.require_live(&request.intent, request.now)?;
    let fresh = record.published_at.is_none();
    record.published_at.get_or_insert(request.now);
    let outcome = record.clone();
    if fresh {
      record_audit(&mut state, &audit, &key, true);
    }
    Ok(outcome)
  }
  async fn project_skill_import(
    &self,
    project: ProjectId,
    reference: &ImmutableReference,
  ) -> Result<Option<ProjectSkillImportRecord>, StoreError> {
    Ok(
      self
        .lock()?
        .skill_imports
        .values()
        .find(|r| {
          r.intent.project_id == project && r.published_at.is_some() && r.intent.bundle.reference() == reference
        })
        .cloned(),
    )
  }
}

/// One complete reviewed input fixture; its body remains in the byte-store tests.
pub fn skill_import_contract_request(project_id: ProjectId) -> PublishProjectSkillImport {
  use octacity_server_domain::{ArtifactId, ArtifactUploadId, Timestamp};
  use octacity_server_factory::*;
  let artifact = ArtifactId::generate();
  PublishProjectSkillImport {
    intent: ProjectSkillImportIntent {
      project_id,
      bundle: FactoryRoleSkillBundle::new(
        FactoryKey::new("coder").unwrap(),
        FactoryKey::new(format!("import-{artifact}")).unwrap(),
        [(
          FactoryRepositoryPath::new("SKILL.md").unwrap(),
          FactoryArtifactReference::new(artifact, FactoryDigest::content_sha256(b"# skill"), 7).unwrap(),
        )]
        .into(),
      )
      .unwrap(),
      uploads: [(artifact, ArtifactUploadId::generate())].into(),
      idempotency_key: "import-one".parse().unwrap(),
    },
    now: Timestamp::from_unix_millis(10).unwrap(),
  }
}
/// Shared memory/PostgreSQL outcomes for atomic publication, audit and scoped replay.
pub async fn verify_project_skill_import_publication<S: ProjectSkillImportStore, P: testing::MutationEvidenceProbe>(
  store: &S,
  evidence: &P,
  project: ProjectId,
) -> ProjectSkillImportRecord {
  use testing::management_mutation;
  let request = skill_import_contract_request(project);
  let before = evidence.mutation_evidence_counts().await;
  let pending = store
    .reserve_project_skill_import(management_mutation(request.clone()))
    .await
    .unwrap();
  assert!(pending.published_at.is_none());
  assert_eq!(pending.staging_until.unix_millis(), 300_010);
  assert_eq!(
    store.list_project_skill_imports(project, None, 10).await.unwrap().total,
    0
  );
  let original = store
    .publish_project_skill_import(management_mutation(request.clone()))
    .await
    .unwrap();
  assert_eq!(
    store
      .project_skill_import(project, request.intent.bundle.reference())
      .await
      .unwrap(),
    Some(original.clone())
  );
  assert_eq!(
    store.list_project_skill_imports(project, None, 10).await.unwrap().items,
    vec![request.intent.bundle.reference().clone()]
  );
  let after = evidence.mutation_evidence_counts().await;
  assert_eq!(after.audit, before.audit + 2);
  assert_eq!(after.idempotency, before.idempotency + 2);
  assert_eq!(after.outbox, before.outbox + 2);
  assert_eq!(
    store
      .reserve_project_skill_import(management_mutation(request.clone()))
      .await
      .unwrap(),
    original
  );
  assert_eq!(
    store
      .publish_project_skill_import(management_mutation(request.clone()))
      .await
      .unwrap(),
    original
  );
  assert_eq!(evidence.mutation_evidence_counts().await, after);
  let mut conflict = request.clone();
  conflict.intent.bundle = skill_import_contract_request(project).intent.bundle;
  assert!(
    store
      .reserve_project_skill_import(management_mutation(conflict))
      .await
      .is_err()
  );
  let mut other_scope = skill_import_contract_request(project);
  let scope = MutationAuditContext::try_new(
    AuditActor {
      kind: AuditActorKind::UnauthenticatedManagement,
      identity: None,
    },
    ManagementSecurityScope::new("another-accepted-scope").unwrap(),
    "other-scope",
  )
  .unwrap();
  assert!(
    store
      .reserve_project_skill_import(ManagementMutation::new(other_scope.clone(), scope.clone()))
      .await
      .is_ok()
  );
  assert!(
    store
      .publish_project_skill_import(ManagementMutation::new(other_scope.clone(), scope))
      .await
      .is_ok()
  );
  other_scope.intent.idempotency_key = "invalid-upload-map".parse().unwrap();
  other_scope.intent.uploads.clear();
  assert!(
    store
      .reserve_project_skill_import(management_mutation(other_scope))
      .await
      .is_err()
  );
  let mut malformed = skill_import_contract_request(project);
  malformed.intent.idempotency_key = "duplicate-artifact".parse().unwrap();
  let artifact = malformed.intent.bundle.files().values().next().unwrap().clone();
  malformed.intent.bundle = octacity_server_factory::FactoryRoleSkillBundle::new(
    octacity_server_factory::FactoryKey::new("bad-layout").unwrap(),
    octacity_server_factory::FactoryKey::new("v1").unwrap(),
    [
      (
        octacity_server_factory::FactoryRepositoryPath::new("SKILL.md").unwrap(),
        artifact.clone(),
      ),
      (
        octacity_server_factory::FactoryRepositoryPath::new("support.txt").unwrap(),
        artifact,
      ),
    ]
    .into(),
  )
  .unwrap();
  malformed.intent.uploads.insert(
    octacity_server_domain::ArtifactId::generate(),
    octacity_server_domain::ArtifactUploadId::generate(),
  );
  assert!(
    store
      .reserve_project_skill_import(management_mutation(malformed))
      .await
      .is_err()
  );
  original
}
#[test]
fn project_skill_import_memory_matches_shared_publication_contract() {
  let store = InMemoryFactoryConfigurationStore::default();
  let project = ProjectId::generate();
  store.seed_project(project).unwrap();
  let mut future = std::pin::pin!(verify_project_skill_import_publication(&store, &store, project));
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}

fn record_audit(
  state: &mut crate::factory_configuration_testing::FactoryConfigurationMemoryState,
  audit: &MutationAuditContext,
  key: &(ProjectId, ManagementIdempotencyKey),
  published: bool,
) {
  state.evidence.insert(format!(
    "project-skill-import:{published}:{}:{}:{}",
    key.0,
    key.1.security_scope().as_str(),
    key.1.caller_key()
  ));
  state.audit.insert(testing::recorded_management_audit(
    audit,
    if published {
      StoreOperation::PublishProjectSkillImport
    } else {
      StoreOperation::ReserveProjectSkillImport
    },
    EntityKind::Project,
    key.0,
  ));
}
