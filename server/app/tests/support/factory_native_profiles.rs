//! Public configuration publication for the native profile runtime contract.
use super::{managed, postgres, runtime_fixture};
use octacity_server_application::*;
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_pipeline::*;
use octacity_server_store::*;
use octacity_server_store_postgres::PostgresStore;
use serde_json::json;
use std::collections::BTreeSet;

fn key(text: &str) -> FactoryKey {
  text.parse().unwrap()
}
fn replay(text: &str) -> IdempotencyKey {
  IdempotencyKey::new(text).unwrap()
}
fn now() -> Timestamp {
  Timestamp::from_unix_millis(
    i64::try_from(
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis(),
    )
    .unwrap(),
  )
  .unwrap()
}

pub(super) fn runtime_config(
  directory: &std::path::Path,
  database_name: &str,
  profile: &FactoryCodexBuildProfile,
) -> octacity_server::ServerConfig {
  let profiles = directory.join("profiles.json");
  let references = profile
    .candidate_capture_tool
    .iter()
    .map(|reference| {
      json!({
        "kind":"node_capability","alias":"trusted-capture","reference":reference,
      })
    })
    .collect::<Vec<_>>();
  let catalog = json!({"version":1,"projects":[{"project_id":profile.build_configuration.project_id(),"references":references,
    "build_configurations":[{"alias":"observer","reference":profile.build_configuration}]}],
    "codex":[{"toolchain":{"plugin":profile.toolchain.plugin,"executable":profile.toolchain.executable,"model":profile.toolchain.model},
    "build_configuration":profile.build_configuration,"instructions":profile.instructions,"credentials":profile.credentials,"credential_purpose":"evaluator",
    "usage_policy":profile.usage_policy,"candidate_capture_tool":profile.candidate_capture_tool,"pipeline_node":profile.pipeline_node,
    "parameters":profile.parameters,"project_permissions":profile.project_permissions,
    "local_permissions":{"grants":profile.local_permissions.grants(),"enforceable":PermissionCategory::ALL}}]});
  std::fs::write(&profiles, serde_json::to_vec(&catalog).unwrap()).unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(&profiles, std::fs::Permissions::from_mode(0o600)).unwrap();
  }
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(database_name);
  let source = runtime_fixture::runtime_config_source(
    directory,
    url.as_str(),
    &std::env::var("OCTACITY_MINIO_ENDPOINT").unwrap(),
    "[factory]\npoll_interval_milliseconds=25\nclaim_lifetime_milliseconds=30000\nbatch_size=4\nconcurrency=1",
  );
  let source = format!(
    "factory_profiles_file={}\nretention_poll_interval_milliseconds=50\n{source}",
    json!(profiles.to_str().unwrap())
  );
  let canonical = managed::canonical_v3_spec();
  std::fs::write(directory.join("job-spec-policy.json"),serde_json::to_vec(&json!({
    "source":{"provider":"git","plugin_version":"0.1.0","plugin_sha256":canonical.source.plugin_sha256,"repository_parameter":"url"},
    "octa":canonical.octa,"validity":3600,
  })).unwrap()).unwrap();
  for (file, variable) in [
    ("object-access-key", "OCTACITY_MINIO_ACCESS_KEY"),
    ("object-secret-key", "OCTACITY_MINIO_SECRET_KEY"),
  ] {
    std::fs::write(directory.join(file), std::env::var(variable).unwrap()).unwrap();
  }
  let mut config: toml::Value = toml::from_str(&source).unwrap();
  config["object_storage"]
    .as_table_mut()
    .unwrap()
    .insert("force_path_style".to_owned(), toml::Value::Boolean(true));
  octacity_server::ServerConfig::parse_toml(&toml::to_string(&config).unwrap()).unwrap()
}

pub(super) async fn publish_ordinary_configuration(
  store: &PostgresStore,
  work: &WorkEnvelope,
) -> FactoryCodexBuildProfile {
  use octacity_server_store::testing::management_mutation as mutation;
  let project = work.subject().project_id();
  let repository = work.subject().repository().unwrap().repository_id();
  let pool = PoolId::generate();
  let pipeline = PipelineId::generate();
  let canonical = managed::canonical_v3_spec();
  let timestamp = now();
  store
    .create_project(mutation(CreateProject {
      id: project,
      parent_id: None,
      name: ProjectName::new("Native profiles").unwrap(),
      idempotency_key: replay("project"),
      created_at: timestamp,
    }))
    .await
    .unwrap();
  store
    .create_agent_pool(mutation(CreateAgentPool {
      id: pool,
      name: PoolName::new("Qualified isolated worker").unwrap(),
      definition: serde_json::from_value(
        json!({"enabled":true,"drain_state":"accepting","admission_policy":{"mode":"any"},
      "concurrency_limit":1,"fairness_policy":"priority_fifo","static_capacity_limit":1}),
      )
      .unwrap(),
      idempotency_key: replay("pool"),
      published_at: timestamp,
    }))
    .await
    .unwrap();
  store
    .create_repository(mutation(CreateRepository {
      id: repository,
      project_id: project,
      name: RepositoryName::new("Exact source").unwrap(),
      definition: RepositoryDefinition {
        vcs_integration_id: IntegrationId::generate(),
        repository_locator: RepositoryLocator::new("https://example.test/source.git").unwrap(),
        selection: RepositorySelectionPolicy {
          allowed_references: BTreeSet::new(),
          default_reference: None,
          allow_exact_revision: true,
        },
      },
      idempotency_key: replay("repository"),
      published_at: timestamp,
    }))
    .await
    .unwrap();
  let capability = ExecutionCapability::new("native").unwrap();
  let node = PipelineNodeId::new("operator-published-job-42").unwrap();
  let dag = PublishablePipelineDag::new(
    vec![
      PipelineNode::new(
        node.clone(),
        JobName::new("Observe contract").unwrap(),
        DependencyPolicy::AllSucceeded,
        vec![capability.clone()],
        json!({"command":"repository-command-must-not-run"}),
      )
      .unwrap(),
    ],
    vec![],
    &CapabilityCatalog::new([capability]),
  )
  .unwrap();
  store
    .create_pipeline(mutation(CreatePipeline {
      id: pipeline,
      project_id: project,
      name: PipelineName::new("Observation").unwrap(),
      dag,
      idempotency_key: replay("pipeline"),
      published_at: timestamp,
    }))
    .await
    .unwrap();
  let definition: BuildConfigurationDefinition = serde_json::from_value(json!({
    "enabled":true,"job_concurrency_limit":1,"repository_id":repository,"repository_version":1,"pipeline_id":pipeline,"pipeline_version":1,
    "parameters":{"parameters":{},"deny_unknown":true},"triggers":{"allowed":["manual"]},
    "agent_requirements":{"capabilities":["native"],"labels":{},"minimum_cpu_millis":1000,"minimum_memory_bytes":536870912,"minimum_disk_bytes":1073741824},
    "allowed_pools":[pool],"runtime":{"class":"virtualization","operating_system":"linux","architecture":"arm64",
      "host_platform":canonical.runtime.target.host_platform,"required_guarantees":canonical.runtime.target.required_guarantees,
      "immutable_image":canonical.runtime.target.immutable_image,"cpu_millis":1000,"memory_bytes":536870912,"writable_disk_bytes":1073741824,
      "timeout_seconds":600,"network":{"mode":"restricted","allowed_hosts":["api.openai.com"]},"workload_identity_profile":null},
    "secrets_profile":null,"cache":{"namespace":null,"read":false,"write":false},
    "artifacts":{"artifact_count":10,"artifact_bytes":100000,"report_count":10,"report_bytes":100000,"single_output_bytes":100000},
    "retry":{"max_attempts":1,"retry_on":[]}
  })).unwrap();
  let configuration_id = BuildConfigurationId::generate();
  store
    .create_build_configuration(mutation(CreateBuildConfiguration {
      id: configuration_id,
      project_id: project,
      name: BuildConfigurationName::new("Exact immutable observation").unwrap(),
      definition: definition.clone(),
      idempotency_key: replay("configuration"),
      published_at: timestamp,
    }))
    .await
    .unwrap();
  let target = ProjectExecutionTarget {
    mode: canonical.runtime.target.mode,
    host_platform: canonical.runtime.target.host_platform,
    target_platform: canonical.runtime.target.target_platform,
    required_guarantees: canonical.runtime.target.required_guarantees.clone(),
  };
  let policy: ProjectPolicyDefinition = serde_json::from_value(json!({
    "pools":{"mode":"replace","value":[pool]},"repositories":{"mode":"replace","value":[repository]},
    "secret_profiles":{"mode":"replace","value":[]},"identity_profiles":{"mode":"replace","value":[]},
    "runtimes":{"mode":"replace","value":["virtualization"]},"execution_targets":{"mode":"replace","value":[target]},
    "cache":{"mode":"replace","value":{"namespaces":[],"read":false,"write":false,"max_bytes":0}},
    "artifacts":{"mode":"replace","value":definition.artifacts},
    "concurrency":{"mode":"replace","value":{"active_builds":1,"active_jobs":1}},
    "retention":{"mode":"replace","value":{"build_seconds":86400,"log_seconds":86400,"artifact_seconds":86400,"cache_seconds":86400}}
  })).unwrap();
  store
    .publish_project_policy(mutation(PublishProjectPolicy {
      project_id: project,
      expected_current_version: None,
      policy: serde_json::to_value(policy).unwrap(),
      idempotency_key: replay("policy"),
      published_at: timestamp,
    }))
    .await
    .unwrap();
  let mut permissions = serde_json::to_value(managed::permissions(false)).unwrap();
  permissions["secret_profiles"] = json!(["review-credential"]);
  for mount in permissions["mounts"].as_array_mut().unwrap() {
    if mount["root"] == "/workspace/source" {
      mount["mode"] = json!("read_only");
    }
  }
  permissions["outputs"]["kinds"] = json!(["codex-run-trace", "codex-run-provenance", "codex-run-result"]);
  permissions["outputs"]["max_artifact_bytes"] = json!(100000);
  permissions["outputs"]["max_report_bytes"] = json!(100000);
  let permissions: FactoryPermissionSet = serde_json::from_value(permissions).unwrap();
  FactoryCodexBuildProfile {
    authentication: octacity_server_application::FactoryCodexAuthentication::default(),
    mcp_configurations: vec![],
    toolchain: FactoryTaskToolchain {
      plugin: managed::reference("codex", "0.5.0"),
      executable: managed::reference("codex-cli", "0.116.0"),
      model: managed::reference("immutable-model", "v1"),
    },
    build_configuration: BuildConfigurationRef::new(
      configuration_id,
      BuildConfigurationVersion::INITIAL,
      project,
      FactoryDigest::from_bytes([3; 32]),
    ),
    instructions: FactoryText::new("Observe only the explicitly provided contract.").unwrap(),
    credentials: FactoryCredentialProfiles::new(
      key("author-credential"),
      key("review-credential"),
      key("source-credential"),
      key("delivery-credential"),
    )
    .unwrap(),
    consumer: FactoryCredentialConsumer::ReadOnlyReview,
    usage_policy: FactoryCodexUsagePolicy::ReportedCost,
    tool_risk: None,
    tool_response_limits: None,
    reviewer: None,
    candidate_capture_tool: None,
    capture: None,
    pipeline_node: node,
    parameters: Default::default(),
    project_permissions: permissions.clone(),
    local_permissions: LocalPermissionCeiling::fully_enforced(permissions),
  }
}

pub(super) async fn admit(
  store: &PostgresStore,
  work: WorkEnvelope,
  profile: &FactoryCodexBuildProfile,
) -> PublishedFactoryAdmission {
  // Artifact and report ceilings each reserve 100000 bytes in the ordinary Job.
  let budget = BudgetLimit::new(4, 600000, 100, 1000, 200000).unwrap();
  let schema = FlowDataSchema::new(key("declared.measurement"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("operator-selected-observation-73"),
    kind: FlowNodeKind::Reasoning,
    input_schema: Some(schema.reference().clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      key("observed"),
      FlowOutcomeKind::Success,
      schema.reference().clone(),
    )],
    budget,
    permissions: profile.project_permissions.clone(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_build(FlowBuildBinding {
    tool: profile.toolchain.executable.clone(),
    result_tool: None,
    capture_write_paths: None,
    capture_deliverables: None,
    plugin: profile.toolchain.plugin.clone(),
    model_or_tool: profile.toolchain.model.clone(),
    task_digest: profile.task_digest(),
    build_configuration: profile.build_configuration.clone(),
    result_output: key("codex-run-result"),
    result_output_kind: EvidenceOutputKind::Report,
    result_outcome: key("observed"),
    evidence: vec![],
  })
  .unwrap()
  .with_input_binding(
    FlowInputBinding::new(
      Default::default(),
      FlowDataMapping::new(FlowValueMapping::Constant { value: json!(true) }).unwrap(),
    )
    .unwrap(),
  )
  .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema.reference().clone()),
    entry: node.key().clone(),
    nodes: vec![node.clone()],
    transitions: vec![FlowTransition::new(
      node.key().clone(),
      key("observed"),
      FlowTransitionTarget::Terminal(key("done")),
    )],
    terminals: vec![FlowTerminalDefinition::new(key("done"), schema.reference().clone())],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, profile.project_permissions.clone(), 1).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let root = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&root).unwrap();
  let flow = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, budget, profile.project_permissions.clone()).unwrap(),
    root,
    cycle,
    FactoryWipLimits::new(4, 4).unwrap(),
  )
  .unwrap()
  .with_data_schemas(vec![schema])
  .unwrap();
  admit_flow(store, work, profile, flow).await
}

/// Publishes an explicit immutable Flow using the same native configuration and
/// admission boundary as the default readonly profile contract.
pub(super) async fn admit_flow(
  store: &PostgresStore,
  work: WorkEnvelope,
  profile: &FactoryCodexBuildProfile,
  flow: AdmittedFlow,
) -> PublishedFactoryAdmission {
  use octacity_server_store::testing::management_mutation as mutation;
  let run = FactoryRun::admitted(flow.root_run().factory_run_id(), &work);
  let budget = flow.limits().budget();
  let fixture = postgres::factory_configuration::fixture(work.subject().project_id());
  let mut draft = fixture.draft;
  draft.hard_budget = budget;
  draft.wip_limits = flow.wip_limits();
  draft.flow = Some(
    FactoryFlowConfiguration::new(
      flow.closure().clone(),
      flow.limits().clone(),
      flow.data_schemas().to_vec(),
      Default::default(),
    )
    .unwrap(),
  );
  let mut entries = fixture.choices.entries().clone();
  entries.build_configurations = vec![(key("observer"), profile.build_configuration.clone())];
  let choices = FactoryConfigurationChoices::try_new(entries).unwrap();
  let configuration = FactoryConfiguration::publish(
    work.configuration().id(),
    work.configuration().version(),
    work.subject().project_id(),
    work.configuration().definition_digest(),
    draft.clone(),
    &choices,
  )
  .unwrap();
  store
    .create_factory_configuration(mutation(CreateFactoryConfiguration {
      idempotency_key: replay("factory-configuration"),
      published_at: now(),
      intent: FactoryConfigurationMutationIntent::Create {
        id: work.configuration().id(),
        project_id: work.subject().project_id(),
        definition_digest: work.configuration().definition_digest(),
        draft,
      },
      configuration,
    }))
    .await
    .unwrap();
  store
    .admit_factory_work(mutation(AdmitFactoryWork {
      probe: FactoryAdmissionProbe {
        source_scope: FactoryWorkSourceScope {
          source: key("manual"),
          security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
        },
        external_identity: work.external_identity().clone(),
        intent_digest: FactoryDigest::from_bytes([7; 32]),
        idempotency_key: replay("admission"),
      },
      repository_version: Some(RepositoryVersion::INITIAL),

      work: work.clone(),
      run: run.clone(),
      flow: flow.clone(),
      admitted_at: now(),
    }))
    .await
    .unwrap();
  PublishedFactoryAdmission {
    work,
    run,
    flow,
    admitted_at: now(),
  }
}
