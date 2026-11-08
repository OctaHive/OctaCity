//! Native ordinary command Build ports and exact retained validation inputs.
use crate::runtime::{FactoryRuntimeInstallationError, FactoryRuntimeResources};
use async_trait::async_trait;
use octacity_artifact_s3::S3ArtifactStore;
use octacity_server_application::*;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
use std::{collections::BTreeMap, sync::Arc};

type NativeReader = FactoryArtifactReader<PostgresStore, PostgresAuthoritativeStore, S3ArtifactStore>;
type NativeCandidate =
  FactoryStoredCandidateContextSource<PostgresStore, PostgresStore, PostgresAuthoritativeStore, S3ArtifactStore>;
type NativeProtectedTestInputs = FactoryStoredProtectedTestContextSource<
  PostgresStore,
  PostgresStore,
  PostgresAuthoritativeStore,
  S3ArtifactStore,
  FactoryStoredProtectedTestPlanSource<PostgresStore>,
>;
type NativeValidationInputs = FactoryStoredProtectedValidationInputSource<
  PostgresStore,
  PostgresStore,
  PostgresAuthoritativeStore,
  S3ArtifactStore,
  NativeCandidate,
  FactoryStoredProtectedTestPlanSource<PostgresStore>,
>;
type NativeCommandBuilds =
  FactoryOrdinaryBuildService<PostgresAuthoritativeStore, PostgresStore, dyn FactoryBuildMaterializer>;
type NativeCommandOutputs =
  FactoryCommandBuildOutputSource<PostgresStore, PostgresAuthoritativeStore, PostgresStore, S3ArtifactStore>;

/// Exact retained producer contracts for an independently installed verifier.
#[derive(Clone)]
pub struct FactoryRuntimeProtectedValidationProfile {
  /// Exact author profile whose protected test plan is accepted.
  pub author_profile: ImmutableReference,
  /// Frozen coverage, lineage and resource limits.
  pub policy: FactoryProtectedTestPolicy,
  /// Independently installed Artifact inventory contract.
  pub inventory: EvidenceRequirement,
  /// Installed command supports the frozen supplemental inventory.
  pub accepts_supplement: bool,
  /// Trusted capture implementation accepted for the original candidate.
  pub capture_tool: ImmutableReference,
}

enum CommandInstallation {
  Ordinary(Vec<FactoryCommandBuildProfile>),
  Promotion(Vec<FactoryCommandBuildProfile>),
  Validation {
    profiles: Vec<FactoryCommandBuildProfile>,
    inputs: Arc<NativeValidationInputs>,
  },
  TestObservation {
    profiles: Vec<FactoryCommandBuildProfile>,
    inputs: Arc<NativeProtectedTestInputs>,
  },
  Author(Vec<FactoryProtectedTestAuthoringProfile>),
  Deployment(Vec<FactoryDeploymentCommandProfile>),
}
impl CommandInstallation {
  fn profiles(&self) -> Vec<FactoryCommandBuildProfile> {
    match self {
      Self::Ordinary(profiles)
      | Self::Promotion(profiles)
      | Self::Validation { profiles, .. }
      | Self::TestObservation { profiles, .. } => profiles.clone(),
      Self::Author(profiles) => profiles.iter().map(|profile| profile.command().clone()).collect(),
      Self::Deployment(profiles) => profiles.iter().map(|profile| profile.command().clone()).collect(),
    }
  }
}

struct CommandCandidates {
  runs: Arc<PostgresStore>,
  reader: Arc<NativeReader>,
  profiles: Vec<FactoryCommandBuildProfile>,
}
impl CommandCandidates {
  fn selected(&self, intent: &FlowBuildIntent) -> Result<NativeCandidate, ApplicationError> {
    let mut matching = self.profiles.iter().filter(|profile| profile.matches_intent(intent));
    let profile = matching.next().ok_or_else(ApplicationError::invalid)?;
    if matching.next().is_some() {
      return Err(ApplicationError::invalid());
    }
    let capture_tool = profile
      .candidate_capture_tool
      .clone()
      .ok_or_else(ApplicationError::invalid)?;
    Ok(FactoryStoredCandidateContextSource::new(
      self.runs.clone(),
      Arc::new(FactoryCandidateContextSource::new(self.reader.clone())),
      capture_tool,
      Arc::new(SystemFactoryExecutionClock),
    ))
  }
}
#[async_trait]
impl FactoryNodeCandidateContextSource for CommandCandidates {
  async fn read_candidate_context(
    &self,
    intent: &FlowBuildIntent,
    ownership: &FactoryClaimOwnership,
    observed_at: Timestamp,
    required_until: Timestamp,
  ) -> Result<FactoryCandidateContext, ApplicationError> {
    self
      .selected(intent)?
      .read_candidate_context(intent, ownership, observed_at, required_until)
      .await
  }
}
#[async_trait]
impl FactoryRetainedCandidateContextSource for CommandCandidates {
  async fn read_retained_candidate_context(
    &self,
    intent: &FlowBuildIntent,
    observed_at: Timestamp,
    required_until: Timestamp,
  ) -> Result<FactoryCandidateContext, ApplicationError> {
    self
      .selected(intent)?
      .read_retained_candidate_context(intent, observed_at, required_until)
      .await
  }
}

/// Commands sharing an executor can still require different materializers and
/// output verifiers. Select both from the frozen installed command policy.
#[derive(Default)]
pub(super) struct FileCommandExecutors(
  BTreeMap<ImmutableReference, BTreeMap<FactoryDigest, Arc<dyn FactoryNodeExecutor>>>,
);

impl FileCommandExecutors {
  pub(super) fn add(
    &mut self,
    profiles: &[FactoryCommandBuildProfile],
    binding: FactoryNodeExecutorBinding,
  ) -> Result<(), FactoryRuntimeInstallationError> {
    let (FlowNodeKind::BuildCommand, NodeExecutionIdentity::External(identity), executor) = binding else {
      return Err(FactoryRuntimeInstallationError::Invalid);
    };
    let by_policy = self.0.entry(identity).or_default();
    for profile in profiles {
      if by_policy.insert(profile.task_digest(), executor.clone()).is_some() {
        return Err(FactoryRuntimeInstallationError::Invalid);
      }
    }
    Ok(())
  }

  pub(super) fn bindings(self) -> impl Iterator<Item = FactoryNodeExecutorBinding> {
    self.0.into_iter().map(|(identity, by_policy)| {
      let executor: Arc<dyn FactoryNodeExecutor> = Arc::new(InstalledCommandExecutor(by_policy));
      (
        FlowNodeKind::BuildCommand,
        NodeExecutionIdentity::External(identity),
        executor,
      )
    })
  }
}

struct InstalledCommandExecutor(BTreeMap<FactoryDigest, Arc<dyn FactoryNodeExecutor>>);

impl InstalledCommandExecutor {
  fn select(&self, request: &FactoryNodeExecutionRequest<'_>) -> Result<&dyn FactoryNodeExecutor, ApplicationError> {
    request
      .build
      .and_then(|intent| self.0.get(&intent.profile().task_digest))
      .map(Arc::as_ref)
      .ok_or_else(ApplicationError::invalid)
  }
}

#[async_trait]
impl FactoryNodeExecutor for InstalledCommandExecutor {
  async fn scoped_validation(
    &self,
    action: FactoryScopedValidationCall,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryScopedValidationVerdict, ApplicationError> {
    self.select(&request)?.scoped_validation(action, request).await
  }

  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    self.select(&request)?.observe(request).await
  }

  async fn cancel(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
    requested_at: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    self.select(&request)?.cancel(request, requested_at).await
  }
}

impl FactoryRuntimeResources {
  /// Installs exact static command profiles against ordinary native Build ports.
  /// No storage/provider probe or command executes while constructing the binding.
  pub fn command_executor(
    &self,
    profiles: Vec<FactoryCommandBuildProfile>,
  ) -> Result<FactoryNodeExecutorBinding, FactoryRuntimeInstallationError> {
    let executor = profiles
      .first()
      .ok_or(FactoryRuntimeInstallationError::Invalid)?
      .executor
      .clone();
    let (builds, outputs) = self.command_ports(CommandInstallation::Ordinary(profiles))?;
    Ok((
      FlowNodeKind::BuildCommand,
      NodeExecutionIdentity::External(executor),
      Arc::new(FactoryNodeBuildAdapter::new(builds, outputs)),
    ))
  }

  /// Installs readonly protected-test authors through the existing command Job
  /// pipeline and the independently verified private bundle/handoff contract.
  pub fn protected_test_authoring_command_executor(
    &self,
    profiles: Vec<FactoryProtectedTestAuthoringProfile>,
  ) -> Result<FactoryNodeExecutorBinding, FactoryRuntimeInstallationError> {
    use FactoryRuntimeInstallationError::Invalid;
    let identity = profiles.first().ok_or(Invalid)?.command().executor.clone();
    let (builds, outputs) = self.command_ports(CommandInstallation::Author(profiles.clone()))?;
    let actor =
      FactoryProtectedTestAuthoringExecutor::new(builds, outputs, self.runs.clone(), profiles).map_err(|_| Invalid)?;
    Ok((
      FlowNodeKind::Reasoning,
      NodeExecutionIdentity::External(identity),
      Arc::new(actor),
    ))
  }

  pub(in crate::runtime) fn protected_test_output_finalizer(
    &self,
    profiles: Vec<FactoryProtectedTestAuthoringProfile>,
    codex_profiles: Vec<FactoryCodexBuildProfile>,
  ) -> Result<Arc<dyn AgentOutputFinalizer>, FactoryRuntimeInstallationError> {
    let clock = Arc::new(SystemFactoryExecutionClock);
    let commands = profiles.iter().map(|profile| profile.command().clone()).collect();
    let candidates = Arc::new(CommandCandidates {
      runs: self.runs.clone(),
      reader: self.retained_artifact_reader(),
      profiles: commands,
    });
    Ok(Arc::new(
      FactoryProtectedTestOutputFinalizer::new(
        self.runs.clone(),
        self.builds.clone(),
        self.runs.clone(),
        self.objects.clone(),
        profiles,
        clock,
      )
      .map_err(|_| FactoryRuntimeInstallationError::Invalid)?
      .with_codex_profiles(codex_profiles, self.runs.clone())
      .map_err(|_| FactoryRuntimeInstallationError::Invalid)?
      .with_verification_inputs(Arc::new(FactoryProjectInputReader::new(
        self.runs.clone(),
        self.objects.clone(),
      )))
      .with_retained_candidate_context_source(candidates),
    ))
  }

  /// Wraps the ordinary command Build ports in exact promotion qualification.
  pub fn promotion_command_executor(
    &self,
    identity: ImmutableReference,
    profiles: Vec<FactoryCommandBuildProfile>,
  ) -> Result<FactoryNodeExecutorBinding, FactoryRuntimeInstallationError> {
    if profiles.iter().any(|profile| {
      profile.executor != identity || profile.plugin != identity || profile.candidate_capture_tool.is_none()
    }) {
      return Err(FactoryRuntimeInstallationError::Invalid);
    }
    let (builds, outputs) = self.command_ports(CommandInstallation::Promotion(profiles))?;
    Ok((
      FlowNodeKind::TrustedAction,
      NodeExecutionIdentity::External(identity.clone()),
      Arc::new(FactoryPromotionBuildAdapter::new(
        identity,
        FactoryNodeBuildAdapter::new(builds, outputs),
      )),
    ))
  }

  /// Installs an explicitly pinned external target through ordinary promotion
  /// Jobs. Both dispatch and observation share the same approved qualification.
  pub fn deployment_command_executor(
    &self,
    identity: ImmutableReference,
    profiles: Vec<FactoryDeploymentCommandProfile>,
  ) -> Result<FactoryNodeExecutorBinding, FactoryRuntimeInstallationError> {
    if profiles.iter().any(|profile| {
      let command = profile.command();
      command.executor != identity || command.plugin != identity || command.candidate_capture_tool.is_none()
    }) {
      return Err(FactoryRuntimeInstallationError::Invalid);
    }
    let (builds, outputs) = self.command_ports(CommandInstallation::Deployment(profiles))?;
    Ok((
      FlowNodeKind::TrustedAction,
      NodeExecutionIdentity::External(identity.clone()),
      Arc::new(FactoryPromotionBuildAdapter::new(
        identity,
        FactoryNodeBuildAdapter::new(builds, outputs),
      )),
    ))
  }

  /// Installs independent validation against the original candidate and private
  /// test source. Reports must be distinct exact outputs of every installed profile.
  pub fn protected_validation_command_executor(
    &self,
    profiles: Vec<FactoryCommandBuildProfile>,
    validation: FactoryRuntimeProtectedValidationProfile,
    report: EvidenceRequirement,
    facts: EvidenceRequirement,
  ) -> Result<FactoryNodeExecutorBinding, FactoryRuntimeInstallationError> {
    use FactoryRuntimeInstallationError::Invalid;
    if report.output_kind() != EvidenceOutputKind::Report
      || facts.output_kind() != EvidenceOutputKind::Report
      || report.kind() == facts.kind()
      || profiles.iter().any(|profile| {
        profile.candidate_capture_tool.as_ref() != Some(&validation.capture_tool)
          || !profile.outputs.contains(&report)
          || !profile.outputs.contains(&facts)
      })
    {
      return Err(Invalid);
    }
    let executor = profiles.first().ok_or(Invalid)?.executor.clone();
    let inputs = self.protected_validation_inputs(validation)?;
    let (builds, outputs) = self.command_ports(CommandInstallation::Validation {
      profiles,
      inputs: inputs.clone(),
    })?;
    Ok((
      FlowNodeKind::BuildCommand,
      NodeExecutionIdentity::External(executor),
      Arc::new(FactoryProtectedValidationAdapter::new(
        builds, outputs, inputs, report, facts,
      )),
    ))
  }

  /// Installs qualified observations on the Work base with the exact private author bundle.
  pub(in crate::runtime) fn protected_test_observation_executor(
    &self,
    profiles: Vec<FactoryCommandBuildProfile>,
    author_profile: ImmutableReference,
    policy: FactoryProtectedTestPolicy,
    inventory: EvidenceRequirement,
    accepts_supplement: bool,
  ) -> Result<FactoryNodeExecutorBinding, FactoryRuntimeInstallationError> {
    use FactoryRuntimeInstallationError::Invalid;
    let executor = profiles.first().ok_or(Invalid)?.executor.clone();
    let inputs = self.protected_test_context(author_profile, policy, inventory, accepts_supplement)?;
    let (builds, outputs) = self.command_ports(CommandInstallation::TestObservation { profiles, inputs })?;
    Ok((
      FlowNodeKind::BuildCommand,
      NodeExecutionIdentity::External(executor),
      Arc::new(FactoryNodeBuildAdapter::new(builds, outputs)),
    ))
  }

  fn command_ports(
    &self,
    installation: CommandInstallation,
  ) -> Result<(Arc<NativeCommandBuilds>, Arc<NativeCommandOutputs>), FactoryRuntimeInstallationError> {
    use FactoryRuntimeInstallationError::Invalid;
    let profiles = installation.profiles();
    let executor = &profiles.first().ok_or(Invalid)?.executor;
    for profile in &profiles {
      if &profile.executor != executor
        || profile.requires_protected_tests() != matches!(&installation, CommandInstallation::TestObservation { .. })
      {
        return Err(Invalid);
      }
      if !matches!(&installation, CommandInstallation::Deployment(_)) {
        profile.validate_installation(&self.toolchain).map_err(|_| Invalid)?;
      }
    }
    let clock = Arc::new(SystemFactoryExecutionClock);
    let reader = self.retained_artifact_reader();
    let candidates = Arc::new(CommandCandidates {
      runs: self.runs.clone(),
      reader,
      profiles: profiles.clone(),
    });
    let contexts = Arc::new(StoreBackedBuildExecutionContext::new(
      self.runs.clone(),
      self.runs.clone(),
      Arc::new(StoreBackedEffectiveProjectPolicySource::new(self.runs.clone())),
      self.toolchain.clone(),
    ));
    let publisher = Arc::new(FactoryExecutionInputPublisher::new(
      self.runs.clone(),
      self.objects.clone(),
      clock.clone(),
    ));
    let (compiler, outputs) = match &installation {
      CommandInstallation::Deployment(deployment) => (
        FactoryCommandBuildMaterializer::for_deployment(
          contexts,
          publisher,
          deployment.clone(),
          self.toolchain.clone(),
          clock.clone(),
        )
        .map_err(|_| Invalid)?,
        FactoryCommandBuildOutputSource::for_deployment(
          self.runs.clone(),
          self.builds.clone(),
          self.runs.clone(),
          self.objects.clone(),
          deployment.clone(),
          self.toolchain.clone(),
          clock.clone(),
        )
        .map_err(|_| Invalid)?,
      ),
      _ => (
        FactoryCommandBuildMaterializer::new(contexts, publisher, profiles.clone(), clock.clone())
          .map_err(|_| Invalid)?,
        FactoryCommandBuildOutputSource::new(
          self.runs.clone(),
          self.builds.clone(),
          self.runs.clone(),
          self.objects.clone(),
          profiles,
          clock.clone(),
        )
        .map_err(|_| Invalid)?,
      ),
    };
    let mut compiler = compiler
      .with_candidate_context_source(candidates.clone())
      .with_work_input_store(self.runs.clone());
    let mut outputs = outputs.with_retained_candidate_context_source(candidates);
    match installation {
      CommandInstallation::Author(authors) => {
        outputs = outputs.with_protected_test_authoring(authors).map_err(|_| Invalid)?;
      }
      CommandInstallation::Promotion(_) => {
        compiler = compiler.with_promotion();
        outputs = outputs.with_promotion();
      }
      CommandInstallation::Validation { inputs, .. } => {
        compiler = compiler.with_protected_validation_input_source(inputs.clone());
        outputs = outputs.with_protected_validation_input_source(inputs);
      }
      CommandInstallation::TestObservation { inputs, .. } => {
        compiler = compiler.with_protected_test_observation_input_source(inputs.clone());
        outputs = outputs.with_protected_test_observation_input_source(inputs);
      }
      CommandInstallation::Ordinary(_) | CommandInstallation::Deployment(_) => {}
    }
    let compiler: Arc<dyn FactoryBuildMaterializer> = Arc::new(compiler);
    Ok((
      Arc::new(FactoryOrdinaryBuildService::new(
        self.builds.clone(),
        self.runs.clone(),
        compiler,
        clock,
      )),
      Arc::new(outputs),
    ))
  }

  pub(in crate::runtime) fn protected_validation_inputs(
    &self,
    profile: FactoryRuntimeProtectedValidationProfile,
  ) -> Result<Arc<NativeValidationInputs>, FactoryRuntimeInstallationError> {
    let tests = self.protected_test_context(
      profile.author_profile,
      profile.policy,
      profile.inventory,
      profile.accepts_supplement,
    )?;
    let clock = Arc::new(SystemFactoryExecutionClock);
    let reader = self.retained_artifact_reader();
    let candidates = Arc::new(FactoryStoredCandidateContextSource::new(
      self.runs.clone(),
      Arc::new(FactoryCandidateContextSource::new(reader.clone())),
      profile.capture_tool.clone(),
      clock.clone(),
    ));
    Ok(Arc::new(FactoryStoredProtectedValidationInputSource::new(
      self.runs.clone(),
      candidates,
      tests,
      reader,
      profile.capture_tool,
      clock,
    )))
  }
  fn protected_test_context(
    &self,
    author_profile: ImmutableReference,
    policy: FactoryProtectedTestPolicy,
    inventory: EvidenceRequirement,
    accepts_supplement: bool,
  ) -> Result<Arc<NativeProtectedTestInputs>, FactoryRuntimeInstallationError> {
    if inventory.output_kind() != EvidenceOutputKind::Artifact {
      return Err(FactoryRuntimeInstallationError::Invalid);
    }
    Ok(Arc::new(
      FactoryStoredProtectedTestContextSource::new(
        self.runs.clone(),
        self.retained_artifact_reader(),
        Arc::new(FactoryStoredProtectedTestPlanSource::new(
          self.runs.clone(),
          author_profile,
          policy,
        )),
        inventory,
        Arc::new(SystemFactoryExecutionClock),
      )
      .with_verification_supplement(accepts_supplement),
    ))
  }
}
