//! Shared ordinary published execution context used by admission contracts.
use crate::{
  ArtifactPolicy, BuildExecutionContext, CachePolicy, ConcurrencyPolicy, EffectiveProjectPolicy, IdentityProfileName,
  JobSpecToolchainPolicy, PolicySource, ProjectPolicy, RetentionPolicy, RuntimeClass, SecretProfileName,
};
use octacity_protocol::{OctaSpec, PlatformArchitecture, PlatformOs};
use octacity_server_domain::*;
use octacity_server_job::SourcePluginPolicy;
use octacity_server_pipeline::*;
use octacity_server_store::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn context() -> (BuildExecutionContext, PoolId) {
  let project_id = id::<ProjectId>(1);
  let repository_id = id::<RepositoryId>(2);
  let pipeline_id = id::<PipelineId>(3);
  let configuration_id = id::<BuildConfigurationId>(4);
  let configured_pool = id::<PoolId>(5);
  let shared_pool = id::<PoolId>(6);
  let policy_only_pool = id::<PoolId>(7);
  let capability = ExecutionCapability::new("native").unwrap();
  let root = PipelineNode::new(
    PipelineNodeId::new("root").unwrap(),
    JobName::new("Root").unwrap(),
    DependencyPolicy::AllSucceeded,
    vec![capability.clone()],
    json!({"commands": ["build"]}),
  )
  .unwrap();
  let child = PipelineNode::new(
    PipelineNodeId::new("child").unwrap(),
    JobName::new("Child").unwrap(),
    DependencyPolicy::AllSucceeded,
    Vec::new(),
    json!({"commands": ["test"]}),
  )
  .unwrap();
  let dag = PublishablePipelineDag::new(
    vec![root, child],
    vec![PipelineEdge::new(
      PipelineNodeId::new("root").unwrap(),
      PipelineNodeId::new("child").unwrap(),
    )],
    &CapabilityCatalog::new([capability.clone()]),
  )
  .unwrap()
  .into_dag();
  let artifacts = ArtifactPolicy {
    artifact_count: 10,
    artifact_bytes: 1_000_000,
    report_count: 10,
    report_bytes: 1_000_000,
    single_output_bytes: 100_000,
  };
  let configuration = PublishedBuildConfiguration {
    id: configuration_id,
    project_id,
    name: BuildConfigurationName::new("Build").unwrap(),
    version: BuildConfigurationVersion::INITIAL,
    definition: BuildConfigurationDefinition {
      enabled: true,
      job_concurrency_limit: 2,
      repository_id,
      repository_version: RepositoryVersion::INITIAL,
      pipeline_id,
      pipeline_version: PipelineVersion::INITIAL,
      parameters: ParameterSchema {
        parameters: BTreeMap::from([
          (
            "required".to_owned(),
            ParameterDefinition {
              value_type: ParameterType::String,
              required: true,
              default: None,
            },
          ),
          (
            "with_default".to_owned(),
            ParameterDefinition {
              value_type: ParameterType::Boolean,
              required: false,
              default: Some(json!(true)),
            },
          ),
        ]),
        deny_unknown: true,
      },
      triggers: ConfigurationTriggerPolicy {
        allowed: BTreeSet::from([TriggerKind::Manual]),
      },
      agent_requirements: ConfigurationAgentRequirements {
        capabilities: BTreeSet::from([capability]),
        labels: BTreeMap::from([("tier".to_owned(), "trusted".to_owned())]),
        minimum_cpu_millis: 1_000,
        minimum_memory_bytes: 1_024,
        minimum_disk_bytes: 2_048,
      },
      allowed_pools: BTreeSet::from([configured_pool, shared_pool]),
      runtime: ConfigurationRuntimePolicy {
        class: RuntimeClass::Native,
        operating_system: PlatformOs::Linux,
        architecture: PlatformArchitecture::Amd64,
        host_platform: None,
        required_guarantees: BTreeSet::new(),
        immutable_image: None,
        cpu_millis: 1_000,
        memory_bytes: 1_024,
        writable_disk_bytes: 2_048,
        timeout_seconds: 60,
        network: ConfigurationNetworkPolicy::Disabled,
        workload_identity_profile: None,
      },
      secrets_profile: Some(SecretProfileName::new("ci/secrets.yml").unwrap()),
      cache: ConfigurationCachePolicy {
        namespace: None,
        read: false,
        write: false,
      },
      artifacts,
      retry: ConfigurationRetryPolicy {
        max_attempts: 1,
        retry_on: BTreeSet::<RetryClass>::new(),
      },
    },
    published_at: time(10),
  };
  let repository = PublishedRepository {
    id: repository_id,
    project_id,
    name: RepositoryName::new("Source").unwrap(),
    version: RepositoryVersion::INITIAL,
    definition: RepositoryDefinition {
      vcs_integration_id: id::<IntegrationId>(8),
      repository_locator: octacity_server_domain::RepositoryLocator::new("https://example.test/repository.git")
        .unwrap(),
      selection: RepositorySelectionPolicy {
        allowed_references: BTreeSet::from([SourceReference::new("refs/heads/main").unwrap()]),
        default_reference: Some(SourceReference::new("refs/heads/main").unwrap()),
        allow_exact_revision: true,
      },
    },
    published_at: time(10),
  };
  let pipeline = PublishedPipeline {
    id: pipeline_id,
    project_id,
    name: PipelineName::new("Build").unwrap(),
    version: PipelineVersion::INITIAL,
    dag,
    published_at: time(10),
  };
  let effective_policy = EffectiveProjectPolicy {
    sources: vec![PolicySource {
      project_id,
      version: ProjectPolicyVersion::INITIAL,
    }],
    policy: ProjectPolicy {
      pools: BTreeSet::from([shared_pool, policy_only_pool]),
      repositories: BTreeSet::from([repository_id]),
      secret_profiles: BTreeSet::from([SecretProfileName::new("ci/secrets.yml").unwrap()]),
      identity_profiles: BTreeSet::<IdentityProfileName>::new(),
      runtimes: BTreeSet::from([RuntimeClass::Native]),
      execution_targets: BTreeSet::new(),
      cache: CachePolicy {
        namespaces: BTreeSet::new(),
        read: false,
        write: false,
        max_bytes: 0,
      },
      artifacts,
      concurrency: ConcurrencyPolicy {
        active_builds: 10,
        active_jobs: 10,
      },
      retention: RetentionPolicy {
        build_seconds: 86_400,
        log_seconds: 86_400,
        artifact_seconds: 86_400,
        cache_seconds: 86_400,
      },
    },
  };
  (
    BuildExecutionContext {
      configuration,
      repository,
      pipeline,
      effective_policy,
      job_spec_toolchain: JobSpecToolchainPolicy {
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
      },
    },
    shared_pool,
  )
}

fn time(n: i64) -> Timestamp {
  Timestamp::from_unix_millis(n).unwrap()
}
fn id<T: FromUuid>(n: u128) -> T {
  T::from_uuid(uuid::Uuid::from_u128(n))
}
trait FromUuid {
  fn from_uuid(value: uuid::Uuid) -> Self;
}
macro_rules! ids { ($($kind:ty),+) => { $(impl FromUuid for $kind { fn from_uuid(value: uuid::Uuid) -> Self { <$kind>::from_uuid(value).unwrap() } })+ }; }
ids!(
  ProjectId,
  RepositoryId,
  PipelineId,
  BuildConfigurationId,
  PoolId,
  IntegrationId
);
const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
