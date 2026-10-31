use super::*;
use crate::RecoverSourceFactoryWorkCommand;

fn recovery(request: &AdmitSourceFactoryWorkCommand) -> RecoverSourceFactoryWorkCommand {
  RecoverSourceFactoryWorkCommand {
    input_id: request.input_id,
    input_version: request.input_version,
    item: request.item.clone(),
  }
}

#[test]
fn the_same_native_item_can_enter_distinct_stable_inputs_once_each_for_any_provider() {
  run_ready(async {
    for provider in ["github", "gitlab", "jira", "custom-tracker"] {
      let project = ProjectId::generate();
      let configuration = FactoryConfigurationId::generate();
      let repository_id = RepositoryId::generate();
      let store = seeded_store(
        project,
        configuration,
        repository(project, repository_id, Some("refs/heads/main".parse().unwrap())),
      );
      let resolver = Arc::new(RecordingResolver::default());
      let intent = command(project, configuration, repository_id);
      let mut results = Vec::new();
      for _ in 0..2 {
        let authorization = installed(&store, &intent, provider).await;
        let request = source_command(&authorization, intent.clone());
        let handler = FactorySourceAdmissionHandlers::new(
          store.clone(),
          resolver.clone(),
          Arc::new(crate::TrustedNetworkManagementPolicy),
          Arc::new(InstalledSource(authorization)),
        );
        let applied = handler.admit(request.clone()).await.unwrap();
        assert_eq!(applied.disposition, MutationDisposition::Applied);
        assert_eq!(
          handler.admit(request).await.unwrap().factory_run_id,
          applied.factory_run_id
        );
        results.push(applied);
      }
      assert_ne!(results[0].factory_run_id, results[1].factory_run_id, "{provider}");
      let first = store.factory_run_snapshot(results[0].factory_run_id).await.unwrap();
      let second = store.factory_run_snapshot(results[1].factory_run_id).await.unwrap();
      assert_ne!(first.work.external_identity(), second.work.external_identity());
      assert_eq!(resolver.call_count(), 2);
      assert_eq!(store.mutation_evidence_counts().await.idempotency, 2);
    }
  });
}

#[test]
fn native_resource_identity_cannot_be_substituted_by_an_observation() {
  run_ready(async {
    let project = ProjectId::generate();
    let configuration = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let store = seeded_store(
      project,
      configuration,
      repository(project, repository_id, Some("refs/heads/main".parse().unwrap())),
    );
    let resolver = Arc::new(RecordingResolver::default());
    let intent = command(project, configuration, repository_id);
    let authorized = installed(&store, &intent, "jira").await;
    let mut request = source_command(&authorized, intent);
    request.item.resource = ExternalWorkIdentity::new("unpermitted-ticket-container").unwrap();
    let handler = FactorySourceAdmissionHandlers::new(
      store.clone(),
      resolver.clone(),
      Arc::new(crate::TrustedNetworkManagementPolicy),
      Arc::new(InstalledSource(authorized)),
    );
    assert_eq!(
      handler.admit(request).await.unwrap_err().classification(),
      ApplicationFailure::Invalid
    );
    assert_eq!(resolver.call_count(), 0);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);
  });
}

#[test]
fn changed_observations_recover_original_work_before_preparing_artifacts_or_resolving_vcs() {
  run_ready(async {
    let project = ProjectId::generate();
    let configuration = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let store = seeded_store(
      project,
      configuration,
      repository(project, repository_id, Some("refs/heads/main".parse().unwrap())),
    );
    let resolver = Arc::new(RecordingResolver::default());
    let intent = command(project, configuration, repository_id);
    let authorized = installed(&store, &intent, "jira").await;
    let request = source_command(&authorized, intent);
    let authorizer = Arc::new(InstalledSource(authorized));
    let handler = FactorySourceAdmissionHandlers::new(
      store.clone(),
      resolver.clone(),
      Arc::new(crate::TrustedNetworkManagementPolicy),
      authorizer.clone(),
    );
    assert_eq!(handler.recover(recovery(&request)).await.unwrap(), None);
    let applied = handler.admit(request.clone()).await.unwrap();
    let original = store.factory_run_snapshot(applied.factory_run_id).await.unwrap();
    let restarted = FactorySourceAdmissionHandlers::new(
      store.clone(),
      Arc::new(UnavailableResolver),
      Arc::new(crate::TrustedNetworkManagementPolicy),
      authorizer,
    );
    // New content and label/update metadata would map to different artifacts.
    // Recovery needs only the native item identity and prepares none of them.
    let mut changed = request.clone();
    changed.intent.artifacts = WorkArtifacts::new(artifact(11), artifact(12), vec![artifact(13)]).unwrap();
    changed.intent.classification = WorkClassification::new(
      WorkPriority::new(80).unwrap(),
      RiskClass::High,
      FactoryMetadata::try_new([
        ("labels".to_owned(), "review-ready".to_owned()),
        ("updated".to_owned(), "second-observation".to_owned()),
      ])
      .unwrap(),
    );
    changed.intent.idempotency_key = IdempotencyKey::new("later-poll").unwrap();
    let retained = restarted.recover(recovery(&changed)).await.unwrap().unwrap();
    assert_eq!(retained.work, original.work);
    assert_eq!(retained.run, original.run);
    assert_eq!(retained.admitted_at, applied.admitted_at);
    // Explicit replacement of the accepted intent remains a conflict.
    assert_eq!(
      restarted.admit(changed).await.unwrap_err().classification(),
      ApplicationFailure::Conflict
    );
    assert_eq!(
      store.factory_run_snapshot(applied.factory_run_id).await.unwrap(),
      original
    );
    assert_eq!(resolver.call_count(), 1);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 1);
    assert_eq!(store.management_audit_facts().await.len(), 1);
  });
}

#[test]
fn native_container_ids_are_opaque_and_independent_of_operator_resource_references() {
  run_ready(async {
    let project = ProjectId::generate();
    let configuration = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let store = seeded_store(
      project,
      configuration,
      repository(project, repository_id, Some("refs/heads/main".parse().unwrap())),
    );
    let resolver = Arc::new(RecordingResolver::default());
    let intent = command(project, configuration, repository_id);
    let mut authorized = installed(&store, &intent, "github").await;
    authorized.resource_identity = ExternalWorkIdentity::new("R_kgDONativeContainer").unwrap();
    let first_request = source_command(&authorized, intent.clone());
    let handler = FactorySourceAdmissionHandlers::new(
      store.clone(),
      resolver.clone(),
      Arc::new(crate::TrustedNetworkManagementPolicy),
      Arc::new(InstalledSource(authorized.clone())),
    );
    let first = handler.admit(first_request.clone()).await.unwrap();
    // An operator reference is not the forge's native container identity.
    // Retargeting it cannot recover another repository's item with the same number/id.
    let selected = store
      .factory_admission_context(ReadFactoryAdmissionContext {
        project_id: project,
        configuration_id: configuration,
        configuration_version: intent.configuration_version,
        repository_id: Some(repository_id),

        repository_version: intent.repository_version,
      })
      .await
      .unwrap();
    authorized.capability.binding.resource = ImmutableReference::new(key("ticket-container"), key("v2"), digest(88));
    let mut definition = authorized.input.definition().clone();
    definition.source = authorized.capability.binding.clone();
    let repository_ref = definition.repository.clone();
    authorized.input = authorized
      .input
      .replace(
        WorkSourceInputVersion::new(2).unwrap(),
        definition,
        WorkSourceInputContext {
          target: &selected.configuration.configuration,
          repository: &repository_ref,
          source: Some(&authorized.capability),
          reporter: None,
        },
      )
      .unwrap();
    authorized.resource_identity = ExternalWorkIdentity::new("r_kgDONativeContainer").unwrap();
    let second_request = source_command(&authorized, intent);
    let handler = FactorySourceAdmissionHandlers::new(
      store.clone(),
      resolver.clone(),
      Arc::new(crate::TrustedNetworkManagementPolicy),
      Arc::new(InstalledSource(authorized)),
    );
    assert_eq!(handler.recover(recovery(&second_request)).await.unwrap(), None);
    let second = handler.admit(second_request).await.unwrap();
    assert_ne!(first.factory_run_id, second.factory_run_id);
    let mut wrong_container = recovery(&first_request);
    wrong_container.input_version = WorkSourceInputVersion::new(2).unwrap();
    assert_eq!(
      handler.recover(wrong_container).await.unwrap_err().classification(),
      ApplicationFailure::Invalid
    );
  });
}

#[test]
fn input_replacement_and_rotated_installation_descriptors_recover_the_original_intent() {
  run_ready(async {
    let project = ProjectId::generate();
    let configuration_id = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let store = seeded_store(
      project,
      configuration_id,
      repository(project, repository_id, Some("refs/heads/main".parse().unwrap())),
    );
    let resolver = Arc::new(RecordingResolver::default());
    let intent = command(project, configuration_id, repository_id);
    let mut authorized = installed(&store, &intent, "gitlab").await;
    let original_request = source_command(&authorized, intent.clone());
    let original = FactorySourceAdmissionHandlers::new(
      store.clone(),
      resolver.clone(),
      Arc::new(crate::TrustedNetworkManagementPolicy),
      Arc::new(InstalledSource(authorized.clone())),
    )
    .admit(original_request.clone())
    .await
    .unwrap();
    let frozen = store.factory_run_snapshot(original.factory_run_id).await.unwrap();
    let mut revised = serde_json::to_value(super::super::configuration(project, configuration_id)).unwrap();
    *revised.pointer_mut("/reference/version").unwrap() = serde_json::json!(2);
    *revised.pointer_mut("/reference/definition_digest").unwrap() = serde_json::json!(digest(75));
    let target: FactoryConfiguration = serde_json::from_value(revised).unwrap();
    store
      .seed_factory_configuration_version(PublishedFactoryConfiguration {
        configuration: target.clone(),
        published_at: time(10),
      })
      .unwrap();
    for reference in [
      &mut authorized.capability.binding.installation,
      &mut authorized.capability.binding.configuration,
      &mut authorized.capability.binding.resource,
    ] {
      *reference = ImmutableReference::new(reference.identity().clone(), key("v2"), digest(76));
    }
    let mut definition = authorized.input.definition().clone();
    definition.target = target.reference().clone();
    definition.source = authorized.capability.binding.clone();
    let repository_ref = definition.repository.clone();
    authorized.input = authorized
      .input
      .replace(
        WorkSourceInputVersion::new(2).unwrap(),
        definition.clone(),
        WorkSourceInputContext {
          target: &target,
          repository: &repository_ref,
          source: Some(&authorized.capability),
          reporter: None,
        },
      )
      .unwrap();
    let mut new_intent = intent;
    new_intent.configuration_version = FactoryConfigurationVersion::new(2).unwrap();
    new_intent.artifacts = WorkArtifacts::new(artifact(20), artifact(21), vec![]).unwrap();
    let request = source_command(&authorized, new_intent.clone());
    let handler = FactorySourceAdmissionHandlers::new(
      store.clone(),
      Arc::new(UnavailableResolver),
      Arc::new(crate::TrustedNetworkManagementPolicy),
      Arc::new(InstalledSource(authorized.clone())),
    );
    let receipt = handler.recover(recovery(&request)).await.unwrap().unwrap();
    assert_eq!(receipt.work, frozen.work);
    assert_eq!(receipt.run, frozen.run);
    assert_eq!(receipt.work.configuration().version().get(), 1);
    assert_eq!(
      handler.admit(request.clone()).await.unwrap_err().classification(),
      ApplicationFailure::Conflict
    );
    // A deliberately new input identity is a new processing scope.
    authorized.input = WorkSourceInput::publish(
      WorkSourceInputId::generate(),
      project,
      definition,
      WorkSourceInputContext {
        target: &target,
        repository: &repository_ref,
        source: Some(&authorized.capability),
        reporter: None,
      },
    )
    .unwrap();
    let fresh = source_command(&authorized, new_intent);
    let handler = FactorySourceAdmissionHandlers::new(
      store.clone(),
      Arc::new(UnavailableResolver),
      Arc::new(crate::TrustedNetworkManagementPolicy),
      Arc::new(InstalledSource(authorized)),
    );
    assert_eq!(handler.recover(recovery(&fresh)).await.unwrap(), None);
    assert_eq!(resolver.call_count(), 1);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 1);
  });
}

#[test]
fn recovery_rechecks_original_resource_visibility_and_source_authority() {
  run_ready(async {
    let project = ProjectId::generate();
    let configuration = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let store = seeded_store(
      project,
      configuration,
      repository(project, repository_id, Some("refs/heads/main".parse().unwrap())),
    );
    let resolver = Arc::new(RecordingResolver::default());
    let mut intent = command(project, configuration, repository_id);
    intent.artifacts = WorkArtifacts::new(
      intent.artifacts.task().clone(),
      intent.artifacts.acceptance().clone(),
      vec![artifact(19)],
    )
    .unwrap();
    let authorized = installed(&store, &intent, "gitlab").await;
    let request = source_command(&authorized, intent.clone());
    let authorizer = Arc::new(InstalledSource(authorized.clone()));
    let first = FactorySourceAdmissionHandlers::new(
      store.clone(),
      resolver.clone(),
      Arc::new(crate::TrustedNetworkManagementPolicy),
      authorizer.clone(),
    )
    .admit(request.clone())
    .await
    .unwrap();
    use crate::ManagementResourceKind::{Artifact, FactoryConfiguration, Project, Repository};
    let mut protected = vec![
      (Project, project.to_string()),
      (FactoryConfiguration, configuration.to_string()),
      (Repository, repository_id.to_string()),
      (Artifact, intent.artifacts.task().artifact_id().to_string()),
      (Artifact, intent.artifacts.acceptance().artifact_id().to_string()),
      (Artifact, intent.artifacts.specifications()[0].artifact_id().to_string()),
    ]
    .into_iter()
    .map(|(kind, id)| ManagementResource::instance(kind, crate::ManagementResourceIdentity::new(id).unwrap()).unwrap())
    .collect::<Vec<_>>();
    protected.push(crate::ManagementAuthorizationTarget::validated_management_resource(&intent).unwrap());
    for hidden in protected {
      for restricted in [false, true] {
        let handler = FactorySourceAdmissionHandlers::new(
          store.clone(),
          Arc::new(UnavailableResolver),
          Arc::new(SourceVisibilityPolicy {
            hidden: hidden.clone(),
            restricted,
          }),
          authorizer.clone(),
        );
        assert_eq!(
          handler.recover(recovery(&request)).await.unwrap_err().classification(),
          ApplicationFailure::Invalid
        );
      }
    }
    let denied = FactorySourceAdmissionHandlers::new(
      store.clone(),
      Arc::new(UnavailableResolver),
      Arc::new(crate::TrustedNetworkManagementPolicy),
      Arc::new(DeniedSource),
    );
    assert_eq!(
      denied.recover(recovery(&request)).await.unwrap_err().classification(),
      ApplicationFailure::CapabilityUnavailable
    );
    let mut foreign = authorized;
    foreign.context = ManagementRequestContext::new(
      crate::ManagementActor::authenticated("different-owner").unwrap(),
      crate::ManagementSecurityScope::new("installed:other-scope").unwrap(),
      context().request_id(),
      crate::ManagementRequestAttributes::new(
        crate::ManagementIngress::VerifiedIdentity,
        Some(crate::ManagementClientKind::Automation),
      ),
    )
    .unwrap();
    let policy = SourceVisibilityPolicy {
      hidden: ManagementResource::instance(
        Artifact,
        crate::ManagementResourceIdentity::new(ArtifactId::generate().to_string()).unwrap(),
      )
      .unwrap(),
      restricted: false,
    };
    let other_scope = FactorySourceAdmissionHandlers::new(
      store.clone(),
      Arc::new(UnavailableResolver),
      Arc::new(policy),
      Arc::new(InstalledSource(foreign)),
    );
    assert_eq!(other_scope.recover(recovery(&request)).await.unwrap(), None);
    assert_eq!(
      store
        .factory_run_snapshot(first.factory_run_id)
        .await
        .unwrap()
        .work
        .artifacts(),
      &intent.artifacts
    );
    assert_eq!(resolver.call_count(), 1);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 1);
  });
}

#[derive(Default)]
struct ConcurrentResolver {
  arrived: Mutex<usize>,
  both: std::sync::Condvar,
}

#[async_trait]
impl RevisionResolver for ConcurrentResolver {
  async fn resolve(
    &self,
    _request: RevisionResolutionRequest,
  ) -> Result<octacity_server_domain::ImmutableRevision, RevisionResolutionError> {
    let mut arrived = self.arrived.lock().unwrap();
    *arrived += 1;
    self.both.notify_all();
    let (_guard, deadline) = self
      .both
      .wait_timeout_while(arrived, std::time::Duration::from_secs(5), |count| *count < 2)
      .unwrap();
    assert!(
      !deadline.timed_out(),
      "both observers must reach the controlled VCS boundary"
    );
    Ok(octacity_server_domain::ImmutableRevision::new("0123456789abcdef").unwrap())
  }
}

#[test]
fn concurrent_observers_publish_one_run_and_share_the_original_receipt() {
  let project = ProjectId::generate();
  let configuration = FactoryConfigurationId::generate();
  let repository_id = RepositoryId::generate();
  let store = seeded_store(
    project,
    configuration,
    repository(project, repository_id, Some("refs/heads/main".parse().unwrap())),
  );
  let resolver = Arc::new(ConcurrentResolver::default());
  let intent = command(project, configuration, repository_id);
  let authorized = run_ready(installed(&store, &intent, "jira"));
  let request = source_command(&authorized, intent);
  let handler = Arc::new(FactorySourceAdmissionHandlers::new(
    store.clone(),
    resolver.clone(),
    Arc::new(crate::TrustedNetworkManagementPolicy),
    Arc::new(InstalledSource(authorized)),
  ));
  let observers: Vec<_> = (0..2)
    .map(|index| {
      let handler = handler.clone();
      let mut request = request.clone();
      request.intent.idempotency_key = IdempotencyKey::new(format!("observer-{index}")).unwrap();
      std::thread::spawn(move || run_ready(handler.admit(request)).unwrap())
    })
    .collect();
  let results: Vec<_> = observers.into_iter().map(|observer| observer.join().unwrap()).collect();
  assert_eq!(results[0].factory_run_id, results[1].factory_run_id);
  assert_eq!(results[0].work_envelope_id, results[1].work_envelope_id);
  assert_ne!(results[0].disposition, results[1].disposition);
  assert_eq!(*resolver.arrived.lock().unwrap(), 2);
  let recovered = run_ready(handler.recover(recovery(&request))).unwrap().unwrap();
  assert_eq!(recovered.run.id(), results[0].factory_run_id);
  assert_eq!(run_ready(store.mutation_evidence_counts()).idempotency, 1);
  assert_eq!(run_ready(store.management_audit_facts()).len(), 1);
}
