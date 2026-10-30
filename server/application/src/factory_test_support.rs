//! Existing memory ports shared by Factory application and transport conformance.
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactByteReader, ArtifactObject, ArtifactStoreError};
use octacity_server_domain::{ArtifactId, ProjectId};
use std::{
  collections::BTreeMap,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
};
#[derive(Default)]
/// Immutable byte port used by import/retention and public transport conformance tests.
pub struct InMemoryArtifactBytes(
  pub(crate) Mutex<BTreeMap<ArtifactId, Vec<u8>>>,
  pub(crate) AtomicBool,
  pub(crate) AtomicUsize,
);
#[async_trait::async_trait]
impl ArtifactBytePublisher for InMemoryArtifactBytes {
  async fn publish_verified(&self, object: &ArtifactObject, bytes: Vec<u8>) -> Result<(), ArtifactStoreError> {
    self.0.lock().unwrap().insert(object.artifact_id(), bytes);
    if self.1.swap(false, Ordering::SeqCst) {
      return Err(ArtifactStoreError::Backend {
        operation: octacity_artifact_store::ArtifactStoreOperation::PublishVerified,
      });
    }
    Ok(())
  }
}
#[async_trait::async_trait]
impl ArtifactByteReader for InMemoryArtifactBytes {
  async fn read_published(&self, object: &ArtifactObject, _: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    self.2.fetch_add(1, Ordering::SeqCst);
    self
      .0
      .lock()
      .unwrap()
      .get(&object.artifact_id())
      .cloned()
      .ok_or(ArtifactStoreError::NotFound)
  }
}

/// Compatible installed profile data reused by application and HTTP authoring contracts.
pub fn installed_role_choices(
  project: ProjectId,
) -> (
  octacity_server_factory::FactoryConfigurationChoices,
  octacity_server_factory::FactoryRoleProfileDefinition,
) {
  use octacity_server_factory::*;
  let mut definition = octacity_server_store::testing::role_profile_contract_definition(project);
  definition.execution.integrations.clear();
  definition.execution.mcp_servers.clear();
  definition.permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    executables: vec![definition.toolchain.executable.clone()],
    commands: vec![
      CommandPermission::try_new(
        definition.toolchain.executable.clone(),
        vec![CommandArgumentPattern::Exact(
          FactoryText::new("--approved-private-argument").unwrap(),
        )],
      )
      .unwrap(),
    ],
    ..Default::default()
  })
  .unwrap();
  let refs = [
    definition.toolchain.plugin.clone(),
    definition.toolchain.executable.clone(),
    definition.toolchain.model.clone(),
  ]
  .into_iter()
  .chain(definition.skills.values().cloned())
  .enumerate()
  .map(|(i, reference)| FactoryReferenceChoice {
    kind: FactoryChoiceKind::NodeCapability,
    alias: FactoryKey::new(format!("dependency-{i}")).unwrap(),
    reference,
  })
  .collect();
  let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: refs,
    build_configurations: vec![(
      FactoryKey::new("build").unwrap(),
      definition.build_configuration.clone(),
    )],
  })
  .unwrap();
  let schema = |name, fields| {
    FlowDataSchema::new(
      FactoryKey::new(name).unwrap(),
      FactoryKey::new("v1").unwrap(),
      FlowValueSchema::Object { fields },
    )
    .unwrap()
  };
  let capability = FactoryRoleProfileCapability {
    mcp_required_environment: Default::default(),
    model_catalog: None,
    ceiling: definition.clone(),
    parameters: schema(
      "parameters",
      [(
        FactoryKey::new("reasoning_effort").unwrap(),
        FlowFieldSchema::required(FlowValueSchema::Enum {
          values: vec![serde_json::json!("medium"), serde_json::json!("high")],
        }),
      )]
      .into(),
    ),
    public_environment: [(
      "LANGUAGE".into(),
      FlowDataSchema::new(
        FactoryKey::new("environment").unwrap(),
        FactoryKey::new("v1").unwrap(),
        FlowValueSchema::String {
          min_bytes: 0,
          max_bytes: 20,
        },
      )
      .unwrap(),
    )]
    .into(),
    local_permissions: LocalPermissionCeiling::fully_enforced(definition.permissions.clone()),
  };
  (
    choices
      .with_role_profile_capability(FactoryKey::new("dependency-1").unwrap(), capability)
      .unwrap(),
    definition,
  )
}

/// Installed role capabilities plus explicit policies for public preset/publication conformance.
pub fn installed_preset_choices(
  project: ProjectId,
) -> (
  octacity_server_factory::FactoryConfigurationChoices,
  octacity_server_factory::FactoryRoleProfileDefinition,
) {
  use octacity_server_factory::*;
  let (choices, definition) = installed_role_choices(project);
  let mut entries = choices.entries();
  entries.references.push(FactoryReferenceChoice {
    kind: FactoryChoiceKind::NodeCapability,
    alias: "reporter".parse().unwrap(),
    reference: ImmutableReference::new(
      "reporter".parse().unwrap(),
      "v1".parse().unwrap(),
      FactoryDigest::from_bytes([0x99; 32]),
    ),
  });
  for (kind, alias, byte) in [
    (FactoryChoiceKind::AdmissionPolicy, "admission", 21),
    (FactoryChoiceKind::PermissionCeiling, "permissions", 22),
  ] {
    entries.references.push(FactoryReferenceChoice {
      kind,
      alias: FactoryKey::new(alias).unwrap(),
      reference: ImmutableReference::new(
        FactoryKey::new(alias).unwrap(),
        FactoryKey::new("v1").unwrap(),
        FactoryDigest::from_bytes([byte; 32]),
      ),
    });
  }
  let mut installed = FactoryConfigurationChoices::try_new(entries).unwrap();
  for (alias, capability) in choices.role_profile_capabilities() {
    installed = installed
      .with_role_profile_capability(alias.clone(), capability.clone())
      .unwrap();
  }
  (installed, definition)
}

use crate::{
  ApplicationError, FactoryExecutionClock, FactoryNodeCancellationStep, FactoryNodeExecutionRequest,
  FactoryNodeExecutionStep, FactoryNodeExecutor, FactoryNodeRunner, FactoryNodeTarget, SystemFactoryExecutionClock,
};
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::{
  AuditActorKind, ClaimFactoryRun, FactoryAuditFact, FactoryRunClaimRecord, FactoryRunStore, PublishedFactoryAdmission,
  testing as store_testing,
};
/// Pre-dispatch observer: any execution would violate the pending-selection contract.
pub struct NoProfileExecution;
#[async_trait::async_trait]
impl FactoryNodeExecutor for NoProfileExecution {
  async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    panic!("profile selection cannot call a Build or provider")
  }
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    panic!("profile selection owns no external execution")
  }
}
/// Publishes and claims the same memory Run used by shared owner/control contracts.
pub async fn start_profile_run(
  admission: PublishedFactoryAdmission,
  ownership: FactoryClaimOwnership,
) -> (
  Arc<store_testing::InMemoryFactoryConfigurationStore>,
  FactoryRunClaimRecord,
) {
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "profile-owner")
    .audit()
    .clone();
  store
    .seed_factory_admission(
      admission.clone(),
      octacity_server_store::FactoryWorkSourceScope {
        source: "manual".parse().unwrap(),
        security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
      },
      FactoryDigest::from_bytes([1; 32]),
      &audit,
    )
    .unwrap();
  let claim = FactoryRunClaimRecord::new(admission.run.id(), ownership.owner().clone(), ownership.claim());
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: admission.run.id(),
      expected_version: admission.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        admission.run.id(),
        AuditActorKind::Worker,
        None,
        "factory.claimed".parse().unwrap(),
        FactoryDigest::from_bytes([1; 32]),
        "accepted".parse().unwrap(),
        claim.claim.claimed_at(),
      ),
    })
    .await
    .unwrap();
  (store, claim)
}

/// One real undispatched manual wait for HTTP command/diagnostic contracts.
pub struct PendingFactoryProfileFixture {
  /// Existing deterministic memory adapter, shared with ordinary controls and discovery.
  pub store: Arc<store_testing::InMemoryFactoryConfigurationStore>,
  /// Exact owning Run.
  pub run_id: String,
  /// Exact immutable node occurrence.
  pub input_digest: String,
  /// Pinned eligible profile references in candidate order.
  pub profiles: Vec<serde_json::Value>,
  /// Version after freezing the pending input, before any operator intent.
  pub version: u64,
  input: FlowNodeInput,
  claim: FactoryRunClaimRecord,
}
impl PendingFactoryProfileFixture {
  /// Consumes the accepted intent and commits an ordinary Build intent without executing it.
  pub async fn commit_build_intent(&self) -> u64 {
    let clock = Arc::new(SystemFactoryExecutionClock);
    let snapshot = self
      .store
      .factory_run_snapshot(self.input.factory_run_id())
      .await
      .unwrap();
    let now = clock.now().unwrap();
    let claim = FactoryRunClaimRecord::new(
      self.input.factory_run_id(),
      self.claim.owner.clone(),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([52; 32])),
        now,
        Timestamp::from_unix_millis(now.unix_millis() + 60_000).unwrap(),
      )
      .unwrap(),
    );
    self
      .store
      .claim_factory_run(ClaimFactoryRun {
        run_id: self.input.factory_run_id(),
        expected_version: snapshot.run.version(),
        record: claim.clone(),
        audit: FactoryAuditFact::new(
          self.input.factory_run_id(),
          AuditActorKind::Worker,
          None,
          "factory.claimed".parse().unwrap(),
          FactoryDigest::from_bytes([52; 32]),
          "accepted".parse().unwrap(),
          now,
        ),
      })
      .await
      .unwrap();
    let runner = FactoryNodeRunner::new(self.store.clone(), Arc::new(NoProfileExecution), clock.clone())
      .with_profile_catalog(self.store.clone());
    for _ in 0..2 {
      runner
        .run_once(
          self.input.factory_run_id(),
          claim.id,
          FactoryNodeTarget {
            flow_run_id: self.input.flow_run_id(),
            cycle_id: self.input.cycle_id(),
            node: self.input.node().clone(),
          },
          None,
          clock.now().unwrap(),
        )
        .await
        .unwrap();
    }
    self
      .store
      .factory_run_snapshot(self.input.factory_run_id())
      .await
      .unwrap()
      .run
      .version()
      .get()
  }
}
/// Uses the common Manual-mode fixture and owner, with a current trusted clock for HTTP controls.
pub async fn pending_factory_profile_fixture() -> PendingFactoryProfileFixture {
  pending_factory_profile_fixture_with_settings(|_| {}).await
}
/// Uses the same owner and journal while varying immutable runtime-declared settings.
pub(crate) async fn pending_factory_profile_fixture_with_settings(
  configure: impl Fn(&mut FactoryRoleProfileDefinition),
) -> PendingFactoryProfileFixture {
  let (admission, input, original) = store_testing::factory_profile_selection_configured_contract_fixture(configure);
  let clock = Arc::new(SystemFactoryExecutionClock);
  let now = clock.now().unwrap();
  let ownership = FactoryClaimOwnership::new(
    original.owner().clone(),
    FactoryClaim::new(
      original.claim().fence(),
      now,
      Timestamp::from_unix_millis(now.unix_millis() + 60_000).unwrap(),
    )
    .unwrap(),
  );
  let profiles = admission
    .flow
    .closure()
    .definition(input.definition())
    .unwrap()
    .node(input.node())
    .unwrap()
    .profile_selection()
    .unwrap()
    .candidates
    .values()
    .map(|candidate| serde_json::to_value(candidate.profile.reference()).unwrap())
    .collect();
  let (store, claim) = start_profile_run(admission.clone(), ownership).await;
  store.seed_project(input.subject().project_id()).unwrap();
  store
    .enable_project(
      input.subject().project_id(),
      store_testing::factory_profile_selection_capabilities(&admission.flow),
    )
    .unwrap();
  let runner = FactoryNodeRunner::new(store.clone(), Arc::new(NoProfileExecution), clock.clone());
  runner
    .run_once(
      input.factory_run_id(),
      claim.id,
      FactoryNodeTarget {
        flow_run_id: input.flow_run_id(),
        cycle_id: input.cycle_id(),
        node: input.node().clone(),
      },
      Some(input.clone()),
      clock.now().unwrap(),
    )
    .await
    .unwrap();
  let snapshot = store.factory_run_snapshot(input.factory_run_id()).await.unwrap();
  PendingFactoryProfileFixture {
    store,
    run_id: input.factory_run_id().to_string(),
    input_digest: input.digest().unwrap().to_string(),
    profiles,
    version: snapshot.run.version().get(),
    input,
    claim,
  }
}

/// Tool-using profile without installed task-plugin control qualification.
pub fn installed_tool_readiness_choices(
  project: ProjectId,
) -> (FactoryConfigurationChoices, FactoryRoleProfileDefinition) {
  let (choices, mut definition) = installed_preset_choices(project);
  let mut permissions = serde_json::to_value(&definition.permissions).unwrap();
  permissions["tools"] = serde_json::json!([definition.toolchain.executable]);
  definition.permissions = serde_json::from_value(permissions).unwrap();
  let mut installed = FactoryConfigurationChoices::try_new(choices.entries()).unwrap();
  for (alias, capability) in choices.role_profile_capabilities() {
    let mut capability = capability.clone();
    capability.ceiling = definition.clone();
    capability.local_permissions = LocalPermissionCeiling::fully_enforced(definition.permissions.clone());
    installed = installed
      .with_role_profile_capability(alias.clone(), capability)
      .unwrap();
  }
  (installed, definition)
}

/// Authoring-qualified MCP metadata without evidence of native capture or a live authenticated peer.
pub fn installed_mcp_readiness_choices(
  project: ProjectId,
) -> (FactoryConfigurationChoices, FactoryRoleProfileDefinition) {
  installed_mcp_readiness_choices_with_count(project, 1)
}
/// Multiple independently scoped connections for readiness work-bound conformance.
pub fn installed_mcp_readiness_choices_with_count(
  project: ProjectId,
  count: usize,
) -> (FactoryConfigurationChoices, FactoryRoleProfileDefinition) {
  let (choices, mut definition) = installed_preset_choices(project);
  let mut entries = choices.entries();
  let mut configurations = BTreeMap::new();
  let mut credentials = Vec::new();
  for index in 0..count {
    let name: FactoryKey = if index == 0 {
      "city".into()
    } else {
      format!("city-{index}")
    }
    .parse()
    .unwrap();
    let credential: FactoryKey = format!("city-reader-{index}").parse().unwrap();
    let environment = format!("OCTACITY_MCP_CITY_{index}");
    let configuration = FactoryMcpConfiguration::new(
      name.clone(),
      "v1".parse().unwrap(),
      FactoryMcpServer::Http {
        url: format!("https://mcp.example.test/issues/{index}"),
        bearer_token_env_var: Some(environment.clone()),
        enabled_tools: Some(vec!["search_issues".into(), "get_issue".into()]),
      },
      None,
    )
    .unwrap()
    .with_authorization(FactoryMcpAuthorization {
      credential_profile: credential.clone(),
      secret: serde_json::from_value(
        serde_json::json!({"provider":"installed","reference":format!("mcp-reader-{index}")}),
      )
      .unwrap(),
      environment: environment.clone(),
      purpose: FactoryMcpCredentialPurpose::McpRead,
      resource: ImmutableReference::new(
        "project-issues".parse().unwrap(),
        "v1".parse().unwrap(),
        FactoryDigest::from_bytes([71; 32]),
      ),
    })
    .unwrap();
    definition.execution.mcp_servers.insert(
      name.clone(),
      FactoryRoleMcpBinding {
        configuration: configuration.reference().clone(),
        integration: None,
      },
    );
    definition.execution.environment.insert(
      environment,
      FactoryRoleEnvironmentValue::Secret {
        reference: configuration.authorization().unwrap().secret.clone(),
      },
    );
    entries.references.push(FactoryReferenceChoice {
      kind: FactoryChoiceKind::NodeCapability,
      alias: format!("{name}-mcp").parse().unwrap(),
      reference: configuration.reference().clone(),
    });
    entries.references.push(FactoryReferenceChoice {
      kind: FactoryChoiceKind::SecretProfile,
      alias: credential.clone(),
      reference: ImmutableReference::new(
        credential.clone(),
        "v1".parse().unwrap(),
        FactoryDigest::from_bytes([72; 32]),
      ),
    });
    configurations.insert(name, configuration);
    credentials.push(credential);
  }
  let mut permissions = serde_json::to_value(&definition.permissions).unwrap();
  permissions["secret_profiles"] = serde_json::json!(credentials);
  definition.permissions = serde_json::from_value(permissions).unwrap();
  let mut installed = FactoryConfigurationChoices::try_new(entries).unwrap();
  for (alias, capability) in choices.role_profile_capabilities() {
    let mut capability = capability.clone();
    capability.ceiling = definition.clone();
    capability.local_permissions = LocalPermissionCeiling::fully_enforced(definition.permissions.clone());
    installed = installed
      .with_role_profile_capability(alias.clone(), capability)
      .unwrap()
      .with_runtime_settings_support(
        alias.clone(),
        FactoryRuntimeSettingsSupport {
          mcp_transports: [FactoryMcpTransport::Http].into(),
          authenticated_mcp: configurations.clone(),
          ..Default::default()
        },
      )
      .unwrap();
  }
  (installed, definition)
}

/// Metadata port that refuses any unexpected Build context read.
pub struct UnavailableReadinessContexts;
#[async_trait::async_trait]
impl crate::BuildExecutionContextSource for UnavailableReadinessContexts {
  async fn load(
    &self,
    _: ProjectId,
    _: octacity_server_domain::BuildConfigurationId,
    _: octacity_server_domain::BuildConfigurationVersion,
  ) -> Result<crate::BuildExecutionContext, crate::BuildExecutionContextError> {
    panic!("no Build context read expected")
  }
}

#[path = "testing/ordinary_context.rs"]
pub(crate) mod ordinary_context;
/// Attaches the same trusted single-job declaration used by native materializers.
pub async fn qualify_readiness_pipeline(
  store: &store_testing::InMemoryFactoryConfigurationStore,
  project: ProjectId,
  binding: serde_json::Value,
  node: &str,
) {
  qualify_pipeline_fixture(store, project, binding, node, false).await;
}
/// Adds exact compiler declarations without requiring clients to supply compilation hashes.
pub async fn qualify_preset_compilation(
  store: &store_testing::InMemoryFactoryConfigurationStore,
  project: ProjectId,
  binding: serde_json::Value,
) {
  qualify_pipeline_fixture(store, project, binding, "root", true).await;
}
async fn qualify_pipeline_fixture(
  store: &store_testing::InMemoryFactoryConfigurationStore,
  project: ProjectId,
  binding: serde_json::Value,
  node: &str,
  include_compilation: bool,
) {
  use octacity_server_store::FactoryConfigurationStore;
  let binding: FlowBuildBinding = serde_json::from_value(binding).unwrap();
  let compilation = include_compilation.then(|| FactoryBuildCompilationContract {
    task_digest: binding.task_digest,
    role_profiles: true,
    result_output: binding.result_output.clone(),
    result_output_kind: binding.result_output_kind,
    mcp_result_tool: None,
  });
  let octacity_server_store::FactoryConfigurationAvailability::Available(choices) =
    store.factory_configuration_availability(project).await.unwrap()
  else {
    panic!("installed choices")
  };
  store
    .enable_project(
      project,
      choices
        .with_build_pipeline_contract(FactoryBuildPipelineContract {
          build_configuration: binding.build_configuration,
          tool: binding.tool,
          plugin: binding.plugin,
          model_or_tool: binding.model_or_tool,
          pipeline_node: octacity_server_domain::PipelineNodeId::new(node).unwrap(),
          compilation,
        })
        .unwrap(),
    )
    .unwrap();
}
/// Exact ordinary metadata fixture shared with HTTP readiness conformance.
pub struct ReadinessExecutionFixture {
  /// Mutable external Agent observation for freshness/drain regressions.
  pub agent: Mutex<crate::AgentProjection>,
  /// Current Pool observation.
  pub pool: Mutex<crate::AgentPoolProjection>,
  context: crate::BuildExecutionContext,
}
impl ReadinessExecutionFixture {
  /// Selects an ordinary published Pipeline shape for metadata conformance cases.
  pub fn with_pipeline_nodes(mut self, names: &[&str]) -> Self {
    use octacity_server_pipeline::*;
    let nodes = names
      .iter()
      .map(|name| {
        PipelineNode::new(
          octacity_server_domain::PipelineNodeId::new(*name).unwrap(),
          octacity_server_domain::JobName::new(*name).unwrap(),
          DependencyPolicy::AllSucceeded,
          vec![],
          serde_json::json!({}),
        )
        .unwrap()
      })
      .collect();
    self.context.pipeline.dag = PublishablePipelineDag::new(nodes, vec![], &CapabilityCatalog::new([]))
      .unwrap()
      .into_dag();
    self
  }
  /// Uses the existing ordinary execution fixture and an exact selected binding.
  pub fn new(project: ProjectId, binding: serde_json::Value, repository: WorkSourceRepositoryRef) -> Self {
    use crate::*;
    use octacity_protocol::*;
    use octacity_server_domain::*;
    use octacity_server_store::*;
    let binding: FlowBuildBinding = serde_json::from_value(binding).unwrap();
    let (mut context, pool_id) = ordinary_context::context();
    context.configuration.id = binding.build_configuration.id();
    context.configuration.version = binding.build_configuration.version();
    context.configuration.project_id = project;
    context.repository.project_id = project;
    context.repository.id = repository.id;
    context.repository.version = repository.version;
    context.pipeline.project_id = project;
    let definition = &mut context.configuration.definition;
    definition.repository_id = repository.id;
    definition.repository_version = repository.version;
    definition.agent_requirements.capabilities.clear();
    definition.secrets_profile = None;
    definition.runtime.class = RuntimeClass::Host;
    let platform = PlatformSpec {
      os: definition.runtime.operating_system,
      architecture: definition.runtime.architecture,
    };
    definition.runtime.host_platform = Some(platform);
    let policy = &mut context.effective_policy.policy;
    policy.repositories = [repository.id].into();
    policy.execution_targets.insert(ProjectExecutionTarget {
      mode: ExecutionMode::Host,
      host_platform: platform,
      target_platform: platform,
      required_guarantees: Default::default(),
    });
    context.effective_policy.sources = vec![PolicySource {
      project_id: project,
      version: ProjectPolicyVersion::INITIAL,
    }];
    context.job_spec_toolchain.octa.plugin_digests = [(
      binding.plugin.identity().to_string(),
      binding.plugin.digest().to_string(),
    )]
    .into();
    let id = AgentId::generate();
    let now = Timestamp::from_unix_millis(1000).unwrap();
    let mut inventory = store_testing::compatible_inventory(id);
    inventory.labels = definition.agent_requirements.labels.clone();
    inventory.execution_contract = SUPPORTED_EXECUTION_CONTRACTS;
    let execution = ExecutionCapabilityV2 {
      provider: ExecutionProviderId::new("qualified-host").unwrap(),
      mode: ExecutionMode::Host,
      host_platform: platform,
      target_platform: platform,
      guarantees: Default::default(),
      immutable_images: false,
    };
    inventory.executions = vec![execution.clone()];
    inventory.factory_executions = vec![FactoryExecutionCapabilityV3 {
      execution: execution.clone(),
      workspace_anchor: true,
      controlled_tools: true,
      enforcement: FactoryEnforcementCapabilityV3::ALL.to_vec(),
    }];
    inventory.octa.plugins = vec![TaskPluginInventory {
      name: binding.plugin.identity().to_string(),
      version: binding.plugin.version().to_string(),
      protocol: 1,
      platforms: vec!["linux-x86_64".into()],
      sha256: binding.plugin.digest().to_string(),
      capabilities: vec!["native".into()],
    }];
    let mut agent: AgentProjection = EnrolledAgent {
      id,
      name: AgentName::new("qualified agent").unwrap(),
      version: AgentVersion::INITIAL,
      pool_id,
      pool_version: PoolVersion::INITIAL,
      inventory,
      status: AgentStatus::Online,
      enabled: true,
      last_seen_at: now,
      created_at: now,
      updated_at: now,
    }
    .into();
    agent.resources = Some(AgentResources {
      registration_expires_at: Some(Timestamp::from_unix_millis(61000).unwrap()),
      backends: Some(vec![BackendHealth {
        backend: "qualified-host".into(),
        execution: Some(execution),
        status: BackendHealthStatus::Ready,
        message: None,
      }]),
      observed_at: now,
      available_cpu_millis: 2000,
      available_memory_bytes: 4096,
      work_disk_free_bytes: 8192,
      state_disk_free_bytes: 8192,
    });
    let pool = AgentPoolProjection {
      id: pool_id,
      name: "qualified pool".into(),
      version: PoolVersion::INITIAL,
      enabled: true,
      drain_state: AgentPoolDrainStateProjection::Accepting,
      admission_policy: AgentPoolAdmissionPolicyProjection::Any,
      concurrency_limit: 2,
      fairness_policy: AgentPoolFairnessPolicyProjection::PriorityFifo,
      static_capacity_limit: 2,
      published_at: now,
    };
    Self {
      agent: Mutex::new(agent),
      pool: Mutex::new(pool),
      context,
    }
    .with_pipeline_nodes(&["root"])
  }
}
#[async_trait::async_trait]
impl crate::BuildExecutionContextSource for ReadinessExecutionFixture {
  async fn load(
    &self,
    project: ProjectId,
    id: octacity_server_domain::BuildConfigurationId,
    version: octacity_server_domain::BuildConfigurationVersion,
  ) -> Result<crate::BuildExecutionContext, crate::BuildExecutionContextError> {
    assert_eq!(
      (project, id, version),
      (
        self.context.configuration.project_id,
        self.context.configuration.id,
        self.context.configuration.version
      )
    );
    Ok(self.context.clone())
  }
}
#[async_trait::async_trait]
impl crate::ManagementQueryUseCase<crate::ListAgentsQuery> for ReadinessExecutionFixture {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &crate::ManagementRequestContext,
    _: &crate::ManagementAuthorizationGrant,
    _: crate::ListAgentsQuery,
  ) -> Result<crate::AgentPageProjection, ApplicationError> {
    let mut agent = self.agent.lock().unwrap().clone();
    agent.resources = None;
    Ok(crate::AgentPageProjection {
      agents: vec![agent],
      next_cursor: None,
      active_count: 1,
      total_count: 1,
    })
  }
}
#[async_trait::async_trait]
impl crate::ManagementQueryUseCase<crate::GetAgentQuery> for ReadinessExecutionFixture {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &crate::ManagementRequestContext,
    _: &crate::ManagementAuthorizationGrant,
    _: crate::GetAgentQuery,
  ) -> Result<crate::AgentProjection, ApplicationError> {
    Ok(self.agent.lock().unwrap().clone())
  }
}
#[async_trait::async_trait]
impl crate::ManagementQueryUseCase<crate::GetAgentPoolQuery> for ReadinessExecutionFixture {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &crate::ManagementRequestContext,
    _: &crate::ManagementAuthorizationGrant,
    _: crate::GetAgentPoolQuery,
  ) -> Result<crate::AgentPoolProjection, ApplicationError> {
    Ok(self.pool.lock().unwrap().clone())
  }
}

/// Expands legal retained profile versions to exercise shared metadata work limits.
pub fn expand_readiness_profiles(draft: &mut serde_json::Value, count: usize) {
  let mut flow: FactoryFlowConfiguration = serde_json::from_value(draft["flow"].clone()).unwrap();
  assert_eq!(flow.closure.definitions().len(), 1);
  let old = &flow.closure.definitions()[0];
  let mut nodes = Vec::new();
  for node in old.nodes() {
    let mut wire = serde_json::to_value(node).unwrap();
    if let Some(candidates) = wire
      .pointer_mut("/profile_selection/candidates")
      .and_then(serde_json::Value::as_object_mut)
    {
      let original = candidates["selected"].clone();
      for version in 2..=count {
        let mut candidate = original.clone();
        candidate["profile"]["reference"]["version"] = serde_json::json!(version);
        candidates.insert(format!("alternative-{version}"), candidate);
      }
    }
    nodes.push(serde_json::from_value(wire).unwrap());
  }
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: old.reference().id(),
    version: old.reference().version(),
    input_schema: old.input_schema().cloned(),
    entry: old.entry().clone(),
    nodes,
    transitions: old.transitions().to_vec(),
    terminals: old.terminals().to_vec(),
    context_projections: old.context_projections().to_vec(),
    data_projections: old.data_projections().to_vec(),
    execution: old.execution().clone(),
  })
  .unwrap();
  flow.closure = PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap();
  draft["flow"] = serde_json::to_value(flow).unwrap();
}
