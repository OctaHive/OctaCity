//! Native ordinary command Build ports and exact retained validation inputs.
use super::{FactoryRuntimeInstallationError, FactoryRuntimeResources};
use async_trait::async_trait;
use octacity_artifact_s3::S3ArtifactStore;
use octacity_server_application::*;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
use std::sync::Arc;

type NativeReader = FactoryArtifactReader<PostgresStore, PostgresAuthoritativeStore, S3ArtifactStore>;
type NativeCandidate =
  FactoryStoredCandidateContextSource<PostgresStore, PostgresStore, PostgresAuthoritativeStore, S3ArtifactStore>;
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
  Author(Vec<FactoryProtectedTestAuthoringProfile>),
  Deployment(Vec<FactoryDeploymentCommandProfile>),
}
impl CommandInstallation {
  fn profiles(&self) -> Vec<FactoryCommandBuildProfile> {
    match self {
      Self::Ordinary(profiles) | Self::Promotion(profiles) | Self::Validation { profiles, .. } => profiles.clone(),
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

  pub(super) fn protected_test_output_finalizer(
    &self,
    profiles: Vec<FactoryProtectedTestAuthoringProfile>,
  ) -> Result<Arc<dyn AgentOutputFinalizer>, FactoryRuntimeInstallationError> {
    let clock = Arc::new(SystemFactoryExecutionClock);
    let commands = profiles.iter().map(|profile| profile.command().clone()).collect();
    let candidates = Arc::new(CommandCandidates {
      runs: self.runs.clone(),
      reader: Arc::new(FactoryArtifactReader::new(
        self.runs.clone(),
        self.builds.clone(),
        self.objects.clone(),
      )),
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

  fn command_ports(
    &self,
    installation: CommandInstallation,
  ) -> Result<(Arc<NativeCommandBuilds>, Arc<NativeCommandOutputs>), FactoryRuntimeInstallationError> {
    use FactoryRuntimeInstallationError::Invalid;
    let profiles = installation.profiles();
    let executor = &profiles.first().ok_or(Invalid)?.executor;
    for profile in &profiles {
      if &profile.executor != executor {
        return Err(Invalid);
      }
      if !matches!(&installation, CommandInstallation::Deployment(_)) {
        profile.validate_installation(&self.toolchain).map_err(|_| Invalid)?;
      }
    }
    let clock = Arc::new(SystemFactoryExecutionClock);
    let reader = Arc::new(FactoryArtifactReader::new(
      self.runs.clone(),
      self.builds.clone(),
      self.objects.clone(),
    ));
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
    let mut compiler = compiler.with_candidate_context_source(candidates.clone());
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

  pub(super) fn protected_validation_inputs(
    &self,
    profile: FactoryRuntimeProtectedValidationProfile,
  ) -> Result<Arc<NativeValidationInputs>, FactoryRuntimeInstallationError> {
    if profile.inventory.output_kind() != EvidenceOutputKind::Artifact {
      return Err(FactoryRuntimeInstallationError::Invalid);
    }
    let clock = Arc::new(SystemFactoryExecutionClock);
    let reader = Arc::new(FactoryArtifactReader::new(
      self.runs.clone(),
      self.builds.clone(),
      self.objects.clone(),
    ));
    let plans = Arc::new(FactoryStoredProtectedTestPlanSource::new(
      self.runs.clone(),
      profile.author_profile,
      profile.policy,
    ));
    let candidates = Arc::new(FactoryStoredCandidateContextSource::new(
      self.runs.clone(),
      Arc::new(FactoryCandidateContextSource::new(reader.clone())),
      profile.capture_tool.clone(),
      clock.clone(),
    ));
    let tests = Arc::new(FactoryStoredProtectedTestContextSource::new(
      self.runs.clone(),
      reader.clone(),
      plans,
      profile.inventory,
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
}
