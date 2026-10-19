//! Optional installed Source reads, without a store, admission target or background worker.
use super::*;
use octacity_work_source_protocol::{Request, source};

/// Explicit pull capability reusing installed Source selection, bounds and verified process I/O.
/// A future card owner selects its own installed read profile and owns refresh progress/policy.
pub struct HostedWorkSourceObservations {
  registry: Arc<WorkAdapterRegistry>,
  profiles: Vec<WorkSourceReadProfile>,
  clock: Arc<dyn FactoryExecutionClock>,
  timeout: Duration,
  grace: Duration,
  cancellation: CancellationToken,
}
impl HostedWorkSourceObservations {
  /// Installs existing trusted read profiles; construction performs no reads and starts no worker.
  pub fn new(
    registry: Arc<WorkAdapterRegistry>,
    profiles: Vec<WorkSourceReadProfile>,
    clock: Arc<dyn FactoryExecutionClock>,
    timeout: Duration,
    grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<Self, ApplicationError> {
    if timeout.is_zero() || grace.is_zero() || profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS {
      return Err(ApplicationError::invalid());
    }
    for (index, profile) in profiles.iter().enumerate() {
      profile.polling.validate()?;
      if profiles[..index]
        .iter()
        .any(|p| p.project_id == profile.project_id && p.binding == profile.binding)
      {
        return Err(ApplicationError::invalid());
      }
    }
    Ok(Self {
      registry,
      profiles,
      clock,
      timeout,
      grace,
      cancellation,
    })
  }
}
#[async_trait::async_trait]
impl WorkSourceObservations for HostedWorkSourceObservations {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    source: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    require_running(&self.cancellation)?;
    self
      .authorize_until(
        self.profile(context, project, &source)?,
        tokio::time::Instant::now() + self.timeout,
      )
      .await
  }
  async fn read(
    &self,
    context: &ManagementRequestContext,
    request: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    let profile = self.profile(context, request.project_id, &request.source)?;
    if request.limit == 0 {
      return Err(ApplicationError::invalid());
    }
    require_running(&self.cancellation)?;
    let observed_at = self.clock.now()?;
    let deadline = tokio::time::Instant::now() + self.timeout;
    self.authorize_until(profile, deadline).await?;
    let wire = Request::Source(source::Request {
      protocol_version: source::PROTOCOL_VERSION,
      request_id: uuid::Uuid::new_v4().to_string(),
      command: source::Command::Scan(Box::new(source::ScanIntent {
        scan_id: request.scan_id.to_string(),
        configuration_digest: profile.provider_configuration.to_string(),
        resource: profile.resource.clone(),
        selection: profile.selection.clone(),
        additional_selections: vec![],
        network_hosts: profile.network_hosts.clone(),
        cursor: request.continuation,
        window: request
          .window
          .map(|(from, until)| {
            Ok::<_, ApplicationError>(source::ScanWindow {
              from_millis: u64::try_from(from.unix_millis()).map_err(|_| ApplicationError::invalid())?,
              until_millis: u64::try_from(until.unix_millis()).map_err(|_| ApplicationError::invalid())?,
            })
          })
          .transpose()?,
        max_items: request
          .limit
          .min(profile.polling.page_items)
          .min(profile.polling.max_items),
        max_snapshot_bytes: MAX_SOURCE_INPUT_BYTES,
        max_page_bytes: profile.polling.max_page_bytes.min(profile.polling.max_bytes),
      })),
    });
    let source::Outcome::Page(page) = process::execute_source(
      &self.registry,
      profile,
      wire,
      deadline.saturating_duration_since(tokio::time::Instant::now()),
      self.grace,
      self.cancellation.clone(),
    )
    .await?
    else {
      return Err(ApplicationError::capability_unavailable());
    };
    // A grant may be revoked while the external read is in flight. Do not hand its private
    // response to a consumer after revocation, or reset the original operation deadline.
    self.authorize_until(profile, deadline).await?;
    let expires_at = add(observed_at, Duration::from_millis(profile.polling.retention_millis))?;
    let scope = octacity_server_store::ManagementSecurityScope::new(context.security_scope().as_str())
      .map_err(|_| ApplicationError::invalid())?;
    let observations = page
      .candidates
      .into_iter()
      .map(|candidate| {
        let snapshot = process::normalized_snapshot(candidate)?;
        Ok(WorkSourceObservation {
          project_id: profile.project_id,
          security_scope: context.security_scope().clone(),
          source: profile.binding.clone(),
          plugin: profile.plugin.clone(),
          provider_configuration: profile.provider_configuration,
          selection_digest: selection_digest(&profile.selection)?,
          native_correlation: source_native_correlation(
            source_native_issue(
              profile.project_id,
              profile.binding.installation.identity(),
              &snapshot.item,
            ),
            source_security_partition(&scope),
          ),
          snapshot_digest: FactoryDigest::content_sha256(
            &serde_json::to_vec(&snapshot).map_err(|_| ApplicationError::invalid())?,
          ),
          snapshot,
          observed_at,
          expires_at,
        })
      })
      .collect::<Result<_, ApplicationError>>()?;
    Ok(WorkSourceObservationPage {
      observations,
      next_cursor: page.next_cursor,
    })
  }
}
impl HostedWorkSourceObservations {
  fn profile(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    source: &WorkSourceBinding,
  ) -> Result<&WorkSourceReadProfile, ApplicationError> {
    self
      .profiles
      .iter()
      .find(|profile| {
        profile.project_id == project
          && profile.binding == *source
          && profile.context.security_scope() == context.security_scope()
      })
      .ok_or_else(ApplicationError::capability_unavailable)
  }
  async fn authorize_until(
    &self,
    profile: &WorkSourceReadProfile,
    deadline: tokio::time::Instant,
  ) -> Result<(), ApplicationError> {
    tokio::select! {
      biased;
      () = self.cancellation.cancelled() => require_running(&self.cancellation),
      result = tokio::time::timeout_at(deadline, profile.authorize_access()) => result
        .map_err(|_| ApplicationError::SourceDependency { class: WorkSourceFailure::Timeout, retry_after_millis: None })?,
    }
  }
}
