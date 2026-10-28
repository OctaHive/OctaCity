//! Real REST resources for the packaged Factory pilot.
use super::{Client, ReleaseBackend, get_json, json, post_management, resource_id, string};
use octacity_server_domain::{BuildConfigurationVersion, PipelineNodeId};
use octacity_server_factory::{BuildConfigurationRef, FactoryDigest, FactoryResourceLimits};

pub(super) struct PilotResources {
  pub(super) project: String,
  pub(super) pool: String,
  pub(super) repository: String,
  pub(super) configuration: BuildConfigurationRef,
  pub(super) pipeline_node: PipelineNodeId,
  pub(super) trigger: String,
}

pub(super) async fn create_resources(
  client: &Client,
  origin: &str,
  run: &str,
  backend: &ReleaseBackend,
  repository: &str,
) -> PilotResources {
  create_resources_with_limits(
    client,
    origin,
    run,
    backend,
    repository,
    FactoryResourceLimits::new(1000, 536_870_912, backend.workspace_bytes(), 64, 120_000).unwrap(),
  )
  .await
}

pub(super) async fn create_resources_with_limits(
  client: &Client,
  origin: &str,
  run: &str,
  backend: &ReleaseBackend,
  repository: &str,
  limits: FactoryResourceLimits,
) -> PilotResources {
  let pool = post_management(client, origin, "/api/v1/agent-pools", &format!("{run}-pool"), json!({
    "name":format!("pilot-{run}"),"definition":{"enabled":true,"drain_state":"accepting", "admission_policy":backend.pool_admission_policy(),"concurrency_limit":4,"fairness_policy":"priority_fifo","static_capacity_limit":4}
  })).await;
  let pool = resource_id(&pool);
  let project = post_management(
    client,
    origin,
    "/api/v1/projects",
    &format!("{run}-project"),
    json!({"parent_id":null,"name":format!("pilot-{run}")}),
  )
  .await;
  let project = resource_id(&project);
  let pipeline_node = PipelineNodeId::new(format!("worker-{run}")).unwrap();
  let pipeline=post_management(client,origin,"/api/v1/pipelines",&format!("{run}-pipeline"),json!({
    "project_id":project,"name":format!("pilot-{run}"),"dag":{"nodes":[{
      "id":pipeline_node,"name":"Installed worker","dependency_policy":"all_succeeded","required_capabilities":backend.required_capabilities(),
      "execution":{"octafile":"Octafile.yml","commands":["prepare"],"parallel":false,"failfast":true}
    }],"edges":[]}
  })).await;
  let pipeline = resource_id(&pipeline);
  let repository=post_management(client,origin,"/api/v1/repositories",&format!("{run}-repository"),json!({
    "project_id":project,"name":format!("source-{run}"),"definition":{"vcs_integration_id":uuid::Uuid::new_v4(),"repository_locator":repository,"selection":{"allowed_references":[],"default_reference":null,"allow_exact_revision":true}}
  })).await;
  let repository = resource_id(&repository);
  let outputs = json!({"artifact_count":16,"artifact_bytes":4*1024*1024,"report_count":16,"report_bytes":4*1024*1024,"single_output_bytes":4*1024*1024});
  let network = if std::env::var_os("OCTACITY_FACTORY_PILOT_NATIVE_MCP").is_some() {
    json!({"mode":"restricted","allowed_hosts":["host.microsandbox.internal"]})
  } else {
    json!({"mode":"disabled"})
  };
  let configuration=post_management(client,origin,"/api/v1/build-configurations",&format!("{run}-configuration"),json!({
    "project_id":project,"name":format!("pilot-{run}"),"definition":{
      "enabled":true,"job_concurrency_limit":4,"repository_id":repository,"repository_version":1,"pipeline_id":pipeline,"pipeline_version":1,
      "parameters":{"parameters":{},"deny_unknown":true},"triggers":["manual"],
      "agent_requirements":{"capabilities":backend.required_capabilities(),"labels":{},"minimum_cpu_millis":limits.cpu_millis(),"minimum_memory_bytes":limits.memory_bytes(),"minimum_disk_bytes":limits.disk_bytes()},
      "allowed_pools":[pool],"runtime":{"class":backend.runtime_class(),"operating_system":"linux","architecture":backend.guest_architecture(),"host_platform":backend.host_platform(),"required_guarantees":backend.required_guarantees(),"immutable_image":backend.immutable_image(),"cpu_millis":limits.cpu_millis(),"memory_bytes":limits.memory_bytes(),"writable_disk_bytes":limits.disk_bytes(),"timeout_seconds":limits.elapsed_millis()/1000,"network":network,"workload_identity_profile":null},
      "cache":{"namespace":null,"read":false,"write":false},"artifacts":outputs,"retry":{"max_attempts":1,"retry_on":[]}
    }
  })).await;
  let id = resource_id(&configuration);
  let published = get_json(client, format!("{origin}/api/v1/build-configurations/{id}/versions/1")).await;
  assert_eq!(
    published, configuration["resource"],
    "the exact immutable ordinary configuration is read back through REST"
  );
  assert_eq!(published["project_id"], project);
  assert_eq!(published["version"], 1);
  // This operator installation pins the actual public immutable definition,
  // rather than fabricating a reference to an absent ordinary configuration.
  let digest = FactoryDigest::content_sha256(&serde_json::to_vec(&published["definition"]).unwrap());
  let configuration = BuildConfigurationRef::new(
    id.parse().unwrap(),
    BuildConfigurationVersion::INITIAL,
    project.parse().unwrap(),
    digest,
  );
  let mut policy =
    super::support::project_policy_body(&pool, &repository, backend.runtime_class(), backend.execution_target());
  policy["policy"]["artifacts"]["value"] = outputs;
  policy["policy"]["concurrency"]["value"] = json!({"active_builds":4,"active_jobs":4});
  post_management(
    client,
    origin,
    &format!("/api/v1/projects/{project}/policy-versions"),
    &format!("{run}-policy"),
    policy,
  )
  .await;
  let trigger = post_management(
    client,
    origin,
    "/api/v1/trigger-definitions/manual",
    &format!("{run}-trigger-definition"),
    json!({"configuration_id":id,"configuration_version":1,"enabled":true,"definition":{}}),
  )
  .await;
  PilotResources {
    project,
    pool,
    repository,
    configuration,
    pipeline_node,
    trigger: resource_id(&trigger),
  }
}

pub(super) async fn issue_enrollment(
  client: &Client,
  origin: &str,
  run: &str,
  pool: &str,
  backend: &ReleaseBackend,
) -> String {
  let (operating_system, architecture) = backend.agent_platform();
  let response=post_management(client,origin,"/api/v1/agent-enrollments",&format!("{run}-enrollment"),json!({"pool_id":pool,"pool_version":1,"expected_platform":{"operating_system":operating_system,"architecture":architecture}})).await;
  string(&response, "credential")
}
