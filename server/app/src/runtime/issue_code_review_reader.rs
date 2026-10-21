//! Optional installed review reads; no store, task intake or background poller.
use octacity_server_application::*;
use octacity_server_domain::{ProjectId, RepositoryId};
use octacity_server_factory::*;
use octacity_server_store::IssueCodeReviewRepositoryVisibility;
use octacity_server_work_source::WorkAdapterRegistry;
use octacity_work_source_protocol::{Request, ResourceAccess, Response, review};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

/// Explicit repository reader, independently pinned and granted through existing connections.
#[derive(Clone)]
pub struct IssueCodeReviewReadProfile {
  /// Current managed repository read authority; ticket and Delivery grants do not substitute it.
  pub managed_access: ManagedIntegrationAccess,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exact installed namespace, capability, configuration and resource pins.
  pub binding: WorkSourceBinding,
  /// Explicit mapping to the local code Repository; absent mappings cannot attach candidate lineage.
  pub repository: Option<RepositoryId>,
  /// Exact frozen Delivery policy pins targeting this installed native repository.
  /// These prove destination correspondence only and never grant Delivery authority.
  pub publication_targets: Vec<ImmutableReference>,
  /// Content-verified provider executable.
  pub plugin: ImmutableReference,
  /// Exact private provider transport/repository profile.
  pub provider_configuration: FactoryDigest,
  /// Native code repository and existing opaque `vcs_read` grant handle.
  pub resource: ResourceAccess,
  /// Installed destination ceiling.
  pub network_hosts: Vec<String>,
  /// Installation owner's accepted management security partition.
  pub context: ManagementRequestContext,
  /// Maximum check records in one explicit page, at most 100.
  pub page_items: u16,
  /// Encoded normalized page ceiling, at most 4 MiB.
  pub max_page_bytes: u32,
}
/// One explicit original-head check page. Its stable read identity survives continuation/retry.
#[derive(Clone)]
pub struct ReadIssueCodeReviewChecks {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exact installed/native review selection.
  pub selection: IssueCodeReviewSelection,
  /// Original immutable head, never substituted with the current remote head.
  pub head: octacity_server_domain::ImmutableRevision,
  /// Server-owned identity of this paging pass.
  pub read_id: FactoryDigest,
  /// Private opaque continuation within this original pass.
  pub continuation: Option<String>,
  /// Caller-narrowed records per page.
  pub limit: u16,
}
/// Bounded explicit observations over the common verified Work adapter process host.
/// Construction and retained-data queries never dispatch a process or start a worker.
pub struct HostedIssueCodeReviewObservations {
  registry: Arc<WorkAdapterRegistry>,
  profiles: Vec<IssueCodeReviewReadProfile>,
  clock: Arc<dyn FactoryExecutionClock>,
  timeout: Duration,
  grace: Duration,
  cancellation: CancellationToken,
}
impl HostedIssueCodeReviewObservations {
  /// Installs finite profiles without acquiring credentials or contacting providers.
  pub fn new(
    registry: Arc<WorkAdapterRegistry>,
    profiles: Vec<IssueCodeReviewReadProfile>,
    clock: Arc<dyn FactoryExecutionClock>,
    timeout: Duration,
    grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<Self, ApplicationError> {
    if timeout.is_zero() || grace.is_zero() || profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS {
      return Err(ApplicationError::invalid());
    }
    for (index, profile) in profiles.iter().enumerate() {
      if profile.publication_targets.len() > MAX_FACTORY_EXECUTOR_BINDINGS
        || (profile.repository.is_none() && !profile.publication_targets.is_empty())
        || profile
          .publication_targets
          .iter()
          .enumerate()
          .any(|(i, target)| profile.publication_targets[..i].contains(target))
      {
        return Err(ApplicationError::invalid());
      }
      profile
        .intent("validation", FactoryDigest::from_bytes([0; 32]))
        .validate()
        .map_err(|_| ApplicationError::invalid())?;
      if profiles[..index]
        .iter()
        .any(|other| other.project_id == profile.project_id && other.binding == profile.binding)
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
  /// Reads one bounded original-head page under current repository authority.
  /// A retry hint is returned through the existing dependency error; no retry or polling is scheduled.
  pub async fn read_checks(
    &self,
    context: &ManagementRequestContext,
    request: ReadIssueCodeReviewChecks,
  ) -> Result<review::CheckPage, ApplicationError> {
    self
      .read_checks_until(context, request, tokio::time::Instant::now() + self.timeout)
      .await
  }
  async fn read_checks_until(
    &self,
    context: &ManagementRequestContext,
    request: ReadIssueCodeReviewChecks,
    deadline: tokio::time::Instant,
  ) -> Result<review::CheckPage, ApplicationError> {
    if request.limit == 0 {
      return Err(ApplicationError::invalid());
    }
    let profile = self.profile(context, request.project_id, &request.selection)?;
    let mut intent = profile.intent(request.selection.item.item.as_str(), request.read_id);
    intent.head = Some(request.head.as_str().to_owned());
    intent.cursor = request.continuation;
    intent.max_items = request.limit.min(profile.page_items);
    match self
      .execute(profile, review::Command::Checks(Box::new(intent)), deadline)
      .await?
    {
      review::Outcome::Checks(page) => Ok(page),
      _ => Err(ApplicationError::capability_unavailable()),
    }
  }
  fn profile(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    selection: &IssueCodeReviewSelection,
  ) -> Result<&IssueCodeReviewReadProfile, ApplicationError> {
    self
      .profiles
      .iter()
      .find(|profile| {
        profile.project_id == project
          && profile.context.security_scope() == context.security_scope()
          && profile.binding == selection.source
          && profile.resource.resource_id == selection.item.resource.as_str()
      })
      .ok_or_else(ApplicationError::capability_unavailable)
  }
  async fn authorize_until(
    &self,
    profile: &IssueCodeReviewReadProfile,
    deadline: tokio::time::Instant,
  ) -> Result<(), ApplicationError> {
    tokio::select! {
      biased;
      _ = self.cancellation.cancelled() => Err(dependency(WorkSourceFailure::Cancelled)),
      result = tokio::time::timeout_at(deadline, profile.managed_access.authorize_work_binding(profile.project_id, &profile.binding, &profile.plugin, &profile.resource.resource_id, octacity_server_store::IntegrationPurpose::RepositoryRead)) => result.map_err(|_| dependency(WorkSourceFailure::Timeout))?,
    }
  }
  async fn execute(
    &self,
    profile: &IssueCodeReviewReadProfile,
    command: review::Command,
    deadline: tokio::time::Instant,
  ) -> Result<review::Outcome, ApplicationError> {
    self.authorize_until(profile, deadline).await?;
    let response = super::work_integration_read::execute_read(
      &self.registry,
      &profile.plugin,
      Request::Review(review::Request {
        protocol_version: review::PROTOCOL_VERSION,
        request_id: uuid::Uuid::new_v4().to_string(),
        command,
      }),
      deadline.saturating_duration_since(tokio::time::Instant::now()),
      self.grace,
      self.cancellation.clone(),
    )
    .await?;
    self.authorize_until(profile, deadline).await?;
    match response {
      Response::Review(response) => Ok(response.outcome),
      _ => Err(ApplicationError::capability_unavailable()),
    }
  }
}
impl IssueCodeReviewReadProfile {
  fn intent(&self, item_id: &str, read_id: FactoryDigest) -> review::Intent {
    review::Intent {
      read_id: read_id.to_string(),
      configuration_digest: self.provider_configuration.to_string(),
      resource: self.resource.clone(),
      item_id: item_id.into(),
      head: None,
      network_hosts: self.network_hosts.clone(),
      cursor: None,
      max_items: self.page_items,
      max_page_bytes: self.max_page_bytes,
    }
  }
}
#[async_trait::async_trait]
impl IssueCodeReviewObservations for HostedIssueCodeReviewObservations {
  async fn visibility(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
  ) -> Result<IssueCodeReviewRepositoryVisibility, ApplicationError> {
    let mut identities = std::collections::BTreeSet::new();
    for profile in self
      .profiles
      .iter()
      .filter(|profile| profile.project_id == project && profile.context.security_scope() == context.security_scope())
    {
      let repository =
        ExternalWorkIdentity::new(profile.resource.resource_id.clone()).map_err(|_| ApplicationError::invalid())?;
      identities.insert(IssueCodeReviewSelection::repository_identity_for(
        profile.binding.installation.identity(),
        &repository,
      ));
    }
    if identities.is_empty() {
      Ok(IssueCodeReviewRepositoryVisibility::none())
    } else {
      IssueCodeReviewRepositoryVisibility::restricted(identities).map_err(|_| ApplicationError::invalid())
    }
  }
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    selection: &IssueCodeReviewSelection,
  ) -> Result<(), ApplicationError> {
    self
      .authorize_until(
        self.profile(context, project, selection)?,
        tokio::time::Instant::now() + self.timeout,
      )
      .await
  }
  async fn authorize_candidate(
    &self,
    context: &ManagementRequestContext,
    selection: &IssueCodeReviewSelection,
    candidate: &CandidateSubject,
  ) -> Result<(), ApplicationError> {
    let profile = self.profile(context, candidate.exact().project_id(), selection)?;
    if profile.repository != Some(candidate.exact().repository_id()) {
      return Err(ApplicationError::capability_unavailable());
    }
    self
      .authorize_until(profile, tokio::time::Instant::now() + self.timeout)
      .await
  }
  async fn authorize_publication(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    selection: &IssueCodeReviewSelection,
    target: &ImmutableReference,
  ) -> Result<(), ApplicationError> {
    let profile = self.profile(context, project, selection)?;
    if !profile.publication_targets.contains(target) {
      return Err(ApplicationError::capability_unavailable());
    }
    self
      .authorize_until(profile, tokio::time::Instant::now() + self.timeout)
      .await
  }
  async fn read(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    selection: &IssueCodeReviewSelection,
  ) -> Result<IssueCodeReviewObservation, ApplicationError> {
    let deadline = tokio::time::Instant::now() + self.timeout;
    let observed_at = self.clock.now()?;
    let profile = self.profile(context, project, selection)?;
    let read_id = FactoryDigest::content_sha256(uuid::Uuid::new_v4().as_bytes());
    let review::Outcome::Observed(observation) = self
      .execute(
        profile,
        review::Command::Observe(Box::new(profile.intent(selection.item.item.as_str(), read_id))),
        deadline,
      )
      .await?
    else {
      return Err(ApplicationError::capability_unavailable());
    };
    let facts = observation.facts;
    let mut observation = IssueCodeReviewObservation {
      selection: selection.clone(),
      observed_at,
      title: safe(facts.title),
      number: safe(facts.number),
      url: safe(facts.url),
      author: safe(facts.author),
      state: facts.state.map(|state| match state {
        review::State::Open => IssueCodeReviewState::Open,
        review::State::Closed => IssueCodeReviewState::Closed,
        review::State::Merged => IssueCodeReviewState::Merged,
      }),
      draft: facts.draft,
      head: facts.head.and_then(|head| head.parse().ok()),
      base: facts.base.and_then(|base| base.parse().ok()),
      changed_files: facts.changed_files,
      additions: facts.additions,
      deletions: facts.deletions,
      review: safe(facts.review),
      checks: safe(facts.checks),
      check_details: None,
    };
    if let Some(head) = &observation.head
      && let Ok(page) = self
        .read_checks_until(
          context,
          ReadIssueCodeReviewChecks {
            project_id: project,
            selection: selection.clone(),
            head: head.clone(),
            read_id,
            continuation: None,
            limit: MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE,
          },
          deadline,
        )
        .await
    {
      if page.head != head.as_str() {
        return Err(ApplicationError::invalid());
      }
      let items = page
        .items
        .into_iter()
        .map(|check| {
          Ok(IssueExternalCheck {
            id: safe(Some(check.id)).ok_or_else(ApplicationError::invalid)?,
            name: safe(Some(check.name)).ok_or_else(ApplicationError::invalid)?,
            producer: safe(check.producer),
            required: None,
            state: match (check.state, check.conclusion) {
              (review::CheckState::Pending, _) => IssueCheckState::Pending,
              (review::CheckState::Running, _) => IssueCheckState::Running,
              (review::CheckState::Completed, Some(review::Conclusion::Success)) => IssueCheckState::Pass,
              (
                review::CheckState::Completed,
                Some(review::Conclusion::Failure | review::Conclusion::TimedOut | review::Conclusion::ActionRequired),
              ) => IssueCheckState::Fail,
              (review::CheckState::Completed, Some(review::Conclusion::Cancelled)) => IssueCheckState::Cancelled,
              (review::CheckState::Completed, Some(review::Conclusion::Skipped)) => IssueCheckState::Skipped,
              _ => IssueCheckState::Indeterminate,
            },
          })
        })
        .collect::<Result<Vec<_>, ApplicationError>>()?;
      observation.check_details = Some(IssueExternalChecks {
        source: IssueExternalCheckSource::ExternalCi,
        head: head.clone(),
        observed_at,
        has_more: page.next_cursor.is_some(),
        items,
      });
    }
    self.authorize_until(profile, deadline).await?;
    observation.validate()?;
    Ok(observation)
  }
}
fn safe(value: Option<String>) -> Option<FactorySafeText> {
  value.and_then(|value| value.parse().ok())
}
fn dependency(class: WorkSourceFailure) -> ApplicationError {
  ApplicationError::SourceDependency {
    class,
    retry_after_millis: None,
  }
}
