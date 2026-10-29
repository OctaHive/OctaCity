//! Optional configurable owner and explicit deployment installation boundary.
use async_trait::async_trait;
use octacity_server_application::{FactoryFlowInputPreparer, FactoryNodeExecutorBinding, JobSpecToolchainPolicy};
use octacity_server_store_postgres::{FactoryCapabilitySource, PostgresAuthoritativeStore, PostgresStore};
use std::sync::Arc;

/// Runtime-owned resources supplied to trusted installed capability compilers.
/// No task-authored routing or mutable provider aliases select an installation.
pub struct FactoryRuntimeResources {
  /// Common durable Run, configuration, input and result journal.
  pub runs: Arc<PostgresStore>,
  /// Ordinary atomic signed Build execution and cancellation.
  pub builds: Arc<PostgresAuthoritativeStore>,
  /// Bounded retained protected inputs and ordinary execution outputs.
  pub objects: Arc<octacity_artifact_s3::S3ArtifactStore>,
  /// Exact server-approved source and Octa toolchain snapshot.
  pub toolchain: JobSpecToolchainPolicy,
  /// Shared installed VCS resolver; Source and manual admission use the same policy.
  pub revisions: Arc<dyn octacity_server_application::RevisionResolver>,
}

impl FactoryRuntimeResources {
  /// Reuses bounded connector timing and the native Flow journal for pre-dispatch profile selection.
  pub fn profile_selectors(
    &self,
    mut profiles: Vec<octacity_server_application::FactoryProfileSelector>,
  ) -> Result<Arc<dyn octacity_server_application::FactoryProfileSelectionDriver>, FactoryRuntimeInstallationError> {
    let clock = Arc::new(octacity_server_application::SystemFactoryExecutionClock);
    for profile in &mut profiles {
      profile
        .validate()
        .map_err(|_| FactoryRuntimeInstallationError::Invalid)?;
      profile.provider = Arc::new(super::factory_signal_provider::BoundedFactorySignalProvider::new(
        profile.provider.clone(),
        clock.clone(),
      ));
    }
    let selectors = octacity_server_application::FactoryProfileSelectors::new(self.runs.clone(), profiles, clock)
      .map_err(|_| FactoryRuntimeInstallationError::Invalid)?;
    Ok(Arc::new(selectors))
  }
  /// Installs local Issue effects over the common action executor and transition command.
  /// The host supplies accepted attribution, current policy and the shared execution clock;
  /// neither provider configuration nor a Reporter is required.
  pub fn issue_status_executor(
    &self,
    identity: octacity_server_factory::ImmutableReference,
    context: octacity_server_application::ManagementRequestContext,
    policy: Arc<dyn octacity_server_application::ManagementAuthorizationPolicy>,
    clock: Arc<dyn octacity_server_application::FactoryExecutionClock>,
  ) -> FactoryNodeExecutorBinding {
    use octacity_server_application::{FactoryFlowActionAdapter, FactoryIssueStatusAction};
    let action = FactoryIssueStatusAction::new(
      identity.clone(),
      self.runs.clone(),
      self.runs.clone(),
      context,
      policy,
      clock,
    );
    (
      octacity_server_factory::FlowNodeKind::TrustedAction,
      octacity_server_factory::NodeExecutionIdentity::External(identity),
      Arc::new(FactoryFlowActionAdapter::new(Arc::new(action))),
    )
  }
  /// Supplies ordinary outputs and Project-owned inputs to every native compiler.
  pub fn retained_artifact_reader(
    &self,
  ) -> Arc<
    octacity_server_application::FactoryArtifactReader<
      PostgresStore,
      PostgresAuthoritativeStore,
      octacity_artifact_s3::S3ArtifactStore,
    >,
  > {
    Arc::new(
      octacity_server_application::FactoryArtifactReader::new(
        self.runs.clone(),
        self.builds.clone(),
        self.objects.clone(),
      )
      .with_project_inputs(self.runs.clone()),
    )
  }
  /// Shares the same provider-independent publication service with every text ingress.
  pub fn project_text_intake(
    &self,
    authorization: Arc<dyn octacity_server_application::ManagementAuthorizationPolicy>,
  ) -> octacity_server_application::ProjectTextIntake<PostgresStore, octacity_artifact_s3::S3ArtifactStore> {
    octacity_server_application::ProjectTextIntake::new(
      self.runs.clone(),
      self.objects.clone(),
      self.revisions.clone(),
      authorization,
      Arc::new(octacity_server_application::SystemFactoryExecutionClock),
    )
  }
  /// Explicitly installs provider-neutral verified Reporter profiles through the existing outbox.
  /// Construction performs no dependency I/O; manual intake and ordinary Builds require no profiles.
  pub fn work_reporter_dispatcher(
    &self,
    profiles: Vec<super::FactoryReporterProfile>,
    registry: Arc<octacity_server_work_source::WorkAdapterRegistry>,
    settings: octacity_server_application::FactoryReporterWorkerSettings,
    timeout: std::time::Duration,
    grace: std::time::Duration,
    cancellation: tokio_util::sync::CancellationToken,
  ) -> Result<Arc<dyn octacity_server_application::FactoryOutboxDispatcher>, FactoryRuntimeInstallationError> {
    let capabilities = profiles
      .iter()
      .map(|p| p.binding().capability.clone())
      .collect::<std::collections::BTreeSet<_>>();
    let projector = super::HostedFactoryWorkStatusProjector::new(
      self.runs.clone(),
      registry,
      profiles,
      Arc::new(octacity_server_application::SystemFactoryExecutionClock),
      timeout,
      grace,
      cancellation,
    )
    .map_err(|_| FactoryRuntimeInstallationError::Invalid)?;
    let projector = projector.with_managed_connections(
      octacity_server_application::ManagedIntegrationConnections::new(self.runs.clone()),
    );
    let projector = Arc::new(projector) as Arc<dyn octacity_server_application::FactoryWorkStatusProjector>;
    self.work_status_dispatcher(
      capabilities.into_iter().map(|key| (key, projector.clone())).collect(),
      settings,
    )
  }
  /// Opts in to provider-neutral polling using native journals, private objects and ordinary admission.
  /// Construction does not contact a provider or resolve credentials.
  pub fn source_worker(
    &self,
    installation: super::FactorySourceInstallation,
    registry: Arc<octacity_server_work_source::WorkAdapterRegistry>,
    authorization: Arc<dyn octacity_server_application::ManagementAuthorizationPolicy>,
  ) -> Result<Arc<dyn super::FactorySourceRuntime>, FactoryRuntimeInstallationError> {
    super::FactorySourceWorker::new(
      super::FactorySourceWorkerDependencies {
        store: self.runs.clone(),
        bytes: self.objects.clone(),
        project_inputs: Some(Arc::new(octacity_server_application::FactoryProjectInputReader::new(
          self.runs.clone(),
          self.objects.clone(),
        ))),
        revisions: self.revisions.clone(),
        authorization,
        registry,
        clock: Arc::new(octacity_server_application::SystemFactoryExecutionClock),
      },
      installation,
    )
    .map(|worker| {
      Arc::new(
        worker.with_managed_connections(octacity_server_application::ManagedIntegrationConnections::new(
          self.runs.clone(),
        )),
      ) as Arc<dyn super::FactorySourceRuntime>
    })
    .map_err(|_| FactoryRuntimeInstallationError::Invalid)
  }

  /// Installs a content-pinned deterministic review join over the native journal.
  /// Construction performs no evaluator or external-service I/O.
  pub fn evaluation_decision_executor(
    &self,
    identity: octacity_server_factory::ImmutableReference,
    profile: octacity_server_application::FactoryEvaluationDecisionProfile,
  ) -> Result<FactoryNodeExecutorBinding, FactoryRuntimeInstallationError> {
    use octacity_server_application::{
      FactoryEvaluationDecisionAction, FactoryFlowActionAdapter, SystemFactoryExecutionClock,
    };
    if identity.digest() != profile.digest().map_err(|_| FactoryRuntimeInstallationError::Invalid)? {
      return Err(FactoryRuntimeInstallationError::Invalid);
    }
    let action = FactoryEvaluationDecisionAction::new(
      identity.clone(),
      self.runs.clone(),
      profile,
      Arc::new(SystemFactoryExecutionClock),
    );
    Ok((
      octacity_server_factory::FlowNodeKind::TrustedAction,
      octacity_server_factory::NodeExecutionIdentity::External(identity),
      Arc::new(FactoryFlowActionAdapter::new(Arc::new(action))),
    ))
  }

  /// Binds private installed Delivery input directories to native retained
  /// Artifact ownership and byte verification. Credentials remain connector-owned.
  pub fn delivery_materializer(
    &self,
    directories: std::collections::BTreeMap<octacity_server_factory::ImmutableReference, std::path::PathBuf>,
  ) -> Result<Arc<dyn super::FactoryDeliveryMaterializer>, FactoryRuntimeInstallationError> {
    let reader = self.retained_artifact_reader();
    super::RetainedFactoryDeliveryMaterializer::new(
      reader,
      directories,
      Arc::new(octacity_server_application::SystemFactoryExecutionClock),
    )
    .map(|material| Arc::new(material) as Arc<dyn super::FactoryDeliveryMaterializer>)
  }

  /// Binds qualified promotion to ordinary Build creation, observation and
  /// cancellation. Installed deployment compilers retain provider ownership;
  /// constructing the binding performs no dependency I/O.
  pub fn promotion_executor<O: octacity_server_application::FactoryNodeBuildOutputSource + 'static>(
    &self,
    identity: octacity_server_factory::ImmutableReference,
    compiler: Arc<dyn octacity_server_application::FactoryBuildMaterializer>,
    outputs: Arc<O>,
  ) -> FactoryNodeExecutorBinding {
    use octacity_server_application::{
      FactoryNodeBuildAdapter, FactoryOrdinaryBuildService, FactoryPromotionBuildAdapter, SystemFactoryExecutionClock,
    };
    let builds = Arc::new(FactoryOrdinaryBuildService::new(
      self.builds.clone(),
      self.runs.clone(),
      compiler,
      Arc::new(SystemFactoryExecutionClock),
    ));
    let executor = FactoryPromotionBuildAdapter::new(identity.clone(), FactoryNodeBuildAdapter::new(builds, outputs));
    (
      octacity_server_factory::FlowNodeKind::TrustedAction,
      octacity_server_factory::NodeExecutionIdentity::External(identity),
      Arc::new(executor),
    )
  }

  /// Binds installed non-authoritative status projectors to the native outbox
  /// journal and trusted clock. Construction performs no external dispatch.
  pub fn work_status_dispatcher(
    &self,
    projectors: std::collections::BTreeMap<
      octacity_server_factory::ImmutableReference,
      Arc<dyn octacity_server_application::FactoryWorkStatusProjector>,
    >,
    settings: octacity_server_application::FactoryReporterWorkerSettings,
  ) -> Result<Arc<dyn octacity_server_application::FactoryOutboxDispatcher>, FactoryRuntimeInstallationError> {
    use octacity_server_application::{FactoryWorkStatusOutboxWorker, SystemFactoryExecutionClock};
    FactoryWorkStatusOutboxWorker::new(
      self.runs.clone(),
      projectors,
      settings,
      Arc::new(SystemFactoryExecutionClock),
    )
    .map(|worker| Arc::new(worker) as Arc<dyn octacity_server_application::FactoryOutboxDispatcher>)
    .map_err(|_| FactoryRuntimeInstallationError::Invalid)
  }

  /// Composes installed tool-risk connectors behind native Build, Job, lease and
  /// Run authority. Provider credentials remain owned by the connector; its
  /// immutable call window is enforced by the runtime before receipt settlement.
  pub fn tool_risk_authorizer(
    &self,
    mut profiles: Vec<octacity_server_application::FactoryToolRiskNodeProfile>,
  ) -> Result<Arc<dyn octacity_server_application::LeaseBoundToolActionAuthorizer>, FactoryRuntimeInstallationError> {
    use octacity_server_application::{FactoryToolRiskAuthorizer, SystemFactoryExecutionClock};
    let clock = Arc::new(SystemFactoryExecutionClock);
    for profile in &mut profiles {
      if profile.binding.project_id != profile.build_configuration.project_id()
        || !signal_network_is_qualified(profile.provider.as_ref(), &profile.binding.network_host)
      {
        return Err(FactoryRuntimeInstallationError::Invalid);
      }
      profile.provider = Arc::new(super::factory_signal_provider::BoundedFactorySignalProvider::new(
        profile.provider.clone(),
        clock.clone(),
      ));
    }
    FactoryToolRiskAuthorizer::new(
      self.runs.clone(),
      self.runs.clone(),
      self.builds.clone(),
      self.builds.clone(),
      self.runs.clone(),
      profiles,
      clock,
    )
    .map(|authorizer| Arc::new(authorizer) as Arc<dyn octacity_server_application::LeaseBoundToolActionAuthorizer>)
    .map_err(|_| FactoryRuntimeInstallationError::Invalid)
  }

  /// Wires exact installed routing actions to native fenced request and receipt
  /// storage. Provider construction and credential resolution belong to the
  /// trusted installer; creating this binding performs no external I/O.
  pub fn decision_signal_executor(
    &self,
    mut profiles: Vec<octacity_server_application::FactoryDecisionSignalNodeProfile>,
  ) -> Result<FactoryNodeExecutorBinding, FactoryRuntimeInstallationError> {
    use octacity_server_application::{FactoryDecisionSignalNodeExecutor, SystemFactoryExecutionClock};
    use octacity_server_factory::{FlowNodeKind, NodeExecutionIdentity, resolve_factory_permissions};
    let adapter = profiles
      .first()
      .ok_or(FactoryRuntimeInstallationError::Invalid)?
      .provider
      .capability()
      .adapter()
      .clone();
    for profile in &profiles {
      let binding = &profile.binding;
      let permissions = &binding.project_permissions;
      if binding.action.plugin() != &adapter
        || !signal_network_is_qualified(profile.provider.as_ref(), &binding.network_host)
        || profile.provider.capability().adapter() != &adapter
        || !permissions.plugins().any(|plugin| plugin == &adapter)
        || !permissions
          .secret_profiles()
          .any(|profile| profile == &binding.credential_profile)
        || !permissions.network_hosts().any(|host| host == &binding.network_host)
        || !resolve_factory_permissions(permissions, permissions, permissions, &binding.local_permissions)
          .is_ok_and(|effective| &effective == permissions)
      {
        return Err(FactoryRuntimeInstallationError::Invalid);
      }
    }
    let identity = NodeExecutionIdentity::External(adapter);
    let clock = Arc::new(SystemFactoryExecutionClock);
    for profile in &mut profiles {
      profile.provider = Arc::new(super::factory_signal_provider::BoundedFactorySignalProvider::new(
        profile.provider.clone(),
        clock.clone(),
      ));
    }
    let executor = FactoryDecisionSignalNodeExecutor::new(self.runs.clone(), profiles, clock)
      .map_err(|_| FactoryRuntimeInstallationError::Invalid)?;
    Ok((FlowNodeKind::DecisionSignal, identity, Arc::new(executor)))
  }

  /// Wires qualified Codex review or bounded authoring to native ordinary Build creation,
  /// immutable context reads, protected input publication and cancellation.
  /// Retained output verification uses the same installed profile catalog; this never probes dependencies
  /// or interprets logical Flow/node names while creating the binding.
  pub fn codex_executor(
    &self,
    profiles: Vec<octacity_server_application::FactoryCodexBuildProfile>,
  ) -> Result<FactoryNodeExecutorBinding, FactoryRuntimeInstallationError> {
    use octacity_server_application::*;
    let executable = profiles
      .first()
      .ok_or(FactoryRuntimeInstallationError::Invalid)?
      .toolchain
      .executable
      .clone();
    for profile in &profiles {
      if profile.toolchain.executable != executable {
        return Err(FactoryRuntimeInstallationError::Invalid);
      }
      profile
        .validate_installation(&self.toolchain)
        .map_err(|_| FactoryRuntimeInstallationError::Invalid)?;
    }
    let identity = octacity_server_factory::NodeExecutionIdentity::External(executable);
    let clock = Arc::new(SystemFactoryExecutionClock);
    let policies = Arc::new(StoreBackedEffectiveProjectPolicySource::new(self.runs.clone()));
    let contexts = Arc::new(StoreBackedBuildExecutionContext::new(
      self.runs.clone(),
      self.runs.clone(),
      policies,
      self.toolchain.clone(),
    ));
    let reader = self.retained_artifact_reader();
    let publisher = Arc::new(FactoryExecutionInputPublisher::new(
      self.runs.clone(),
      self.objects.clone(),
      clock.clone(),
    ));
    let review_invocations = Arc::new(FactoryStoredReviewInvocationSource::new(self.runs.clone()));
    // One trusted profile selection owns both dispatch preparation and retained
    // observation. Candidate source pins come only from installed operator policy.
    let readonly_profiles = profiles
      .iter()
      .filter(|profile| profile.consumer == octacity_server_factory::FactoryCredentialConsumer::ReadOnlyReview)
      .cloned()
      .collect::<Vec<_>>();
    let candidate_contexts = if readonly_profiles.is_empty() {
      None
    } else {
      Some(Arc::new(
        FactoryCodexCandidateContextSource::new(
          self.runs.clone(),
          Arc::new(FactoryCandidateContextSource::new(reader.clone())),
          readonly_profiles,
          clock.clone(),
        )
        .map_err(|_| FactoryRuntimeInstallationError::Invalid)?,
      ))
    };
    let mut outputs = FactoryCodexBuildOutputSource::new(
      self.runs.clone(),
      self.builds.clone(),
      self.runs.clone(),
      self.objects.clone(),
      profiles.clone(),
      reader.clone(),
      clock.clone(),
    )
    .map_err(|_| FactoryRuntimeInstallationError::Invalid)?
    .with_role_invocation_store(self.runs.clone())
    .with_runtime_settings_store(self.runs.clone())
    .with_review_invocation_source(review_invocations.clone());
    let role_invocations = Arc::new(FactoryStoredCodexRoleInvocationSource::new(
      self.runs.clone(),
      self.objects.clone(),
      reader.clone(),
      profiles.clone(),
    ));
    let mut compiler = FactoryCodexBuildMaterializer::new(contexts, publisher, profiles.clone(), reader, clock.clone())
      .map_err(|_| FactoryRuntimeInstallationError::Invalid)?
      .with_review_invocation_source(review_invocations.clone())
      .with_runtime_settings_store(self.runs.clone())
      .with_role_invocation_source(role_invocations);
    if let Some(candidate_contexts) = candidate_contexts {
      outputs = outputs.with_retained_candidate_context_source(candidate_contexts.clone());
      compiler = compiler.with_candidate_context_source(candidate_contexts);
    }
    let outputs = Arc::new(outputs);
    let compiler: Arc<dyn FactoryBuildMaterializer> = Arc::new(compiler);
    let builds = Arc::new(FactoryOrdinaryBuildService::new(
      self.builds.clone(),
      self.runs.clone(),
      compiler,
      clock,
    ));
    Ok((
      octacity_server_factory::FlowNodeKind::Reasoning,
      identity,
      Arc::new(NativeCodexNodeExecutor {
        builds,
        outputs,
        profiles,
        review_invocations,
      }),
    ))
  }
}

/// Dispatches by the exact frozen installed profile; models sharing one CLI can
/// independently have no capture or distinct trusted capture implementations.
pub(super) struct NativeCodexNodeExecutor<B, O, R> {
  pub(super) builds: Arc<B>,
  pub(super) outputs: Arc<O>,
  pub(super) profiles: Vec<octacity_server_application::FactoryCodexBuildProfile>,
  pub(super) review_invocations: Arc<R>,
}
impl<B, O, R> NativeCodexNodeExecutor<B, O, R> {
  pub(super) fn profile(
    &self,
    intent: &octacity_server_factory::FlowBuildIntent,
    admitted: &octacity_server_factory::AdmittedFlow,
  ) -> Result<&octacity_server_application::FactoryCodexBuildProfile, octacity_server_application::ApplicationError> {
    use octacity_server_application::ApplicationError;
    intent
      .verify_admission(admitted)
      .map_err(|_| ApplicationError::invalid())?;
    // The admitted candidate is immutable; dispatch never resolves current catalog heads.
    let selected = intent
      .selected_profile()
      .map(|selected| {
        admitted
          .closure()
          .definition(intent.input().definition())
          .and_then(|definition| definition.node(intent.input().node()))
          .and_then(|node| node.profile_selection())
          .and_then(|selection| {
            selection
              .candidates
              .values()
              .find(|candidate| candidate.profile.reference() == selected)
          })
          .map(|candidate| &candidate.profile)
          .ok_or_else(ApplicationError::invalid)
      })
      .transpose()?;
    let mut matches = self
      .profiles
      .iter()
      .filter(|profile| profile.matches_role_intent(intent, selected));
    let profile = matches
      .next()
      .ok_or_else(octacity_server_application::ApplicationError::invalid)?;
    if matches.next().is_some() {
      return Err(octacity_server_application::ApplicationError::invalid());
    }
    Ok(profile)
  }
}
#[async_trait]
impl<B, O, R> octacity_server_application::FactoryNodeExecutor for NativeCodexNodeExecutor<B, O, R>
where
  B: octacity_server_application::OrdinaryBuildCancellationApplication + octacity_server_store::BuildQueryStore,
  O: octacity_server_application::FactoryNodeBuildOutputSource,
  R: octacity_server_application::FactoryReviewInvocationSource + 'static,
{
  async fn observe(
    &self,
    request: octacity_server_application::FactoryNodeExecutionRequest<'_>,
  ) -> Result<octacity_server_application::FactoryNodeExecutionStep, octacity_server_application::ApplicationError> {
    use octacity_server_application::*;
    let intent = request.build.ok_or_else(ApplicationError::invalid)?;
    let profile = self.profile(intent, request.admitted)?;
    match (&profile.capture, &profile.reviewer) {
      (Some(capture), None) => {
        FactoryNodeCaptureBuildAdapter::new(self.builds.clone(), self.outputs.clone(), capture.tool().clone())
          .observe(request)
          .await
      }
      (None, Some(reviewer)) => {
        FactoryCodexReviewBuildAdapter::new(
          self.builds.clone(),
          self.outputs.clone(),
          self.review_invocations.clone(),
          reviewer.evaluator.clone(),
        )
        .observe(request)
        .await
      }
      (None, None) => {
        FactoryNodeBuildAdapter::new(self.builds.clone(), self.outputs.clone())
          .observe(request)
          .await
      }
      (Some(_), Some(_)) => Err(ApplicationError::invalid()),
    }
  }
  async fn cancel(
    &self,
    request: octacity_server_application::FactoryNodeExecutionRequest<'_>,
    at: octacity_server_domain::Timestamp,
  ) -> Result<octacity_server_application::FactoryNodeCancellationStep, octacity_server_application::ApplicationError>
  {
    use octacity_server_application::*;
    // Cancellation stops the known ordinary execution even when its historical
    // profile is no longer installed; it never creates capture authority.
    FactoryNodeExecutor::cancel(
      &FactoryNodeBuildAdapter::new(self.builds.clone(), self.outputs.clone()),
      request,
      at,
    )
    .await
  }
}

fn signal_network_is_qualified(
  provider: &dyn octacity_server_factory::DecisionSignalProvider,
  permitted_host: &octacity_server_factory::NetworkHost,
) -> bool {
  provider.network_destinations().is_some_and(|hosts| {
    hosts.len() <= octacity_server_application::MAX_FACTORY_EXECUTOR_BINDINGS
      && hosts.iter().all(|host| host == permitted_host)
  })
}

/// Required availability of installed Factory capabilities, evaluated only in enabled mode.
#[async_trait]
pub trait FactoryRuntimeReadiness: Send + Sync {
  /// Reports availability without disclosing provider data or credentials.
  async fn available(&self) -> bool;
}

/// Verified deployment components for the common graph owner.
/// Executors are selected by exact immutable identities, never logical node names.
pub struct FactoryRuntimeInstallation {
  /// Current Project-scoped choices used for new configuration publication.
  pub capabilities: Arc<dyn FactoryCapabilitySource>,
  /// Unique exact installed bindings for effectful primitives.
  pub executors: Vec<FactoryNodeExecutorBinding>,
  /// Trusted owner of configured projections, selected Context and retained bytes.
  pub inputs: Arc<dyn FactoryFlowInputPreparer>,
  /// Availability of the dependencies required by this deployment.
  pub readiness: Arc<dyn FactoryRuntimeReadiness>,
  /// Optional independent deferred-effect dispatchers; source failures cannot drive the owner.
  pub outbox_dispatchers: Vec<Arc<dyn octacity_server_application::FactoryOutboxDispatcher>>,
  /// Optional exact server-owned policy for current lease-bound actions.
  pub tool_authorizer: Option<Arc<dyn octacity_server_application::LeaseBoundToolActionAuthorizer>>,
  /// Optional server-derived publication under the actual current Job Lease.
  pub output_finalizer: Option<Arc<dyn octacity_server_application::AgentOutputFinalizer>>,
  /// Optional Source polling, isolated from manual admission and capability readiness.
  pub work_sources: Option<Arc<dyn super::FactorySourceRuntime>>,
  /// Optional purpose-specific consumer over the shared provider bridge.
  pub profile_selectors: Option<Arc<dyn octacity_server_application::FactoryProfileSelectionDriver>>,
}

/// Secret-safe deployment installation failure.
#[derive(Debug, thiserror::Error)]
pub enum FactoryRuntimeInstallationError {
  /// A binding or protected execution policy cannot be verified.
  #[error("Factory installation is invalid")]
  Invalid,
  /// Required installed material is not currently available.
  #[error("Factory installation is unavailable")]
  Unavailable,
}

/// Explicit deployment extension seam for trusted capability implementations.
/// Ordinary startup never constructs or probes an installer when Factory is disabled.
#[async_trait]
pub trait FactoryRuntimeInstaller: Send + Sync {
  /// Resolves exact capability bindings from runtime-owned resources.
  async fn install(
    &self,
    resources: FactoryRuntimeResources,
  ) -> Result<FactoryRuntimeInstallation, FactoryRuntimeInstallationError>;
}

pub(super) struct CapabilityReadiness(pub(super) Arc<dyn FactoryRuntimeReadiness>);
#[async_trait]
impl crate::readiness::ReadinessCheck for CapabilityReadiness {
  fn name(&self) -> &'static str {
    "factory-capabilities"
  }
  async fn check(&self) -> bool {
    self.0.available().await
  }
}

struct UnavailableInstallation;
#[async_trait]
impl crate::readiness::ReadinessCheck for UnavailableInstallation {
  fn name(&self) -> &'static str {
    "factory-installation"
  }
  async fn check(&self) -> bool {
    false
  }
}

/// Worker plus health tracked by the existing durable supervisor.
pub(super) struct InstalledFactoryWorker {
  pub(super) owner: octacity_server_application::FactoryReconciler<PostgresStore>,
  pub(super) health: Arc<crate::readiness::WorkerHealth>,
  pub(super) outbox: Vec<InstalledFactoryOutbox>,
  pub(super) work_sources: Option<InstalledFactorySource>,
  pub(super) tool_authorizer: Arc<dyn octacity_server_application::LeaseBoundToolActionAuthorizer>,
  pub(super) output_finalizer: Option<Arc<dyn octacity_server_application::AgentOutputFinalizer>>,
}

/// One deferred-effect worker shares its health with the scoped readiness monitor.
pub(super) struct InstalledFactoryOutbox {
  pub(super) dispatcher: Arc<dyn octacity_server_application::FactoryOutboxDispatcher>,
  pub(super) health: Arc<crate::readiness::WorkerHealth>,
}

/// A Source consumer shares one health signal with scoped readiness and supervision.
pub(super) struct InstalledFactorySource {
  pub(super) worker: Arc<dyn super::FactorySourceRuntime>,
  pub(super) health: Arc<crate::readiness::WorkerHealth>,
}

pub(super) fn install_source(
  worker: Arc<dyn super::FactorySourceRuntime>,
  readiness: &mut crate::readiness::ReadinessChecks,
) -> InstalledFactorySource {
  let policy = worker.settings().worker;
  let health = Arc::new(crate::readiness::WorkerHealth::new(
    "factory-source",
    policy.poll_interval().saturating_add(policy.claim_lifetime()),
  ));
  readiness.push_factory(health.clone());
  InstalledFactorySource { worker, health }
}

pub(super) fn install_outbox(
  dispatchers: Vec<Arc<dyn octacity_server_application::FactoryOutboxDispatcher>>,
  policy: crate::FactoryRuntimeConfig,
  readiness: &mut crate::readiness::ReadinessChecks,
) -> Vec<InstalledFactoryOutbox> {
  dispatchers
    .into_iter()
    .map(|dispatcher| {
      let health = Arc::new(crate::readiness::WorkerHealth::new(
        "factory-outbox",
        policy.poll_interval().saturating_add(policy.claim_lifetime()),
      ));
      readiness.push_factory(health.clone());
      InstalledFactoryOutbox { dispatcher, health }
    })
    .collect()
}

pub(super) async fn prepare(
  config: &crate::ServerConfig,
  resources: &mut super::assembly::RuntimeResources,
  placement_store: Arc<PostgresAuthoritativeStore>,
  installer: Option<Arc<dyn FactoryRuntimeInstaller>>,
  revisions: Arc<dyn octacity_server_application::RevisionResolver>,
) -> Result<Option<FactoryRuntimeInstallation>, super::ServerRuntimeError> {
  use super::ServerRuntimeError;
  let installed = match (config.factory(), installer) {
    (Some(_), Some(installer)) => match tokio::time::timeout(
      config.readiness_check_timeout(),
      installer.install(FactoryRuntimeResources {
        runs: Arc::new(PostgresStore::new(resources.postgres.clone())),
        builds: placement_store.clone(),
        objects: resources.object_storage.clone(),
        toolchain: resources.job_spec_toolchain.clone(),
        revisions,
      }),
    )
    .await
    {
      Ok(Ok(installed)) => Some(installed),
      Ok(Err(FactoryRuntimeInstallationError::Unavailable)) | Err(_) => {
        resources.readiness.push_factory(Arc::new(UnavailableInstallation));
        None
      }
      Ok(Err(FactoryRuntimeInstallationError::Invalid)) => return Err(ServerRuntimeError::InvalidFactoryInstallation),
    },
    (None, _) => None,
    (Some(_), None) => unreachable!("installation requirement checked before loading resources"),
  };
  Ok(installed)
}

pub(super) fn assemble(
  config: &crate::ServerConfig,
  resources: &mut super::assembly::RuntimeResources,
  installed: Option<FactoryRuntimeInstallation>,
) -> Result<(Arc<PostgresStore>, Option<InstalledFactoryWorker>), super::ServerRuntimeError> {
  use super::ServerRuntimeError;
  use crate::readiness::WorkerHealth;
  let registration_store = Arc::new(match &installed {
    Some(installed) => {
      PostgresStore::with_factory_capabilities(resources.postgres.clone(), installed.capabilities.clone())
    }
    None => PostgresStore::new(resources.postgres.clone()),
  });
  let factory = match (config.factory(), installed) {
    (Some(policy), Some(installed)) => {
      let health = Arc::new(WorkerHealth::new(
        super::workers::FACTORY_WORKER_NAME,
        policy
          .poll_interval()
          .saturating_add(policy.claim_lifetime())
          .saturating_add(config.readiness_check_interval()),
      ));
      resources.readiness.push_factory(health.clone());
      resources
        .readiness
        .push_factory(Arc::new(CapabilityReadiness(installed.readiness)));
      let capabilities = installed
        .executors
        .iter()
        .filter_map(|(_, identity, _)| match identity {
          octacity_server_factory::NodeExecutionIdentity::External(reference) => Some(reference.clone()),
          octacity_server_factory::NodeExecutionIdentity::BuiltIn => None,
        })
        .collect();
      let phase_selector = Arc::new(
        octacity_server_application::FactoryPhasePoolSelector::new(
          registration_store.clone(),
          capabilities,
          policy.batch_size(),
        )
        .map_err(|_| ServerRuntimeError::InvalidFactoryInstallation)?,
      );
      let executors = Arc::new(
        octacity_server_application::FactoryNodeExecutorRegistry::new(installed.executors)
          .map_err(|_| ServerRuntimeError::InvalidFactoryInstallation)?,
      );
      let mut flow = octacity_server_application::FactoryFlowRunner::new(
        registration_store.clone(),
        executors,
        installed.inputs,
        Arc::new(octacity_server_application::SystemFactoryExecutionClock),
      )
      .with_profile_catalog(registration_store.clone());
      if let Some(selectors) = installed.profile_selectors {
        flow = flow.with_profile_selectors(selectors);
      }
      let flow = Arc::new(flow);
      let worker = octacity_server_application::FactoryReconciler::new(
        registration_store.clone(),
        octacity_server_factory::FactoryKey::new(format!("factory-owner.{}", uuid::Uuid::new_v4()))
          .map_err(|_| ServerRuntimeError::InvalidFactoryInstallation)?,
        policy.batch_size(),
        policy.concurrency(),
        flow,
      )
      .map_err(|_| ServerRuntimeError::InvalidFactoryInstallation)?
      .with_phase_pool_selector(phase_selector);
      Some(InstalledFactoryWorker {
        owner: worker,
        health,
        outbox: install_outbox(installed.outbox_dispatchers, policy, &mut resources.readiness),
        output_finalizer: installed.output_finalizer,
        work_sources: installed
          .work_sources
          .map(|worker| install_source(worker, &mut resources.readiness)),
        tool_authorizer: installed
          .tool_authorizer
          .unwrap_or_else(|| Arc::new(octacity_server_application::FailClosedToolActionAuthorizer)),
      })
    }
    (_, None) => None,
    _ => unreachable!("Factory policy and installation are paired"),
  };
  Ok((registration_store, factory))
}

impl FactoryRuntimeResources {
  /// Binds evidence publication to the original retained candidate, test plan and
  /// completed ordinary validation Build. All policy comes from the installed
  /// profile; construction validates metadata without probing dependencies.
  pub fn protected_validation_evidence_executor(
    &self,
    identity: octacity_server_factory::ImmutableReference,
    profile: octacity_server_application::FactoryProtectedValidationEvidenceProfile,
    author_profile: octacity_server_factory::ImmutableReference,
    policy: octacity_server_application::FactoryProtectedTestPolicy,
    inventory: octacity_server_factory::EvidenceRequirement,
    capture_tool: octacity_server_factory::ImmutableReference,
  ) -> Result<FactoryNodeExecutorBinding, FactoryRuntimeInstallationError> {
    use octacity_server_application::*;
    use octacity_server_factory::{EvidenceOutputKind, FlowActionBinding, FlowNodeKind, NodeExecutionIdentity};
    if inventory.output_kind() != EvidenceOutputKind::Artifact {
      return Err(FactoryRuntimeInstallationError::Invalid);
    }
    let clock = Arc::new(SystemFactoryExecutionClock);
    let reader = self.retained_artifact_reader();
    let inputs = self.protected_validation_inputs(super::FactoryRuntimeProtectedValidationProfile {
      author_profile,
      policy,
      inventory,
      capture_tool,
    })?;
    let binding = FlowActionBinding::new(identity.clone(), profile.action.clone(), serde_json::json!({}))
      .map_err(|_| FactoryRuntimeInstallationError::Invalid)?;
    let outcome = profile.ready.clone();
    let action = FactoryProtectedValidationEvidenceAction::new(
      identity.clone(),
      self.runs.clone(),
      reader,
      inputs,
      profile,
      clock,
    );
    action
      .validate(&binding, &[outcome])
      .map_err(|_| FactoryRuntimeInstallationError::Invalid)?;
    Ok((
      FlowNodeKind::TrustedAction,
      NodeExecutionIdentity::External(identity),
      Arc::new(FactoryFlowActionAdapter::new(Arc::new(action))),
    ))
  }
}
