use std::{collections::BTreeMap, collections::BTreeSet, str::FromStr};

use octacity_protocol::{OctaSpec, PlatformArchitecture, PlatformOs};
use octacity_server_application::{
  ArtifactPolicy, AttemptProjection, BuildConfigurationProjection, BuildProjection, CacheNamespace, CachePolicy,
  ConcurrencyPolicy, DagCausalityProjection, EffectiveProjectPolicy, IdentityProfileName, JobAssignmentProjection,
  JobFailureClassification, JobOutputKind, JobOutputReference, JobPlacementProjection, JobProjection,
  JobSpecToolchainPolicy, JobTerminalOutcomeProjection, ManualSourceSelection, PipelineProjection, PolicySource,
  ProjectPolicy, ProjectProjection, ProjectionError, RetentionPolicy, RuntimeClass, SecretProfileName,
  Sha256DigestProjection, TriggerHistoryProjection,
};
use octacity_server_domain::{
  AgentId, ArtifactId, ArtifactName, AttemptId, AttemptNumber, AttemptVersion, BuildConfigurationName,
  BuildConfigurationVersion, BuildVersion, ImmutableRevision, IntegrationId, JobId, JobName, JobVersion, PipelineName,
  PipelineNodeId, PipelineVersion, PoolId, ProjectId, ProjectPolicyVersion, ProjectVersion, RepositoryId,
  RepositoryVersion, SourceReference, Timestamp, TriggerId, TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
};
use octacity_server_job::{JobFailureClass, JobState, SourcePluginPolicy};
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_pipeline::{
  CapabilityCatalog, DependencyPolicy, ExecutionCapability, PipelineEdge, PipelineNode, PublishablePipelineDag,
};
use octacity_server_store::{
  BuildConfigurationDefinition, ConfigurationAgentRequirements, ConfigurationCachePolicy, ConfigurationNetworkPolicy,
  ConfigurationRetryPolicy, ConfigurationRuntimePolicy, ConfigurationTriggerPolicy, ImmutableBuildInput,
  ParameterDefinition, ParameterSchema, ParameterType, Project, ProjectDetails, PublishedBuildConfiguration,
  PublishedPipeline, RetryClass, TriggerCause, TriggerDefinitionRef, TriggerKind, TriggerMetadata, TriggerTarget,
};
use octacity_server_trigger::{NormalizedTriggerOccurrence, TriggerEventKind, TriggerOccurrenceState};
use serde_json::{Value, json};

const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const CREDENTIAL_MARKER: &str = "credential-marker-that-must-not-escape";
const PRIVATE_URL: &str = "https://objects.test/private?X-Amz-Credential=private";

#[test]
fn resource_projections_are_distinct_safe_application_models() {
  let project_id = id::<ProjectId>(1);
  let root = Project {
    id: id(2),
    parent_id: None,
    name: "Root".parse().unwrap(),
    version: ProjectVersion::INITIAL,
    created_at: time(1),
    updated_at: time(1),
  };
  let child = Project {
    id: project_id,
    parent_id: Some(root.id),
    name: "Child".parse().unwrap(),
    version: ProjectVersion::INITIAL,
    created_at: time(2),
    updated_at: time(2),
  };
  let project = ProjectProjection::from(ProjectDetails {
    project: child,
    ancestors: vec![root],
  });
  let pipeline = PipelineProjection::try_from(pipeline(project_id, json!({"commands": ["build"]}))).unwrap();
  let configuration = BuildConfigurationProjection::try_from(configuration(project_id)).unwrap();

  assert_eq!(project.ancestors.len(), 1);
  assert_eq!(pipeline.nodes[0].execution.commands, ["build"]);
  assert_eq!(
    configuration.runtime.workload_identity_profile,
    Some(IdentityProfileName::new("ci-workload").unwrap())
  );
  assert_eq!(
    configuration.cache.namespace,
    Some(CacheNamespace::new("project-cache").unwrap())
  );
  assert_safe(&json!({
    "project": project,
    "pipeline": pipeline,
    "configuration": configuration,
  }));
}

#[test]
fn pipeline_and_build_projection_boundaries_reject_internal_or_sensitive_shapes() {
  let project_id = id::<ProjectId>(1);
  for forbidden in ["secrets_profile", "credential", "upload_url", "signed_job_spec"] {
    let mut template = serde_json::Map::from_iter([("commands".to_owned(), json!(["build"]))]);
    template.insert(forbidden.to_owned(), json!(CREDENTIAL_MARKER));
    assert_eq!(
      PipelineProjection::try_from(pipeline(project_id, Value::Object(template))),
      Err(ProjectionError::InvalidPipelineTemplate),
      "accepted forbidden Pipeline field {forbidden}"
    );
  }

  let trigger = trigger_history();
  let mut build = build_input(project_id);
  let projection = BuildProjection::from_authoritative(
    &build,
    BuildState::Running,
    BuildVersion::INITIAL,
    trigger.clone(),
    time(20),
    time(21),
  )
  .unwrap();
  let encoded = serde_json::to_string(&projection).unwrap();
  assert!(!encoded.contains("job_spec_toolchain"));
  assert!(!encoded.contains(DIGEST));
  assert_safe(&serde_json::to_value(projection).unwrap());

  build
    .effective_policy_snapshot
    .as_object_mut()
    .unwrap()
    .insert("provider_configuration".to_owned(), json!({"token": CREDENTIAL_MARKER}));
  assert_eq!(
    BuildProjection::from_authoritative(
      &build,
      BuildState::Running,
      BuildVersion::INITIAL,
      trigger,
      time(20),
      time(21),
    ),
    Err(ProjectionError::InvalidBuildSnapshot)
  );
}

#[test]
fn trigger_and_dag_projections_preserve_causality_without_private_capabilities() {
  let trigger = trigger_history();
  let trigger_json = serde_json::to_value(&trigger).unwrap();
  assert_eq!(trigger.kind, TriggerKind::External);
  assert!(!trigger_json.to_string().contains(CREDENTIAL_MARKER));
  assert!(!trigger_json.to_string().contains(PRIVATE_URL));

  let attempt_id = id::<AttemptId>(30);
  let root_id = id::<JobId>(31);
  let child_id = id::<JobId>(32);
  let root = job(attempt_id, root_id, "root", Vec::new(), JobState::Succeeded, None);
  let child = job(
    attempt_id,
    child_id,
    "child",
    vec![root_id],
    JobState::Failed,
    Some(JobFailureClassification::Execution),
  );
  let mut duplicate_dependency = child.clone();
  duplicate_dependency.dependencies.push(root_id);
  assert_eq!(
    DagCausalityProjection::new(attempt_id, &[root.clone(), duplicate_dependency]),
    Err(ProjectionError::InvalidDagCausality),
  );
  let dag = DagCausalityProjection::new(attempt_id, &[child.clone(), root.clone()]).unwrap();
  assert_eq!(
    dag.nodes.iter().map(|node| node.job_id).collect::<Vec<_>>(),
    [root_id, child_id]
  );
  assert_eq!(dag.edges[0].predecessor_job_id, root_id);
  assert_eq!(dag.edges[0].dependent_job_id, child_id);

  let attempt = AttemptProjection {
    id: attempt_id,
    build_id: id(33),
    number: AttemptNumber::FIRST,
    retry_of_attempt_id: None,
    state: AttemptState::Failed,
    version: AttemptVersion::INITIAL,
    created_at: time(30),
    updated_at: time(40),
  };
  assert_safe(&json!({
    "trigger": trigger,
    "attempt": attempt,
    "jobs": [root, child],
    "dag": dag,
  }));
}

fn pipeline(project_id: ProjectId, template: Value) -> PublishedPipeline {
  let node = PipelineNode::new(
    PipelineNodeId::new("build").unwrap(),
    JobName::new("Build").unwrap(),
    DependencyPolicy::AllSucceeded,
    vec![ExecutionCapability::new("native").unwrap()],
    template,
  )
  .unwrap();
  let dag = PublishablePipelineDag::new(
    vec![node],
    Vec::<PipelineEdge>::new(),
    &CapabilityCatalog::new([ExecutionCapability::new("native").unwrap()]),
  )
  .unwrap()
  .into_dag();
  PublishedPipeline {
    id: id(3),
    project_id,
    name: PipelineName::new("Build").unwrap(),
    version: PipelineVersion::INITIAL,
    dag,
    published_at: time(3),
  }
}

fn configuration(project_id: ProjectId) -> PublishedBuildConfiguration {
  PublishedBuildConfiguration {
    id: id(4),
    project_id,
    name: BuildConfigurationName::new("Release").unwrap(),
    version: BuildConfigurationVersion::INITIAL,
    definition: BuildConfigurationDefinition {
      enabled: true,
      job_concurrency_limit: 2,
      repository_id: id(5),
      repository_version: RepositoryVersion::INITIAL,
      pipeline_id: id(3),
      pipeline_version: PipelineVersion::INITIAL,
      parameters: ParameterSchema {
        parameters: BTreeMap::from([(
          "release".to_owned(),
          ParameterDefinition {
            value_type: ParameterType::Boolean,
            required: false,
            default: Some(json!(true)),
          },
        )]),
        deny_unknown: true,
      },
      triggers: ConfigurationTriggerPolicy {
        allowed: BTreeSet::from([TriggerKind::Manual]),
      },
      agent_requirements: ConfigurationAgentRequirements {
        capabilities: BTreeSet::from([ExecutionCapability::new("native").unwrap()]),
        labels: BTreeMap::new(),
        minimum_cpu_millis: 1,
        minimum_memory_bytes: 1,
        minimum_disk_bytes: 1,
      },
      allowed_pools: BTreeSet::from([id(6)]),
      runtime: ConfigurationRuntimePolicy {
        class: RuntimeClass::Native,
        operating_system: PlatformOs::Linux,
        architecture: PlatformArchitecture::Amd64,
        immutable_image: None,
        cpu_millis: 1,
        memory_bytes: 1,
        writable_disk_bytes: 1,
        timeout_seconds: 60,
        network: ConfigurationNetworkPolicy::Disabled,
        workload_identity_profile: Some("ci-workload".to_owned()),
      },
      cache: ConfigurationCachePolicy {
        namespace: Some("project-cache".to_owned()),
        read: true,
        write: false,
      },
      artifacts: artifact_policy(),
      retry: ConfigurationRetryPolicy {
        max_attempts: 2,
        retry_on: BTreeSet::from([RetryClass::InfrastructureFailure]),
      },
    },
    published_at: time(4),
  }
}

fn trigger_history() -> TriggerHistoryProjection {
  let occurrence_id = id::<TriggerOccurrenceId>(10);
  let occurrence = NormalizedTriggerOccurrence::root(
    occurrence_id,
    TriggerDefinitionRef {
      id: id::<TriggerId>(11),
      version: TriggerVersion::INITIAL,
    },
    TriggerTarget {
      configuration_id: id(4),
      configuration_version: BuildConfigurationVersion::INITIAL,
    },
    TriggerIdentity::new("delivery:1").unwrap(),
    TriggerCause::External {
      integration_id: id::<IntegrationId>(12),
      repository_id: id::<RepositoryId>(5),
      event_kind: TriggerEventKind::new("push").unwrap(),
      reference: Some(SourceReference::new("refs/heads/main").unwrap()),
      revision: Some(ImmutableRevision::new("0123456789abcdef").unwrap()),
    },
    TriggerMetadata::new(BTreeMap::from([
      ("provider_credential".to_owned(), json!(CREDENTIAL_MARKER)),
      ("private_transfer_url".to_owned(), json!(PRIVATE_URL)),
    ]))
    .unwrap(),
    time(10),
  )
  .unwrap();
  TriggerHistoryProjection::from_occurrence(
    &occurrence,
    TriggerOccurrenceState::Accepted,
    Some(id(13)),
    time(11),
    time(12),
  )
}

fn build_input(project_id: ProjectId) -> ImmutableBuildInput {
  ImmutableBuildInput {
    id: id(13),
    project_id,
    configuration_id: id(4),
    configuration_version: BuildConfigurationVersion::INITIAL,
    pipeline_id: id(3),
    pipeline_version: PipelineVersion::INITIAL,
    repository_id: id(5),
    repository_version: RepositoryVersion::INITIAL,
    immutable_revision: ImmutableRevision::new("0123456789abcdef").unwrap(),
    input_snapshot: json!({
      "parameters": {"release": true},
      "source": serde_json::to_value(ManualSourceSelection::ExactRevision(
        ImmutableRevision::new("0123456789abcdef").unwrap()
      )).unwrap(),
    }),
    effective_policy_snapshot: json!({
      "project": effective_policy(project_id),
      "job_spec_toolchain": toolchain(),
    }),
    project_job_concurrency_limit: 4,
    priority: 10,
  }
}

fn effective_policy(project_id: ProjectId) -> EffectiveProjectPolicy {
  EffectiveProjectPolicy {
    sources: vec![PolicySource {
      project_id,
      version: ProjectPolicyVersion::INITIAL,
    }],
    policy: ProjectPolicy {
      pools: BTreeSet::from([id(6)]),
      repositories: BTreeSet::from([id(5)]),
      secret_profiles: BTreeSet::from([SecretProfileName::new("build-secrets").unwrap()]),
      identity_profiles: BTreeSet::from([IdentityProfileName::new("ci-workload").unwrap()]),
      runtimes: BTreeSet::from([RuntimeClass::Native]),
      cache: CachePolicy {
        namespaces: BTreeSet::from([CacheNamespace::new("project-cache").unwrap()]),
        read: true,
        write: false,
        max_bytes: 1_024,
      },
      artifacts: artifact_policy(),
      concurrency: ConcurrencyPolicy {
        active_builds: 2,
        active_jobs: 4,
      },
      retention: RetentionPolicy {
        build_seconds: 60,
        log_seconds: 60,
        artifact_seconds: 60,
        cache_seconds: 60,
      },
    },
  }
}

fn toolchain() -> JobSpecToolchainPolicy {
  JobSpecToolchainPolicy {
    source: SourcePluginPolicy::new("git", "1.0.0", DIGEST, "url").unwrap(),
    octa: OctaSpec {
      version: "0.4.0".to_owned(),
      runner_sha256: DIGEST.to_owned(),
      runner_protocol: 3,
      event_schema: 3,
      plugin_protocol: 1,
      plugin_digests: BTreeMap::new(),
    },
    validity: octacity_server_job::JobSpecValidity::new(600).unwrap(),
  }
}

fn job(
  attempt_id: AttemptId,
  job_id: JobId,
  node: &str,
  dependencies: Vec<JobId>,
  state: JobState,
  failure: Option<JobFailureClassification>,
) -> JobProjection {
  JobProjection {
    id: job_id,
    attempt_id,
    pipeline_node_id: PipelineNodeId::new(node).unwrap(),
    dependencies,
    dependency_policy: DependencyPolicy::AllSucceeded,
    allowed_pools: vec![id::<PoolId>(40)],
    placement: JobPlacementProjection {
      capabilities: BTreeSet::from([ExecutionCapability::new("native").unwrap()]),
      labels: BTreeMap::new(),
      minimum_cpu_millis: 1,
      minimum_memory_bytes: 1,
      minimum_disk_bytes: 1,
      runtime_class: RuntimeClass::Native,
      operating_system: PlatformOs::Linux,
      architecture: PlatformArchitecture::Amd64,
    },
    state,
    version: JobVersion::INITIAL,
    created_at: time(30),
    updated_at: time(40),
    queue: None,
    assignment: Some(JobAssignmentProjection {
      selected_pool_id: id(40),
      assigned_agent_id: id::<AgentId>(41),
    }),
    terminal: Some(match (state, failure) {
      (JobState::Succeeded, None) => JobTerminalOutcomeProjection::succeeded(time(40)),
      (JobState::Failed, Some(JobFailureClassification::Execution)) => {
        JobTerminalOutcomeProjection::failed(JobFailureClass::Execution, time(40))
      }
      (JobState::Failed, Some(JobFailureClassification::Infrastructure)) => {
        JobTerminalOutcomeProjection::failed(JobFailureClass::Infrastructure, time(40))
      }
      (JobState::Cancelled, Some(JobFailureClassification::Cancelled)) => {
        JobTerminalOutcomeProjection::cancelled(time(40))
      }
      (JobState::Skipped, Some(JobFailureClassification::DependencyPolicy)) => {
        JobTerminalOutcomeProjection::skipped(time(40))
      }
      _ => panic!("invalid terminal fixture"),
    }),
    event_cursor: 2,
    outputs: vec![JobOutputReference {
      id: id::<ArtifactId>(42),
      name: ArtifactName::new("result.tar.zst").unwrap(),
      kind: JobOutputKind::Artifact,
      sha256: Sha256DigestProjection::new(DIGEST).unwrap(),
      size_bytes: 128,
      published_at: time(40),
    }],
  }
}

fn artifact_policy() -> ArtifactPolicy {
  ArtifactPolicy {
    artifact_count: 10,
    artifact_bytes: 10_000,
    report_count: 10,
    report_bytes: 10_000,
    single_output_bytes: 1_000,
  }
}

fn assert_safe(value: &Value) {
  let encoded = value.to_string();
  for forbidden in [
    CREDENTIAL_MARKER,
    PRIVATE_URL,
    "provider_configuration",
    "provider_metadata",
    "signed_job_spec",
    "job_spec_toolchain",
    "lease_fence",
    "transfer_url",
  ] {
    assert!(
      !encoded.contains(forbidden),
      "projection exposed {forbidden}: {encoded}"
    );
  }
}

fn id<T: FromStr>(suffix: u128) -> T
where
  T::Err: std::fmt::Debug,
{
  format!("00000000-0000-0000-0000-{suffix:012x}").parse().unwrap()
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
