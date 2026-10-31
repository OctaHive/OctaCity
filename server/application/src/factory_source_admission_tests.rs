use super::*;
use crate::{
  AdmitSourceFactoryWorkCommand, ApplicationError, FactorySourceAdmissionHandlers, FactorySourceAuthorization,
  FactorySourceAuthorizer, FactorySourceItem, RecoverSourceFactoryWorkCommand,
};
use octacity_server_factory::{
  WorkSourceBinding, WorkSourceCapability, WorkSourceCapabilityRole, WorkSourceInput, WorkSourceInputContext,
  WorkSourceInputDraft, WorkSourceInputId, WorkSourceInputVersion, WorkSourceRepositoryRef,
};
use octacity_server_store::{FactoryAdmissionStore, ReadFactoryAdmissionContext};

#[path = "factory_source_mapping_tests.rs"]
mod mapping;
#[path = "factory_source_polling_tests.rs"]
mod polling;
#[path = "factory_source_replay_tests.rs"]
mod retained_intent;

struct InstalledSource(FactorySourceAuthorization);

#[async_trait]
impl FactorySourceAuthorizer for InstalledSource {
  async fn authorize(
    &self,
    _input_id: WorkSourceInputId,
    _version: WorkSourceInputVersion,
  ) -> Result<FactorySourceAuthorization, ApplicationError> {
    Ok(self.0.clone())
  }
}

async fn installed(
  store: &InMemoryFactoryConfigurationStore,
  intent: &AdmitManualFactoryWorkCommand,
  provider: &str,
) -> FactorySourceAuthorization {
  let selected = store
    .factory_admission_context(ReadFactoryAdmissionContext {
      project_id: intent.project_id,
      configuration_id: intent.configuration_id,
      configuration_version: intent.configuration_version,
      repository_id: intent.repository_id,
      repository_version: intent.repository_version,
    })
    .await
    .unwrap();
  let repository = WorkSourceRepositoryRef {
    project_id: intent.project_id,
    id: intent.repository_id.unwrap(),
    version: intent.repository_version.unwrap(),
    definition_digest: FactoryDigest::content_sha256(
      &serde_json::to_vec(&selected.repository.as_ref().unwrap().definition).unwrap(),
    ),
  };
  let capability = WorkSourceCapability {
    project_id: intent.project_id,
    role: WorkSourceCapabilityRole::Source,
    binding: WorkSourceBinding {
      installation: exact(provider, 30),
      capability: exact("ticket-source", 31),
      configuration: exact("input-settings", 32),
      resource: exact("ticket-container", 33),
    },
  };
  let input = WorkSourceInput::publish(
    WorkSourceInputId::generate(),
    intent.project_id,
    WorkSourceInputDraft {
      reporting: None,
      predicate: None,
      connection: None,
      target: selected.configuration.configuration.reference().clone(),
      repository: repository.clone(),
      source: capability.binding.clone(),
      reporter: None,
      enabled: true,
    },
    WorkSourceInputContext {
      target: &selected.configuration.configuration,
      repository: &repository,
      source: Some(&capability),
      reporter: None,
    },
  )
  .unwrap();
  FactorySourceAuthorization {
    input,
    capability,
    resource_identity: ExternalWorkIdentity::new("native-ticket-container").unwrap(),
    context: context(),
  }
}

fn source_command(
  authorization: &FactorySourceAuthorization,
  intent: AdmitManualFactoryWorkCommand,
) -> AdmitSourceFactoryWorkCommand {
  AdmitSourceFactoryWorkCommand {
    input_id: authorization.input.id(),
    input_version: authorization.input.version(),
    item: FactorySourceItem {
      resource: authorization.resource_identity.clone(),
      item: intent.external_identity.clone(),
    },
    intent,
  }
}

struct DeniedSource;

#[async_trait]
impl FactorySourceAuthorizer for DeniedSource {
  async fn authorize(
    &self,
    _input_id: WorkSourceInputId,
    _version: WorkSourceInputVersion,
  ) -> Result<FactorySourceAuthorization, ApplicationError> {
    Err(ApplicationError::capability_unavailable())
  }
}

#[test]
fn an_uninstalled_source_is_denied_before_replay_resolution_or_publication() {
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
    let handlers = FactorySourceAdmissionHandlers::new(
      store.clone(),
      resolver.clone(),
      Arc::new(crate::TrustedNetworkManagementPolicy),
      Arc::new(DeniedSource),
    );
    let intent = command(project, configuration, repository_id);
    let request = AdmitSourceFactoryWorkCommand {
      input_id: WorkSourceInputId::generate(),
      input_version: WorkSourceInputVersion::INITIAL,
      item: FactorySourceItem {
        resource: ExternalWorkIdentity::new("native-ticket-container").unwrap(),
        item: intent.external_identity.clone(),
      },
      intent: intent.clone(),
    };
    let error = handlers.admit(request.clone()).await.unwrap_err();
    assert_eq!(error.classification(), ApplicationFailure::CapabilityUnavailable);
    assert_eq!(resolver.call_count(), 0);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);
    assert!(store.management_audit_facts().await.is_empty());
    let manual = FactoryAdmissionHandlers::new(store.clone(), resolver.clone());
    let applied = admit(&manual, intent.clone()).await.unwrap();
    assert_eq!(applied.disposition, MutationDisposition::Applied);
    let denied_recovery = handlers
      .recover(RecoverSourceFactoryWorkCommand {
        input_id: request.input_id,
        input_version: request.input_version,
        item: request.item,
      })
      .await
      .unwrap_err();
    assert_eq!(
      denied_recovery.classification(),
      ApplicationFailure::CapabilityUnavailable
    );
    let replayed = admit(&manual, intent).await.unwrap();
    assert_eq!(
      replayed,
      crate::FactoryAdmissionOutcome {
        disposition: MutationDisposition::Replayed,
        ..applied
      }
    );
    assert_eq!(resolver.call_count(), 1);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 1);
    assert_eq!(store.management_audit_facts().await.len(), 1);
  });
}

#[test]
fn installed_sources_share_admission_without_impersonating_manual_identities() {
  run_ready(async {
    for provider in ["github", "gitlab", "jira", "custom-tracker", "manual"] {
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
      let authorization = installed(&store, &intent, provider).await;
      let request = source_command(&authorization, intent.clone());
      let manual = admit(
        &FactoryAdmissionHandlers::new(store.clone(), resolver.clone()),
        intent.clone(),
      )
      .await
      .unwrap();
      let source = FactorySourceAdmissionHandlers::new(
        store.clone(),
        resolver.clone(),
        Arc::new(crate::TrustedNetworkManagementPolicy),
        Arc::new(InstalledSource(authorization)),
      );
      let applied = source.admit(request.clone()).await.unwrap();
      let replayed = source.admit(request).await.unwrap();
      assert_eq!(applied.disposition, MutationDisposition::Applied, "{provider}");
      assert_ne!(applied.work_envelope_id, manual.work_envelope_id);
      assert_ne!(applied.factory_run_id, manual.factory_run_id);
      assert_eq!(
        replayed,
        crate::FactoryAdmissionOutcome {
          disposition: MutationDisposition::Replayed,
          ..applied.clone()
        }
      );
      let manual_replay = admit(&FactoryAdmissionHandlers::new(store.clone(), resolver.clone()), intent)
        .await
        .unwrap();
      assert_eq!(manual_replay.factory_run_id, manual.factory_run_id);
      assert_eq!(resolver.call_count(), 2);
      assert_eq!(store.mutation_evidence_counts().await.idempotency, 2);
      assert_eq!(store.management_audit_facts().await.len(), 2);
    }
  });
}

#[test]
fn source_authority_requires_the_exact_enabled_input_and_source_capability() {
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
    let mut invalid = Vec::new();
    let mut wrong_id = request.clone();
    wrong_id.input_id = WorkSourceInputId::generate();
    invalid.push(("input identity", authorized.clone(), wrong_id));
    let mut wrong_version = request.clone();
    wrong_version.input_version = WorkSourceInputVersion::new(2).unwrap();
    invalid.push(("input version", authorized.clone(), wrong_version));
    for field in [
      "project",
      "role",
      "installation",
      "capability",
      "configuration",
      "resource",
    ] {
      let mut changed = authorized.clone();
      match field {
        "project" => changed.capability.project_id = ProjectId::generate(),
        "role" => changed.capability.role = WorkSourceCapabilityRole::Reporter,
        "installation" => changed.capability.binding.installation = exact("substitution", 40),
        "capability" => changed.capability.binding.capability = exact("substitution", 40),
        "configuration" => changed.capability.binding.configuration = exact("substitution", 40),
        "resource" => changed.capability.binding.resource = exact("substitution", 40),
        _ => unreachable!(),
      }
      invalid.push((field, changed, request.clone()));
    }
    let selected = store
      .factory_admission_context(ReadFactoryAdmissionContext {
        project_id: project,
        configuration_id: configuration,
        configuration_version: request.intent.configuration_version,
        repository_id: Some(repository_id),

        repository_version: request.intent.repository_version,
      })
      .await
      .unwrap();
    let mut disabled = authorized.clone();
    let mut definition = disabled.input.definition().clone();
    definition.enabled = false;
    disabled.input = disabled
      .input
      .replace(
        WorkSourceInputVersion::new(2).unwrap(),
        definition,
        WorkSourceInputContext {
          target: &selected.configuration.configuration,
          repository: &authorized.input.definition().repository,
          source: Some(&authorized.capability),
          reporter: None,
        },
      )
      .unwrap();
    let disabled_request = source_command(&disabled, request.intent.clone());
    invalid.push(("disabled", disabled, disabled_request));
    for (case, authorization, command) in invalid {
      let handlers = FactorySourceAdmissionHandlers::new(
        store.clone(),
        resolver.clone(),
        Arc::new(crate::TrustedNetworkManagementPolicy),
        Arc::new(InstalledSource(authorization)),
      );
      assert_eq!(
        handlers.admit(command).await.unwrap_err().classification(),
        ApplicationFailure::Invalid,
        "{case}"
      );
    }
    assert_eq!(resolver.call_count(), 0);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);
    assert!(store.management_audit_facts().await.is_empty());
  });
}

#[test]
fn ticket_intent_cannot_substitute_the_installed_project_configuration_or_repository() {
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
    let authorization = installed(&store, &intent, "gitlab").await;
    let handlers = FactorySourceAdmissionHandlers::new(
      store.clone(),
      resolver.clone(),
      Arc::new(crate::TrustedNetworkManagementPolicy),
      Arc::new(InstalledSource(authorization.clone())),
    );
    for field in [
      "project",
      "configuration",
      "configuration version",
      "repository",
      "repository version",
    ] {
      let mut changed = intent.clone();
      match field {
        "project" => changed.project_id = ProjectId::generate(),
        "configuration" => changed.configuration_id = FactoryConfigurationId::generate(),
        "configuration version" => changed.configuration_version = FactoryConfigurationVersion::new(2).unwrap(),
        "repository" => changed.repository_id = Some(RepositoryId::generate()),
        "repository version" => changed.repository_version = Some(RepositoryVersion::new(2).unwrap()),
        _ => unreachable!(),
      }
      assert_eq!(
        handlers
          .admit(source_command(&authorization, changed))
          .await
          .unwrap_err()
          .classification(),
        ApplicationFailure::Invalid,
        "{field}"
      );
    }
    assert_eq!(resolver.call_count(), 0);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);
    assert!(store.management_audit_facts().await.is_empty());
  });
}

struct SourceVisibilityPolicy {
  hidden: ManagementResource,
  restricted: bool,
}

#[async_trait]
impl ManagementAuthorizationPolicy for SourceVisibilityPolicy {
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    _action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource == &self.hidden {
      if self.restricted {
        return Ok(ManagementAuthorizationGrant::new(ManagementVisibility::none()));
      }
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}

#[test]
fn hidden_source_dependencies_and_execution_are_denied_for_new_admission_and_replay() {
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
      vec![artifact(92)],
    )
    .unwrap();
    let authorization = installed(&store, &intent, "jira").await;
    let authorizer = Arc::new(InstalledSource(authorization.clone()));
    let request = source_command(&authorization, intent.clone());
    let applied = FactorySourceAdmissionHandlers::new(
      store.clone(),
      resolver.clone(),
      Arc::new(crate::TrustedNetworkManagementPolicy),
      authorizer.clone(),
    )
    .admit(request.clone())
    .await
    .unwrap();
    let before = store.factory_run_snapshot(applied.factory_run_id).await.unwrap();
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
    .map(|(kind, identity)| {
      ManagementResource::instance(kind, crate::ManagementResourceIdentity::new(identity).unwrap()).unwrap()
    })
    .collect::<Vec<_>>();
    protected.push(crate::ManagementAuthorizationTarget::validated_management_resource(&intent).unwrap());
    for hidden in protected {
      for restricted in [false, true] {
        let handlers = FactorySourceAdmissionHandlers::new(
          store.clone(),
          resolver.clone(),
          Arc::new(SourceVisibilityPolicy {
            hidden: hidden.clone(),
            restricted,
          }),
          authorizer.clone(),
        );
        let mut fresh = request.clone();
        fresh.intent.external_identity = ExternalWorkIdentity::new("another-item").unwrap();
        fresh.item.item = ExternalWorkIdentity::new("another-item").unwrap();
        fresh.intent.idempotency_key = IdempotencyKey::new("another-attempt").unwrap();
        for command in [request.clone(), fresh] {
          let result = handlers.admit(command).await;
          assert!(result.is_err(), "{hidden:?}, restricted={restricted}: {result:?}");
          assert_eq!(
            result.unwrap_err().classification(),
            ApplicationFailure::Invalid,
            "{hidden:?}, restricted={restricted}"
          );
        }
      }
    }
    assert_eq!(resolver.call_count(), 1);
    assert_eq!(
      store.factory_run_snapshot(applied.factory_run_id).await.unwrap(),
      before
    );
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 1);
    assert_eq!(store.management_audit_facts().await.len(), 1);
  });
}

#[test]
fn source_pins_must_match_the_authoritative_target_flow_and_repository_content() {
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
    let authorization = installed(&store, &intent, "github").await;
    let selected = store
      .factory_admission_context(ReadFactoryAdmissionContext {
        project_id: project,
        configuration_id,
        configuration_version: intent.configuration_version,
        repository_id: Some(repository_id),

        repository_version: intent.repository_version,
      })
      .await
      .unwrap();
    let target = selected.configuration.configuration;
    let mut substituted = serde_json::to_value(&target).unwrap();
    *substituted.pointer_mut("/reference/definition_digest").unwrap() = serde_json::json!(digest(55));
    let wrong_digest: FactoryConfiguration = serde_json::from_value(substituted).unwrap();
    let wrong_flow = super::configuration(project, configuration_id);
    assert_ne!(wrong_flow.flow().closure.root(), target.flow().closure.root());
    let original_repository = authorization.input.definition().repository.clone();
    let wrong_repository = WorkSourceRepositoryRef {
      definition_digest: digest(56),
      ..original_repository.clone()
    };
    for (case, target, repository) in [
      ("configuration content", wrong_digest, original_repository.clone()),
      ("root flow", wrong_flow, original_repository),
      ("repository content", target, wrong_repository),
    ] {
      let mut resolved = authorization.clone();
      let mut definition = resolved.input.definition().clone();
      definition.target = target.reference().clone();
      definition.repository = repository.clone();
      resolved.input = WorkSourceInput::publish(
        resolved.input.id(),
        project,
        definition,
        WorkSourceInputContext {
          target: &target,
          repository: &repository,
          source: Some(&resolved.capability),
          reporter: None,
        },
      )
      .unwrap();
      let request = source_command(&resolved, intent.clone());
      let handler = FactorySourceAdmissionHandlers::new(
        store.clone(),
        resolver.clone(),
        Arc::new(crate::TrustedNetworkManagementPolicy),
        Arc::new(InstalledSource(resolved)),
      );
      let result = handler.admit(request).await;
      assert!(result.is_err(), "{case}: {result:?}");
      assert_eq!(
        result.unwrap_err().classification(),
        ApplicationFailure::Invalid,
        "{case}"
      );
    }
    assert_eq!(resolver.call_count(), 0);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);
    assert!(store.management_audit_facts().await.is_empty());
  });
}
