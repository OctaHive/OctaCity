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
