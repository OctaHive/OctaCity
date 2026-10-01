use std::sync::Arc;

use octacity_server_domain::{
  BuildConfigurationName, BuildConfigurationVersion, PipelineId, PipelineName, PipelineVersion, PoolId, ProjectId,
  RepositoryId, RepositoryName, RepositoryVersion, TriggerVersion,
};

use crate::configuration_contract_testing::{build_configuration, repository_definition};
use crate::pipeline_contract_testing::dag;
use crate::test_support::{id, run_ready, time};
use crate::testing::{
  InMemoryConfigurationStore, InMemoryPipelineStore, InMemoryTriggerDefinitionDiscoveryStore, management_mutation,
};
use crate::{
  BuildConfigurationListVisibility, ConfigurationDiscoveryStore, ConfigurationStore, CreateBuildConfiguration,
  CreatePipeline, CreateRepository, CurrentBuildConfigurationSummary, CurrentPipelineSummary, CurrentRepositorySummary,
  CurrentTriggerDefinitionKind, CurrentTriggerDefinitionSummary, IdempotencyKey, ListProjectBuildConfigurations,
  ListProjectPipelines, ListProjectRepositories, ListProjectTriggerDefinitions, PipelineDiscoveryStore,
  PipelineListVisibility, PipelineStore, PublishBuildConfigurationVersion, PublishPipelineVersion,
  PublishRepositoryVersion, RepositoryListVisibility, StoreError, TriggerDefinitionDiscoveryStore,
  TriggerDefinitionListVisibility,
};

/// Verifies Project ownership, current-version selection, and visibility-first pagination.
pub fn verify_in_memory_definition_discovery_contract() {
  run_ready(
    async {
      verify_pipeline_discovery().await;
      verify_configuration_discovery().await;
      verify_trigger_discovery().await;
    },
    "in-memory definition discovery must complete without I/O",
  );
}

async fn verify_pipeline_discovery() {
  let store = Arc::new(InMemoryPipelineStore::new());
  let project_id = id::<ProjectId>(1);
  let other_project_id = id::<ProjectId>(2);
  store.seed_project(project_id).unwrap();
  store.seed_project(other_project_id).unwrap();

  for value in 10..=15 {
    let owner = if value == 15 { other_project_id } else { project_id };
    store
      .create_pipeline(management_mutation(CreatePipeline {
        id: id(value),
        project_id: owner,
        name: PipelineName::new(format!("pipeline-{value}")).unwrap(),
        dag: dag(&format!("build-{value}")),
        idempotency_key: key(format!("create-pipeline-{value}")),
        published_at: time(value as i64),
      }))
      .await
      .unwrap();
  }
  store
    .publish_pipeline_version(management_mutation(PublishPipelineVersion {
      id: id(10),
      expected_current_version: PipelineVersion::INITIAL,
      dag: dag("build-current"),
      idempotency_key: key("publish-pipeline-current"),
      published_at: time(20),
    }))
    .await
    .unwrap();

  verify_seeded_pipeline_definition_discovery_contract(store.as_ref(), project_id).await;
}

/// Verifies the shared Pipeline discovery contract against a seeded adapter.
pub async fn verify_seeded_pipeline_definition_discovery_contract<S>(store: &S, project_id: ProjectId)
where
  S: PipelineDiscoveryStore + ?Sized,
{
  let visibility = PipelineListVisibility::restricted([id(10), id(12), id(14), id(15)]).unwrap();
  let first = store
    .list_project_pipelines(ListProjectPipelines::new(project_id, None, 2, visibility.clone()).unwrap())
    .await
    .unwrap();
  assert_eq!(
    first.items,
    vec![
      CurrentPipelineSummary {
        id: id(10),
        project_id,
        name: PipelineName::new("pipeline-10").unwrap(),
        version: PipelineVersion::new(2).unwrap(),
        published_at: time(20),
      },
      CurrentPipelineSummary {
        id: id(12),
        project_id,
        name: PipelineName::new("pipeline-12").unwrap(),
        version: PipelineVersion::INITIAL,
        published_at: time(12),
      },
    ]
  );
  assert_eq!(first.next_cursor, Some(id(12)));

  let second = store
    .list_project_pipelines(ListProjectPipelines::new(project_id, first.next_cursor, 2, visibility).unwrap())
    .await
    .unwrap();
  assert_eq!(
    second.items,
    vec![CurrentPipelineSummary {
      id: id(14),
      project_id,
      name: PipelineName::new("pipeline-14").unwrap(),
      version: PipelineVersion::INITIAL,
      published_at: time(14),
    }]
  );
  assert_eq!(second.next_cursor, None);
  let missing = store
    .list_project_pipelines(ListProjectPipelines::new(id(999), None, 1, PipelineListVisibility::none()).unwrap())
    .await
    .unwrap_err();
  assert_eq!(
    missing,
    StoreError::NotFound {
      entity: octacity_server_domain::EntityKind::Project,
    }
  );
}

async fn verify_configuration_discovery() {
  let store = Arc::new(InMemoryConfigurationStore::new());
  let project_id = id::<ProjectId>(1);
  let other_project_id = id::<ProjectId>(2);
  let pipeline_id = id::<PipelineId>(3);
  let other_pipeline_id = id::<PipelineId>(4);
  let pool_id = id::<PoolId>(5);
  store.seed_project(project_id).unwrap();
  store.seed_project(other_project_id).unwrap();
  store
    .seed_pipeline_version(project_id, pipeline_id, PipelineVersion::INITIAL)
    .unwrap();
  store
    .seed_pipeline_version(other_project_id, other_pipeline_id, PipelineVersion::INITIAL)
    .unwrap();
  store.seed_pool(pool_id).unwrap();

  for value in 100..=105 {
    let owner = if value == 105 { other_project_id } else { project_id };
    store
      .create_repository(management_mutation(CreateRepository {
        id: id(value),
        project_id: owner,
        name: RepositoryName::new(format!("repository-{value}")).unwrap(),
        definition: repository_definition(&format!("octacity/repository-{value}")),
        idempotency_key: key(format!("create-repository-{value}")),
        published_at: time(value as i64),
      }))
      .await
      .unwrap();
  }
  store
    .publish_repository_version(management_mutation(PublishRepositoryVersion {
      id: id(100),
      expected_current_version: RepositoryVersion::INITIAL,
      definition: repository_definition("octacity/repository-current"),
      idempotency_key: key("publish-repository-current"),
      published_at: time(200),
    }))
    .await
    .unwrap();

  for value in 200..=205 {
    let (owner, repository_id, selected_pipeline_id) = if value == 205 {
      (other_project_id, id::<RepositoryId>(105), other_pipeline_id)
    } else {
      (project_id, id::<RepositoryId>(100), pipeline_id)
    };
    store
      .create_build_configuration(management_mutation(CreateBuildConfiguration {
        id: id(value),
        project_id: owner,
        name: BuildConfigurationName::new(format!("configuration-{value}")).unwrap(),
        definition: build_configuration(
          repository_id,
          if value == 205 {
            RepositoryVersion::INITIAL
          } else {
            RepositoryVersion::new(2).unwrap()
          },
          selected_pipeline_id,
          PipelineVersion::INITIAL,
          pool_id,
          "initial",
        ),
        idempotency_key: key(format!("create-configuration-{value}")),
        published_at: time(value as i64),
      }))
      .await
      .unwrap();
  }
  let mut current_configuration = build_configuration(
    id(100),
    RepositoryVersion::new(2).unwrap(),
    pipeline_id,
    PipelineVersion::INITIAL,
    pool_id,
    "current",
  );
  current_configuration.enabled = false;
  store
    .publish_build_configuration_version(management_mutation(PublishBuildConfigurationVersion {
      id: id(200),
      expected_current_version: BuildConfigurationVersion::INITIAL,
      definition: current_configuration,
      idempotency_key: key("publish-configuration-current"),
      published_at: time(300),
    }))
    .await
    .unwrap();

  verify_seeded_configuration_definition_discovery_contract(store.as_ref(), project_id).await;
}

/// Verifies the shared Repository and Build Configuration discovery contract against a seeded adapter.
pub async fn verify_seeded_configuration_definition_discovery_contract<S>(store: &S, project_id: ProjectId)
where
  S: ConfigurationDiscoveryStore + ?Sized,
{
  let repository_visibility = RepositoryListVisibility::restricted([id(100), id(102), id(104), id(105)]).unwrap();
  let first = store
    .list_project_repositories(
      ListProjectRepositories::new(project_id, None, 2, repository_visibility.clone()).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    first.items,
    vec![
      CurrentRepositorySummary {
        id: id(100),
        project_id,
        name: RepositoryName::new("repository-100").unwrap(),
        version: RepositoryVersion::new(2).unwrap(),
        published_at: time(200),
      },
      CurrentRepositorySummary {
        id: id(102),
        project_id,
        name: RepositoryName::new("repository-102").unwrap(),
        version: RepositoryVersion::INITIAL,
        published_at: time(102),
      },
    ]
  );
  assert_eq!(first.next_cursor, Some(id(102)));
  let second = store
    .list_project_repositories(
      ListProjectRepositories::new(project_id, first.next_cursor, 2, repository_visibility).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    second.items,
    vec![CurrentRepositorySummary {
      id: id(104),
      project_id,
      name: RepositoryName::new("repository-104").unwrap(),
      version: RepositoryVersion::INITIAL,
      published_at: time(104),
    }]
  );
  assert_eq!(second.next_cursor, None);

  let visibility = BuildConfigurationListVisibility::restricted([id(200), id(202), id(204), id(205)]).unwrap();
  let first = store
    .list_project_build_configurations(
      ListProjectBuildConfigurations::new(project_id, None, 2, visibility.clone()).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    first.items,
    vec![
      CurrentBuildConfigurationSummary {
        id: id(200),
        project_id,
        name: BuildConfigurationName::new("configuration-200").unwrap(),
        version: BuildConfigurationVersion::new(2).unwrap(),
        enabled: false,
        published_at: time(300),
      },
      CurrentBuildConfigurationSummary {
        id: id(202),
        project_id,
        name: BuildConfigurationName::new("configuration-202").unwrap(),
        version: BuildConfigurationVersion::INITIAL,
        enabled: true,
        published_at: time(202),
      },
    ]
  );
  assert_eq!(first.next_cursor, Some(id(202)));
  let second = store
    .list_project_build_configurations(
      ListProjectBuildConfigurations::new(project_id, first.next_cursor, 2, visibility).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    second.items,
    vec![CurrentBuildConfigurationSummary {
      id: id(204),
      project_id,
      name: BuildConfigurationName::new("configuration-204").unwrap(),
      version: BuildConfigurationVersion::INITIAL,
      enabled: true,
      published_at: time(204),
    }]
  );
  assert_eq!(second.next_cursor, None);

  let missing_project_id = id(999);
  let repository_error = store
    .list_project_repositories(
      ListProjectRepositories::new(missing_project_id, None, 1, RepositoryListVisibility::none()).unwrap(),
    )
    .await
    .unwrap_err();
  assert_eq!(
    repository_error,
    StoreError::NotFound {
      entity: octacity_server_domain::EntityKind::Project,
    }
  );
  let configuration_error = store
    .list_project_build_configurations(
      ListProjectBuildConfigurations::new(missing_project_id, None, 1, BuildConfigurationListVisibility::none())
        .unwrap(),
    )
    .await
    .unwrap_err();
  assert_eq!(
    configuration_error,
    StoreError::NotFound {
      entity: octacity_server_domain::EntityKind::Project,
    }
  );
}

async fn verify_trigger_discovery() {
  let store = InMemoryTriggerDefinitionDiscoveryStore::new();
  let project_id = id::<ProjectId>(1);
  let other_project_id = id::<ProjectId>(2);
  store.seed_project(project_id).unwrap();
  store.seed_project(other_project_id).unwrap();
  for value in 300..=305 {
    let other_project = value == 305;
    let mut summary = trigger_summary(
      value,
      1,
      if other_project { other_project_id } else { project_id },
      if other_project { 205 } else { 200 },
    );
    summary.kind = match value {
      302 => CurrentTriggerDefinitionKind::Scheduled,
      304 => CurrentTriggerDefinitionKind::Internal,
      _ => CurrentTriggerDefinitionKind::Manual,
    };
    store.seed_version(summary).unwrap();
  }
  let mut current = trigger_summary(300, 2, project_id, 200);
  current.enabled = false;
  current.kind = CurrentTriggerDefinitionKind::Internal;
  store.seed_version(current).unwrap();
  store
    .seed_version(trigger_summary(301, 2, other_project_id, 205))
    .unwrap();

  verify_seeded_trigger_definition_discovery_contract(&store, project_id).await;
}

/// Verifies the shared Trigger-definition discovery contract against a seeded adapter.
pub async fn verify_seeded_trigger_definition_discovery_contract<S>(store: &S, project_id: ProjectId)
where
  S: TriggerDefinitionDiscoveryStore + ?Sized,
{
  let visibility = TriggerDefinitionListVisibility::restricted([id(300), id(301), id(302), id(304), id(305)]).unwrap();
  let first = store
    .list_project_trigger_definitions(
      ListProjectTriggerDefinitions::new(project_id, None, 2, visibility.clone()).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    first.items,
    vec![
      CurrentTriggerDefinitionSummary {
        id: id(300),
        project_id,
        configuration_id: id(200),
        configuration_version: BuildConfigurationVersion::INITIAL,
        version: TriggerVersion::new(2).unwrap(),
        kind: CurrentTriggerDefinitionKind::Internal,
        enabled: false,
        published_at: time(302),
      },
      CurrentTriggerDefinitionSummary {
        id: id(302),
        project_id,
        configuration_id: id(200),
        configuration_version: BuildConfigurationVersion::INITIAL,
        version: TriggerVersion::INITIAL,
        kind: CurrentTriggerDefinitionKind::Scheduled,
        enabled: true,
        published_at: time(303),
      },
    ]
  );
  assert_eq!(first.next_cursor, Some(id(302)));
  let second = store
    .list_project_trigger_definitions(
      ListProjectTriggerDefinitions::new(project_id, first.next_cursor, 2, visibility).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    second.items,
    vec![CurrentTriggerDefinitionSummary {
      id: id(304),
      project_id,
      configuration_id: id(200),
      configuration_version: BuildConfigurationVersion::INITIAL,
      version: TriggerVersion::INITIAL,
      kind: CurrentTriggerDefinitionKind::Internal,
      enabled: true,
      published_at: time(305),
    }]
  );
  assert_eq!(second.next_cursor, None);

  let moved = store
    .list_project_trigger_definitions(
      ListProjectTriggerDefinitions::new(
        id(2),
        None,
        1,
        TriggerDefinitionListVisibility::restricted([id(301)]).unwrap(),
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    moved.items,
    vec![CurrentTriggerDefinitionSummary {
      id: id(301),
      project_id: id(2),
      configuration_id: id(205),
      configuration_version: BuildConfigurationVersion::INITIAL,
      version: TriggerVersion::new(2).unwrap(),
      kind: CurrentTriggerDefinitionKind::Manual,
      enabled: true,
      published_at: time(303),
    }]
  );

  let missing = store
    .list_project_trigger_definitions(
      ListProjectTriggerDefinitions::new(id(999), None, 1, TriggerDefinitionListVisibility::none()).unwrap(),
    )
    .await
    .unwrap_err();
  assert_eq!(
    missing,
    StoreError::NotFound {
      entity: octacity_server_domain::EntityKind::Project,
    }
  );
}

fn trigger_summary(
  value: u64,
  version: u64,
  project_id: ProjectId,
  configuration_id: u64,
) -> CurrentTriggerDefinitionSummary {
  CurrentTriggerDefinitionSummary {
    id: id(value),
    project_id,
    configuration_id: id(configuration_id),
    configuration_version: BuildConfigurationVersion::INITIAL,
    version: TriggerVersion::new(version).unwrap(),
    kind: CurrentTriggerDefinitionKind::Manual,
    enabled: true,
    published_at: time(value as i64 + version as i64),
  }
}

fn key(value: impl Into<String>) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}
