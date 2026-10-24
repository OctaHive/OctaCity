use crate::*;
use octacity_server_domain::{ArtifactId, ProjectId, Timestamp};
use octacity_server_factory::{FactoryDigest, FactoryKey};
use octacity_server_store::testing::InMemoryFactoryConfigurationStore;
use std::{
  collections::BTreeMap,
  sync::{Arc, atomic::Ordering},
};

pub(crate) use crate::factory_test_support::InMemoryArtifactBytes as Bytes;
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(Timestamp::from_unix_millis(20).unwrap())
  }
}
fn command(project_id: ProjectId) -> ImportProjectSkill {
  ImportProjectSkill {
    project_id,
    identity: FactoryKey::new("custom-coder").unwrap(),
    idempotency_key: "import-one".parse().unwrap(),
    entries: vec![
      ReviewedSkillEntry::File {
        path: "SKILL.md".into(),
        sha256: FactoryDigest::content_sha256(b"# Private skill\n"),
        bytes: b"# Private skill\n".to_vec(),
      },
      ReviewedSkillEntry::File {
        path: "examples/sample.txt".into(),
        sha256: FactoryDigest::content_sha256(b"retained example"),
        bytes: b"retained example".to_vec(),
      },
    ],
  }
}
#[test]
fn skill_import_retains_complete_tree_replays_and_materializes_exact_protected_inputs() {
  run(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::default());
    store.seed_project(project).unwrap();
    let bytes = Arc::new(Bytes::default());
    let import = ProjectSkillImport::new(
      store.clone(),
      bytes.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    );
    let context = crate::factory_admission_tests::context();
    let original = import.import(&context, command(project)).await.unwrap();
    assert_eq!(original.intent.bundle.files().len(), 2);
    let replay = import.import(&context, command(project)).await.unwrap();
    assert_eq!(original, replay);
    let manifest = import
      .materialize(
        project,
        BTreeMap::from([(
          octacity_server_factory::FactoryRepositoryPath::new("skills/coder").unwrap(),
          original.intent.bundle.clone(),
        )]),
      )
      .await
      .unwrap();
    assert_eq!(manifest.inputs.len(), 2);
    assert!(
      manifest
        .inputs
        .iter()
        .any(|file| file.destination == "/octacity/protected/skills/coder/SKILL.md"
          && file.sha256 == FactoryDigest::content_sha256(b"# Private skill\n").to_string())
    );
    assert!(
      manifest
        .inputs
        .iter()
        .any(|file| file.destination == "/octacity/protected/skills/coder/examples/sample.txt")
    );
    assert!(
      import
        .materialize(
          ProjectId::generate(),
          BTreeMap::from([(
            octacity_server_factory::FactoryRepositoryPath::new("skills/coder").unwrap(),
            original.intent.bundle
          )])
        )
        .await
        .is_err()
    );
  });
}

fn file(path: &str, content: &[u8]) -> ReviewedSkillEntry {
  ReviewedSkillEntry::File {
    path: path.into(),
    sha256: FactoryDigest::content_sha256(content),
    bytes: content.to_vec(),
  }
}
#[test]
fn skill_import_rejects_unreviewed_or_unsafe_trees_before_any_object_write() {
  run(async {
    for entries in [
      vec![file("examples/only.txt", b"missing root")],
      vec![file("SKILL.md", b"root"), file("../outside", b"escape")],
      vec![file("SKILL.md", b"root"), file("/absolute", b"escape")],
      vec![file("SKILL.md", b"root"), file("examples\\outside", b"escape")],
      vec![file("SKILL.md", b"root"), file("SKILL.md", b"duplicate")],
      vec![
        file("SKILL.md", b"root"),
        file("examples", b"file"),
        file("examples/child", b"conflict"),
      ],
      vec![
        file("SKILL.md", b"root"),
        ReviewedSkillEntry::Symlink {
          path: "examples/link".into(),
          target: "../../secret".into(),
        },
      ],
      vec![ReviewedSkillEntry::File {
        path: "SKILL.md".into(),
        sha256: FactoryDigest::content_sha256(b"reviewed"),
        bytes: b"substituted".to_vec(),
      }],
    ] {
      let project = ProjectId::generate();
      let store = Arc::new(InMemoryFactoryConfigurationStore::default());
      store.seed_project(project).unwrap();
      let bytes = Arc::new(Bytes::default());
      let import = ProjectSkillImport::new(
        store,
        bytes.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
        Arc::new(Clock),
      );
      let mut request = command(project);
      request.entries = entries;
      assert!(
        import
          .import(&crate::factory_admission_tests::context(), request)
          .await
          .is_err()
      );
      assert!(
        bytes.0.lock().unwrap().is_empty(),
        "invalid review must not write private objects"
      );
    }
  });
}
struct AuthorOnly;
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for AuthorOnly {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if action == ManagementAction::Administer && resource.kind() == ManagementResourceKind::Project {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    } else {
      Err(ManagementAuthorizationDenial::forbidden())
    }
  }
}
#[test]
fn skill_import_uses_project_authoring_authority_without_factory_execution_permission() {
  run(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::default());
    store.seed_project(project).unwrap();
    let import = ProjectSkillImport::new(store, Arc::new(Bytes::default()), Arc::new(AuthorOnly), Arc::new(Clock));
    assert!(
      import
        .import(&crate::factory_admission_tests::context(), command(project))
        .await
        .is_ok()
    );
  });
}

#[test]
fn skill_import_recovers_partial_writes_without_exposing_partial_catalog_entries() {
  run(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::default());
    store.seed_project(project).unwrap();
    let bytes = Arc::new(Bytes::default());
    bytes.1.store(true, Ordering::SeqCst);
    let import = ProjectSkillImport::new(
      store.clone(),
      bytes.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    );
    let context = crate::factory_admission_tests::context();
    assert!(import.import(&context, command(project)).await.is_err());
    assert_eq!(import.catalog(&context, project, None, 10).await.unwrap().total, 0);
    let original = import.import(&context, command(project)).await.unwrap();
    let page = import.catalog(&context, project, None, 10).await.unwrap();
    assert_eq!(page.items, vec![original.intent.bundle.reference().clone()]);
    assert_eq!(page.total, 1);
    let published_objects = bytes.0.lock().unwrap().clone();
    let mut changed = command(project);
    changed.entries = vec![file("SKILL.md", b"different")];
    assert!(import.import(&context, changed.clone()).await.is_err());
    assert_eq!(*bytes.0.lock().unwrap(), published_objects);
    changed.idempotency_key = "explicit-reimport".parse().unwrap();
    let replacement = import.import(&context, changed).await.unwrap();
    assert_ne!(
      replacement.intent.bundle.reference(),
      original.intent.bundle.reference()
    );
    assert_eq!(import.catalog(&context, project, None, 1).await.unwrap().total, 2);
    let page = import.catalog(&context, project, None, 1).await.unwrap();
    let next = import.catalog(&context, project, page.next_cursor, 1).await.unwrap();
    assert_eq!(next.items.len(), 1);
    assert_ne!(next.items, page.items);
    assert!(import.catalog(&context, ProjectId::generate(), None, 10).await.is_err());
    let selected = BTreeMap::from([(
      octacity_server_factory::FactoryRepositoryPath::new("skills/coder").unwrap(),
      original.intent.bundle.clone(),
    )]);
    // Only retained exact bytes are eligible; there is no ambient/global fallback.
    let root = original
      .intent
      .bundle
      .files()
      .iter()
      .find(|(path, _)| path.as_str() == "SKILL.md")
      .unwrap()
      .1
      .artifact_id();
    bytes.0.lock().unwrap().remove(&root);
    assert!(import.materialize(project, selected.clone()).await.is_err());
    bytes.0.lock().unwrap().insert(root, b"substituted bytes".to_vec());
    assert!(import.materialize(project, selected).await.is_err());
  });
}
fn run(future: impl std::future::Future<Output = ()>) {
  let mut future = std::pin::pin!(future);
  assert!(
    future
      .as_mut()
      .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
      .is_ready()
  );
}

#[test]
fn skill_import_materialization_bounds_the_complete_selected_set_before_reading_bytes() {
  use octacity_server_domain::ArtifactUploadId;
  use octacity_server_factory::{FactoryArtifactReference, FactoryRepositoryPath, FactoryRoleSkillBundle};
  use octacity_server_store::{
    ProjectSkillImportStore,
    testing::{management_mutation, skill_import_contract_request},
  };
  run(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::default());
    store.seed_project(project).unwrap();
    let bytes = Arc::new(Bytes::default());
    let import = ProjectSkillImport::new(
      store.clone(),
      bytes.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    );
    let mut selected = BTreeMap::new();
    for name in ["first", "second"] {
      let mut request = skill_import_contract_request(project);
      request.intent.idempotency_key = name.parse().unwrap();
      let files: BTreeMap<_, _> = ["SKILL.md", "data/one", "data/two"]
        .into_iter()
        .map(|path| {
          (
            FactoryRepositoryPath::new(path).unwrap(),
            FactoryArtifactReference::new(
              ArtifactId::generate(),
              FactoryDigest::from_bytes([1; 32]),
              16 * 1024 * 1024,
            )
            .unwrap(),
          )
        })
        .collect();
      request.intent.uploads = files
        .values()
        .map(|file| (file.artifact_id(), ArtifactUploadId::generate()))
        .collect();
      request.intent.bundle =
        FactoryRoleSkillBundle::new(FactoryKey::new(name).unwrap(), FactoryKey::new("v1").unwrap(), files).unwrap();
      store
        .reserve_project_skill_import(management_mutation(request.clone()))
        .await
        .unwrap();
      let record = store
        .publish_project_skill_import(management_mutation(request))
        .await
        .unwrap();
      selected.insert(FactoryRepositoryPath::new(name).unwrap(), record.intent.bundle);
    }
    assert_eq!(
      import
        .materialize(project, selected)
        .await
        .unwrap_err()
        .classification(),
      ApplicationFailure::Invalid
    );
    assert_eq!(bytes.2.load(Ordering::SeqCst), 0);
    // Destination conflicts are rejected before reading either retained tree.
    let original = import
      .import(&crate::factory_admission_tests::context(), command(project))
      .await
      .unwrap();
    let mut nested = command(project);
    nested.idempotency_key = "nested".parse().unwrap();
    let nested = import
      .import(&crate::factory_admission_tests::context(), nested)
      .await
      .unwrap();
    let reads = bytes.2.load(Ordering::SeqCst);
    let destinations = BTreeMap::from([
      (FactoryRepositoryPath::new("skills").unwrap(), original.intent.bundle),
      (
        FactoryRepositoryPath::new("skills/SKILL.md").unwrap(),
        nested.intent.bundle,
      ),
    ]);
    assert_eq!(
      import
        .materialize(project, destinations)
        .await
        .unwrap_err()
        .classification(),
      ApplicationFailure::Invalid
    );
    assert_eq!(bytes.2.load(Ordering::SeqCst), reads);
  });
}

#[test]
fn skill_import_recovery_keeps_original_bytes_after_local_folder_removal_and_reimport() {
  run(async {
    let project = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    let bytes = Arc::new(Bytes::default());
    let folder = std::env::temp_dir().join(format!("octacity-role-skill-{}", ProjectId::generate()));
    std::fs::create_dir(&folder).unwrap();
    let file = folder.join("SKILL.md");
    std::fs::write(&file, b"# Original local skill\n").unwrap();
    let reviewed = std::fs::read(&file).unwrap();
    let mut original = command(project);
    original.idempotency_key = "original-local-folder".parse().unwrap();
    original.entries = vec![ReviewedSkillEntry::File {
      path: "SKILL.md".into(),
      sha256: FactoryDigest::content_sha256(&reviewed),
      bytes: reviewed,
    }];
    let import = ProjectSkillImport::new(
      store.clone(),
      bytes.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    );
    let retained = import
      .import(&crate::factory_admission_tests::context(), original)
      .await
      .unwrap();
    std::fs::remove_dir_all(folder).unwrap();
    drop(import);
    let restarted = ProjectSkillImport::new(
      store,
      bytes.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    );
    let replacement = restarted
      .import(&crate::factory_admission_tests::context(), command(project))
      .await
      .unwrap();
    assert_ne!(
      retained.intent.bundle.reference(),
      replacement.intent.bundle.reference()
    );
    let materialized = restarted
      .materialize(
        project,
        BTreeMap::from([(
          octacity_server_factory::FactoryRepositoryPath::new("skills/original").unwrap(),
          retained.intent.bundle,
        )]),
      )
      .await
      .unwrap();
    assert_eq!(materialized.inputs.len(), 1);
    let input = &materialized.inputs[0];
    assert_eq!(
      input.sha256,
      FactoryDigest::content_sha256(b"# Original local skill\n").to_string()
    );
    assert_eq!(
      bytes.0.lock().unwrap()[&input.artifact_id.parse::<ArtifactId>().unwrap()],
      b"# Original local skill\n"
    );
  });
}
