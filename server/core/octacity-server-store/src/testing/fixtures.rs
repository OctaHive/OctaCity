use super::*;
use octacity_protocol::{PlatformArchitecture, PlatformOs};
use octacity_server_domain::RuntimeClass;
use octacity_server_job::JobRequirements;

pub(crate) fn trigger_request(occurrence: u64, base: u64, pool: PoolId) -> AcceptTrigger {
  let build_id = id(base + 1);
  let attempt_id = id(base + 2);
  let root_id = id(base + 3);
  let child_id = id(base + 4);
  let root = materialized_job(root_id, "root", Vec::new(), pool, build_id);
  let child = materialized_job(child_id, "child", vec![root.id], pool, build_id);
  let build = ImmutableBuildInput {
    id: build_id,
    project_id: id::<ProjectId>(31),
    configuration_id: id::<BuildConfigurationId>(32),
    configuration_version: BuildConfigurationVersion::INITIAL,
    pipeline_id: id::<PipelineId>(33),
    pipeline_version: PipelineVersion::INITIAL,
    repository_id: id::<RepositoryId>(34),
    repository_version: RepositoryVersion::INITIAL,
    immutable_revision: ImmutableRevision::new("0123456789abcdef").unwrap(),
    input_snapshot: json!({"parameter": "value"}),
    effective_policy_snapshot: json!({"allowed_pool": pool.to_string()}),
    priority: 10,
  };
  let trigger = NormalizedTriggerOccurrence::root(
    id(occurrence),
    TriggerDefinitionRef {
      id: id::<TriggerId>(30),
      version: TriggerVersion::INITIAL,
    },
    TriggerTarget {
      configuration_id: build.configuration_id,
      configuration_version: build.configuration_version,
    },
    TriggerIdentity::new(format!("manual:{occurrence}")).unwrap(),
    TriggerCause::Manual {},
    TriggerMetadata::default(),
    time(400),
  )
  .unwrap();
  AcceptTrigger::new(
    trigger,
    build,
    attempt_id,
    AttemptNumber::FIRST,
    vec![root, child],
    TriggerIntentDigest::from_bytes([occurrence as u8; 32]),
    time(500),
  )
  .unwrap()
}

fn materialized_job(
  id: JobId,
  node: &str,
  dependencies: Vec<JobId>,
  pool: PoolId,
  build_id: BuildId,
) -> MaterializedJob {
  MaterializedJob::new(
    id,
    PipelineNodeId::new(node).unwrap(),
    dependencies,
    DependencyPolicy::AllSucceeded,
    vec![pool],
    MaterializedJobPayload::new(
      JobRequirements {
        capabilities: BTreeSet::new(),
        labels: BTreeMap::new(),
        minimum_cpu_millis: 0,
        minimum_memory_bytes: 0,
        minimum_disk_bytes: 0,
        runtime_class: RuntimeClass::Native,
        operating_system: PlatformOs::Linux,
        architecture: PlatformArchitecture::Amd64,
      },
      job_spec_template(build_id, node),
    )
    .unwrap(),
  )
  .unwrap()
}

/// Builds a retry candidate from an accepted graph using fresh identities.
pub fn retry_request(request: &AcceptTrigger, base: u64, key: &str, requested_at: Timestamp) -> RetryBuild {
  let attempt_id = id(base);
  let attempt_number = request.attempt_number.next().unwrap();
  let identities: BTreeMap<_, _> = request
    .jobs
    .iter()
    .enumerate()
    .map(|(index, job)| (job.id, id::<JobId>(base + 1 + index as u64)))
    .collect();
  let jobs = request
    .jobs
    .iter()
    .map(|job| {
      MaterializedJob::new(
        identities[&job.id],
        job.pipeline_node_id.clone(),
        job
          .dependencies
          .iter()
          .map(|dependency| identities[dependency])
          .collect(),
        job.dependency_policy,
        job.allowed_pools.clone(),
        MaterializedJobPayload::new(job.requirements.clone(), job.job_spec_template.clone()).unwrap(),
      )
      .unwrap()
    })
    .collect();
  RetryBuild::new(
    request.build.id,
    attempt_id,
    attempt_number,
    jobs,
    IdempotencyKey::new(key).unwrap(),
    requested_at,
  )
  .unwrap()
}

/// Derives deterministic stable execution intent for adapter-only fixtures.
pub fn job_spec_template(build_id: BuildId, node: &str) -> JobSpecTemplate {
  const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
  let build = JobSpecBuildSnapshot::new(
    build_id,
    ImmutableRevision::new("0123456789abcdef").unwrap(),
    None,
    octacity_server_domain::RepositoryLocator::new("https://example.test/repository.git").unwrap(),
    BTreeMap::new(),
  )
  .unwrap();
  let policy = JobSpecPolicySnapshot::new(
    SourcePluginPolicy::new("git", "1.0.0", DIGEST, "url").unwrap(),
    OctaSpec {
      version: "0.4.0".to_owned(),
      runner_sha256: DIGEST.to_owned(),
      runner_protocol: 3,
      event_schema: 3,
      plugin_protocol: 1,
      plugin_digests: BTreeMap::new(),
    },
    RuntimeSpec {
      target: RuntimeTarget::Native {
        platform: PlatformSpec {
          os: PlatformOs::Linux,
          architecture: PlatformArchitecture::Amd64,
        },
      },
      cpu_millis: 1_000,
      memory_bytes: 1_024,
      writable_disk_bytes: 2_048,
      timeout_seconds: 60,
      network: NetworkPolicy::Disabled,
      workload_identity_profile: None,
    },
    None,
    OutputLimits {
      artifact_count: 0,
      artifact_bytes: 0,
      report_count: 0,
      report_bytes: 0,
      single_output_bytes: 0,
    },
    octacity_server_job::JobSpecValidity::new(600).unwrap(),
  )
  .unwrap();
  derive_job_spec_template(
    &build,
    PipelineNodeId::new(node).unwrap(),
    json!({"commands": [node]}),
    &policy,
  )
  .unwrap()
}
