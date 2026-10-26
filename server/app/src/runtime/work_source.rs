//! Optional provider-neutral source worker composition; no forge SDK or manual-ingress dependency.
use octacity_artifact_store::ArtifactBytePublisher;
use octacity_server_application::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
use octacity_server_store::*;
use octacity_server_work_source::WorkAdapterRegistry;
use octacity_work_source_protocol::{ConfigurationReference, ResourceAccess};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[path = "work_source_admission.rs"]
mod admission;
#[path = "work_source_process.rs"]
mod process;
use admission::SourceCandidateBridge;
use process::SourcePageBridge;

/// Runtime-owned ports; credentials remain opaque, resolved only by the verified plugin.
pub struct FactorySourceWorkerDependencies<S, B> {
  /// Existing authoritative source/admission journal.
  pub store: Arc<S>,
  /// Private immutable input publication.
  pub bytes: Arc<B>,
  /// Existing revision resolution; replay precedes this port.
  pub revisions: Arc<dyn RevisionResolver>,
  /// Explicit installation-owner management authorization policy.
  pub authorization: Arc<dyn ManagementAuthorizationPolicy>,
  /// Content-verified source executable inventory.
  pub registry: Arc<WorkAdapterRegistry>,
  /// Trusted execution time.
  pub clock: Arc<dyn FactoryExecutionClock>,
}
/// Independently bounded provider execution under the existing Factory worker policy.
#[derive(Clone, Copy)]
pub struct FactorySourceWorkerSettings {
  /// Polling cadence, backoff, supervisor circuit, batch and concurrency ceilings.
  pub worker: crate::FactoryRuntimeConfig,
  /// Positive per-target failure budget, independent of the supervisor's circuit cooldown.
  pub max_attempts: u16,
  /// Optional exact retained-target retry policy; None preserves explicit legacy worker bounds.
  pub target_retry: Option<DurableRetryPolicy>,
  /// One original provider operation's absolute deadline.
  pub operation_timeout: Duration,
  /// Time for verified process cancellation and tree cleanup.
  pub cancellation_grace: Duration,
}
#[path = "work_source_read_profile.rs"]
mod read_profile;
pub use read_profile::WorkSourceReadProfile;

/// Installed Source read scope plus the separate choices needed only for Work preparation.
#[derive(Clone)]
pub struct FactorySourceProfile {
  /// Shared installed read scope, independently usable without Factory admission settings.
  pub read: WorkSourceReadProfile,
  /// Explicit immutable input mapping, code source policy and Work classification.
  pub preparation: SourcePreparationSettings,
}
impl FactorySourceProfile {
  fn for_definition(
    mut self,
    definition: &WorkSourceInputDraft,
    connections: Option<&ManagedIntegrationConnections>,
  ) -> Result<Self, ApplicationError> {
    self.read = self.read.for_connection(definition.connection.as_ref(), connections)?;
    Ok(self)
  }
  /// Adopts current managed read grants without changing Source/Work identity.
  pub fn with_managed_access(mut self, access: ManagedIntegrationAccess) -> Self {
    self.read = self.read.with_managed_access(access);
    self
  }
}
/// Explicit deployment opt-in. An absent installation starts no source worker.
pub struct FactorySourceInstallation {
  /// Projects authorized for bounded discovery, including unavailable bindings.
  pub projects: Vec<ProjectId>,
  /// Exact supported bindings; absence is an input-local unavailable outcome.
  pub profiles: Vec<FactorySourceProfile>,
  /// Explicit worker ceilings.
  pub settings: FactorySourceWorkerSettings,
}
/// Safe pass counters. Provider text and credentials are never diagnostics.
#[derive(Default, Debug, Eq, PartialEq)]
pub struct FactorySourceWorkerOutcome {
  /// Source targets actually claimed.
  pub claimed: u16,
  /// Targets whose finite polling slice completed.
  pub completed: u16,
  /// Targets durably deferred or blocked after an isolated failure.
  pub deferred: u16,
}
/// Live installed discovery and exact profiles, resolved from the same trusted authoring inventory.
#[async_trait::async_trait]
pub(super) trait FactorySourceProfileProvider: Send + Sync {
  async fn targets(&self, limit: u16) -> Result<Vec<(ProjectId, WorkSourceScanTarget)>, ApplicationError>;
  async fn profiles(&self, members: &[WorkSourceInput]) -> Result<Vec<FactorySourceProfile>, ApplicationError>;
}
/// Bounded worker over the existing source journal and ordinary admission.
pub struct FactorySourceWorker<S, B> {
  dependencies: Arc<FactorySourceWorkerDependencies<S, B>>,
  installation: Arc<FactorySourceInstallation>,
  discovery: tokio::sync::Mutex<(usize, Option<WorkSourceScanTarget>)>,
  managed_connections: Option<ManagedIntegrationConnections>,
  profile_provider: Option<Arc<dyn FactorySourceProfileProvider>>,
}
impl<S: WorkSourceArtifactStore + 'static, B: ArtifactBytePublisher + 'static> FactorySourceWorker<S, B> {
  /// Constructs explicit source support without fetching tokens or contacting a provider.
  pub fn new(
    dependencies: FactorySourceWorkerDependencies<S, B>,
    installation: FactorySourceInstallation,
  ) -> Result<Self, ApplicationError> {
    let settings = installation.settings;
    settings.worker.validate().map_err(|_| ApplicationError::invalid())?;
    if settings.max_attempts == 0
      || settings.operation_timeout.is_zero()
      || settings
        .operation_timeout
        .saturating_add(settings.cancellation_grace.saturating_mul(2))
        >= settings.worker.claim_lifetime()
      || settings.worker.batch_size() > 64
      || installation.projects.len() > 64
      || installation.profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || installation
        .projects
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != installation.projects.len()
    {
      return Err(ApplicationError::invalid());
    }
    for (index, profile) in installation.profiles.iter().enumerate() {
      profile
        .preparation
        .mapping
        .validate()
        .map_err(|_| ApplicationError::invalid())?;
      if !installation.projects.contains(&profile.read.project_id)
        || profile.preparation.mapping.source != profile.read.binding
        || installation.profiles[..index]
          .iter()
          .any(|p| p.read.project_id == profile.read.project_id && p.read.binding == profile.read.binding)
      {
        return Err(ApplicationError::invalid());
      }
    }
    Ok(Self {
      dependencies: Arc::new(dependencies),
      installation: Arc::new(installation),
      discovery: tokio::sync::Mutex::new((0, None)),
      managed_connections: None,
      profile_provider: None,
    })
  }
  /// Enables current connection resolution for explicitly managed inputs; legacy profiles stay explicit.
  pub fn with_managed_connections(mut self, connections: ManagedIntegrationConnections) -> Self {
    self.managed_connections = Some(connections);
    self
  }
  pub(super) fn with_profile_provider(mut self, provider: Arc<dyn FactorySourceProfileProvider>) -> Self {
    self.profile_provider = Some(provider);
    self
  }
  /// Runs one finite slice; absence or pre-cancellation performs no provider activity.
  pub async fn run_once(
    &self,
    cancellation: CancellationToken,
  ) -> Result<FactorySourceWorkerOutcome, ApplicationError> {
    let mut outcome = FactorySourceWorkerOutcome::default();
    if cancellation.is_cancelled() {
      return Ok(outcome);
    }
    let settings = self.installation.settings;
    if self.profile_provider.is_none() && self.installation.projects.is_empty() {
      return Ok(outcome);
    }
    let targets = if let Some(provider) = &self.profile_provider {
      provider.targets(settings.worker.batch_size()).await?
    } else {
      let mut cursor = self.discovery.lock().await;
      let project = self.installation.projects[cursor.0];
      let targets = self
        .dependencies
        .store
        .source_scan_targets(project, cursor.1, settings.worker.batch_size())
        .await?;
      if targets.len() == usize::from(settings.worker.batch_size()) {
        cursor.1 = targets.last().copied();
      } else {
        cursor.0 = (cursor.0 + 1) % self.installation.projects.len();
        cursor.1 = None;
      }
      targets.into_iter().map(|target| (project, target)).collect::<Vec<_>>()
    };
    for chunk in targets.chunks(usize::from(settings.worker.concurrency())) {
      if cancellation.is_cancelled() {
        break;
      }
      let mut tasks = tokio::task::JoinSet::new();
      for (project, target) in chunk {
        let d = self.dependencies.clone();
        let installation = self.installation.clone();
        let cancellation = cancellation.clone();
        let project = *project;
        let target = *target;
        let connections = self.managed_connections.clone();
        let profiles = self.profile_provider.clone();
        tasks.spawn(
          async move { poll_target(d, installation, project, target, cancellation, connections, profiles).await },
        );
      }
      while let Some(result) = tasks.join_next().await {
        let one = result.map_err(|_| ApplicationError::unavailable())??;
        outcome.claimed += one.claimed;
        outcome.completed += one.completed;
        outcome.deferred += one.deferred;
      }
    }
    Ok(outcome)
  }
}

async fn poll_target<S: WorkSourceArtifactStore + 'static, B: ArtifactBytePublisher + 'static>(
  d: Arc<FactorySourceWorkerDependencies<S, B>>,
  installation: Arc<FactorySourceInstallation>,
  project: ProjectId,
  target: WorkSourceScanTarget,
  cancellation: CancellationToken,
  managed_connections: Option<ManagedIntegrationConnections>,
  profile_provider: Option<Arc<dyn FactorySourceProfileProvider>>,
) -> Result<FactorySourceWorkerOutcome, ApplicationError> {
  let mut outcome = FactorySourceWorkerOutcome::default();
  let settings = installation.settings;
  if cancellation.is_cancelled() {
    return Ok(outcome);
  }
  let now = d.clock.now()?;
  let until = octacity_server_domain::Timestamp::from_unix_millis(
    now.unix_millis() + settings.worker.claim_lifetime().as_millis() as i64,
  )
  .map_err(|_| ApplicationError::invalid())?;
  let Some(scan) = d
    .store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: project,
      target,
      owner: FactoryKey::new(format!("source.{}", uuid::Uuid::new_v4())).map_err(|_| ApplicationError::invalid())?,
      now,
      until,
    })
    .await?
  else {
    return Ok(outcome);
  };
  outcome.claimed += 1;
  let operation_cancel = cancellation.child_token();
  let pass = async {
    let members = d.store.source_scan_members(scan.claim.clone(), d.clock.now()?).await?;
    let installed_profiles = if let Some(provider) = &profile_provider {
      provider.profiles(&members).await?
    } else {
      installation.profiles.clone()
    };
    let profiles = profiles_for(&installed_profiles, &members, managed_connections.as_ref())?;
    let mut polling = common_polling(profiles.iter().map(|p| &p.read))?;
    let reader = Arc::new(SourcePageBridge {
      managed_connections: managed_connections.clone(),
      store: d.store.clone(),
      registry: d.registry.clone(),
      profiles: profiles.clone(),
      clock: d.clock.clone(),
      settings,
      cancellation: operation_cancel.clone(),
    });
    let processor = Arc::new(SourceCandidateBridge {
      managed_connections: managed_connections.clone(),
      store: d.store.clone(),
      bytes: d.bytes.clone(),
      revisions: d.revisions.clone(),
      authorization: d.authorization.clone(),
      registry: d.registry.clone(),
      profiles: installed_profiles,
      clock: d.clock.clone(),
      cancellation: operation_cancel.clone(),
      settings,
    });
    let pending_limit = polling.max_items.min(64);
    let pending = d
      .store
      .pending_source_preparations(project, scan.claim.target, None, pending_limit)
      .await?;
    for retained in &pending {
      let input = members
        .iter()
        .find(|m| m.id() == retained.input.id() && m.definition().enabled)
        .ok_or_else(ApplicationError::capability_unavailable)?;
      retained.require_selected_owner(&scan.claim, input)?;
      processor
        .finish_preparation(scan.claim.clone(), input, retained.clone())
        .await?;
    }
    polling.max_items = polling.max_items.saturating_sub(pending.len() as u16);
    if pending.len() == usize::from(pending_limit) {
      return Ok(());
    }
    let poller = FactorySourcePoller::new(d.store.clone(), reader, processor, d.clock.clone(), polling)?;
    poller.run_once(scan.clone()).await?;
    Ok::<(), ApplicationError>(())
  };
  tokio::pin!(pass);
  let result = tokio::select! {
    biased;
    ()=cancellation.cancelled()=>Err(WorkSourceFailure::Cancelled),
    ()=tokio::time::sleep(settings.worker.claim_lifetime().saturating_sub(settings.cancellation_grace.saturating_mul(2)))=>Err(WorkSourceFailure::Timeout),
    result=&mut pass=>Ok(result),
  };
  let result = match result {
    Ok(result) => result,
    Err(class) => {
      operation_cancel.cancel();
      let _ = tokio::time::timeout(settings.cancellation_grace, &mut pass).await;
      Err(ApplicationError::SourceDependency {
        class,
        retry_after_millis: None,
      })
    }
  };
  let at = d.clock.now()?;
  if result.is_ok() {
    outcome.completed += 1;
  } else {
    outcome.deferred += 1;
  }
  let retry = match settings.target_retry {
    Some(retry) => retry,
    None => DurableRetryPolicy::new(
      settings.max_attempts,
      settings.worker.poll_interval().as_millis() as u64,
      settings.worker.retry_policy().max_backoff().as_millis() as u64,
    )
    .map_err(|_| ApplicationError::invalid())?,
  };
  let schedule = source_scan_schedule(
    scan.schedule.as_ref(),
    at,
    settings.worker.poll_interval(),
    retry,
    result.as_ref().err(),
  )?;
  // A definition change may revoke this owner while the provider is in flight.
  // Such an owner must not write diagnostics into the replacement's state.
  match d.store.finish_source_scan(scan.claim.clone(), at, schedule).await {
    Ok(()) | Err(StoreError::Conflict { .. }) => {}
    Err(error) => return Err(error.into()),
  }

  Ok(outcome)
}

fn add(
  at: octacity_server_domain::Timestamp,
  delay: Duration,
) -> Result<octacity_server_domain::Timestamp, ApplicationError> {
  octacity_server_domain::Timestamp::from_unix_millis(
    at.unix_millis()
      .checked_add(i64::try_from(delay.as_millis()).map_err(|_| ApplicationError::invalid())?)
      .ok_or_else(ApplicationError::invalid)?,
  )
  .map_err(|_| ApplicationError::invalid())
}
fn profiles_for(
  profiles: &[FactorySourceProfile],
  members: &[WorkSourceInput],
  connections: Option<&ManagedIntegrationConnections>,
) -> Result<Vec<FactorySourceProfile>, ApplicationError> {
  members
    .iter()
    .filter(|v| v.definition().enabled)
    .map(|input| {
      profiles
        .iter()
        .find(|p| p.read.project_id == input.project_id() && p.read.binding == input.definition().source)
        .cloned()
        .ok_or_else(ApplicationError::capability_unavailable)
        .and_then(|profile| profile.for_definition(input.definition(), connections))
    })
    .collect()
}
fn selection_digest(selection: &ConfigurationReference) -> Result<FactoryDigest, ApplicationError> {
  Ok(FactoryDigest::content_sha256(
    &serde_json::to_vec(selection).map_err(|_| ApplicationError::invalid())?,
  ))
}
fn common_polling<'a>(
  profiles: impl IntoIterator<Item = &'a WorkSourceReadProfile>,
) -> Result<SourcePollingPolicy, ApplicationError> {
  let mut profiles = profiles.into_iter();
  let mut result = profiles.next().ok_or_else(ApplicationError::invalid)?.polling.clone();
  for p in profiles.map(|p| &p.polling) {
    result.page_items = result.page_items.min(p.page_items);
    result.max_pages = result.max_pages.min(p.max_pages);
    result.max_items = result.max_items.min(p.max_items);
    result.max_bytes = result.max_bytes.min(p.max_bytes);
    result.max_page_bytes = result.max_page_bytes.min(p.max_page_bytes);
    result.retention_millis = result.retention_millis.min(p.retention_millis);
    result.overlap_millis = result.overlap_millis.max(p.overlap_millis);
    result.reconciliation_interval_millis = result
      .reconciliation_interval_millis
      .min(p.reconciliation_interval_millis);
  }
  Ok(result)
}

fn require_running(cancellation: &CancellationToken) -> Result<(), ApplicationError> {
  if cancellation.is_cancelled() {
    Err(ApplicationError::SourceDependency {
      class: WorkSourceFailure::Cancelled,
      retry_after_millis: None,
    })
  } else {
    Ok(())
  }
}

/// Optional runtime lifecycle boundary; Source failures remain local to each input.
#[async_trait::async_trait]
pub trait FactorySourceRuntime: Send + Sync {
  /// Installed bounds used by the existing worker supervisor.
  fn settings(&self) -> FactorySourceWorkerSettings;
  /// Runs a bounded pass and drains cancellation before returning.
  async fn poll(&self, cancellation: CancellationToken) -> Result<FactorySourceWorkerOutcome, ApplicationError>;
}
#[async_trait::async_trait]
impl<S: WorkSourceArtifactStore + 'static, B: ArtifactBytePublisher + 'static> FactorySourceRuntime
  for FactorySourceWorker<S, B>
{
  fn settings(&self) -> FactorySourceWorkerSettings {
    self.installation.settings
  }
  async fn poll(&self, cancellation: CancellationToken) -> Result<FactorySourceWorkerOutcome, ApplicationError> {
    self.run_once(cancellation).await
  }
}
#[path = "work_source_preview.rs"]
mod preview;
pub use preview::HostedFactoryWorkSourcePreview;
#[path = "work_source_observation.rs"]
mod observation;
pub use observation::HostedWorkSourceObservations;
