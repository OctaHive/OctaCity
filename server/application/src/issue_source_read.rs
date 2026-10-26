//! Source observations consume the shared bounded driver without a Factory/admission target.
use super::*;
use crate::source_polling::*;
pub use octacity_server_store::IssueSourceReadTarget;
use octacity_server_store::{ClaimWorkSourceScan, IssueSourceReadClaim, IssueSourceReadStore};

/// Installed finite read and retry policy, independent of execution-input policy.
#[derive(Clone)]
pub struct IssueSourceReadPolicy {
  /// Shared page/window/reconciliation/retention budgets.
  pub polling: SourcePollingPolicy,
  /// Shared finite dependency retry budget.
  pub retry: DurableRetryPolicy,
  /// Positive delay between completed slices.
  pub poll_interval_millis: u64,
}
/// Explicit card refresh uses the existing read budgets and a separately bounded ownership lease.
#[derive(Clone)]
pub struct IssueSourceRefreshPolicy {
  /// Existing page, retry and cadence policy; no execution target is required.
  pub read: IssueSourceReadPolicy,
  /// Positive finite lease; a crashed HTTP reader cannot retain ownership indefinitely.
  pub claim_lifetime: std::time::Duration,
}
impl IssueSourceRefreshPolicy {
  fn validate(&self) -> Result<(), ApplicationError> {
    self.read.polling.validate()?;
    if self.read.poll_interval_millis == 0
      || self.read.poll_interval_millis > i64::MAX as u64
      || self.claim_lifetime.is_zero()
      || self.claim_lifetime.as_millis() > i64::MAX as u128
    {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
}
impl<S> IssueSourceHandlers<S> {
  /// Enables resumable card reads using the existing Source progress journal.
  pub fn with_refresh_policy(mut self, policy: IssueSourceRefreshPolicy) -> Result<Self, ApplicationError> {
    policy.validate()?;
    self.refresh_policy = Some(policy);
    Ok(self)
  }
}
#[derive(Clone)]
struct ExplicitRefresh {
  digest: octacity_server_factory::FactoryDigest,
  key: IdempotencyKey,
}

/// Optional consumer owner; construction starts no worker or provider access.
pub struct IssueSourceReadHandlers<S> {
  inner: IssueSourceHandlers<S>,
  settings: IssueSourceReadPolicy,
}
impl<S> IssueSourceReadHandlers<S> {
  /// Reuses the installed observation port, shared clock and management authority.
  pub fn new(
    store: Arc<S>,
    reader: Arc<dyn WorkSourceObservations>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    clock: Arc<dyn FactoryExecutionClock>,
    settings: IssueSourceReadPolicy,
  ) -> Result<Self, ApplicationError> {
    settings.polling.validate()?;
    if settings.poll_interval_millis == 0 || settings.poll_interval_millis > i64::MAX as u64 {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      inner: IssueSourceHandlers::new(store, reader, policy, clock),
      settings,
    })
  }
}
/// One explicitly opted-in import or retained-link refresh slice.
#[derive(Clone)]
pub struct RunIssueSourceReadCommand {
  /// Authorized Project.
  pub project_id: ProjectId,
  /// Installed selection or retained native link; no pipeline target.
  pub target: IssueSourceReadTarget,
  /// Trusted operation owner.
  pub owner: octacity_server_factory::FactoryKey,
  /// Finite original ownership deadline.
  pub until: Timestamp,
}
impl Command for RunIssueSourceReadCommand {
  type Outcome = Option<SourcePollOutcome>;
}
impl ManagementAuthorizationTarget for RunIssueSourceReadCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::Administer,
    ManagementResourceKind::Issue,
    ManagementResourceKind::Project,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::owned_collection_resource(
      ManagementResourceKind::Issue,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedCandidate {
  observation: octacity_server_store::IssueSourceObservation,
  snapshot: WorkSourceSnapshot,
}
struct IssueReadSession<'a, S> {
  owner: &'a IssueSourceReadHandlers<S>,
  context: &'a ManagementRequestContext,
  claim: IssueSourceReadClaim,
  explicit: Option<ExplicitRefresh>,
  complete: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl<S: IssueSourceReadStore> SourcePollingSession for IssueReadSession<'_, S> {
  fn complete(&self) -> bool {
    self.complete.load(std::sync::atomic::Ordering::SeqCst)
  }

  async fn read(&self, request: SourcePollingRequest) -> Result<SourcePollingPage, ApplicationError> {
    let page = self
      .owner
      .inner
      .reader
      .read(
        self.context,
        ReadWorkSourceObservations {
          connection: self.claim.target.read_connection().cloned(),
          project_id: self.claim.project_id,
          source: self.claim.target.source.clone(),
          scan_id: request.scan_id,
          window: request.window,
          continuation: request.continuation,
          limit: request.max_items,
        },
      )
      .await?;
    if page.observations.len() > usize::from(request.max_items) {
      return Err(ApplicationError::invalid());
    }
    let expires_at = page.observations.iter().map(|o| o.expires_at).min();
    let mut candidates = Vec::new();
    for observation in page.observations {
      let selection = IssueSourceSelection {
        connection: self.claim.target.read_connection().cloned(),
        project_id: self.claim.project_id,
        source: self.claim.target.source.clone(),
        item: observation.snapshot.item.clone(),
      };
      let receipt = observation.issue_receipt();
      observation.validate_issue_read(
        self.context,
        &selection,
        self.owner.inner.clock.now()?,
        octacity_server_store::StoreOperation::RefreshIssueSource,
      )?;
      candidates.push(
        serde_json::to_value(ObservedCandidate {
          observation: receipt,
          snapshot: observation.snapshot,
        })
        .map_err(|_| ApplicationError::invalid())?,
      );
    }
    Ok(SourcePollingPage {
      expires_at,
      candidates,
      next_cursor: page.next_cursor,
    })
  }
  async fn settle(&self, candidate: serde_json::Value) -> Result<(), ApplicationError> {
    let candidate: ObservedCandidate = serde_json::from_value(candidate).map_err(|_| ApplicationError::invalid())?;
    if self
      .claim
      .target
      .import
      .as_ref()
      .and_then(|i| i.definition.predicate.as_ref())
      .is_some_and(|p| {
        !p.mismatches(&candidate.snapshot.state, &candidate.snapshot.labels)
          .is_empty()
      })
    {
      return Ok(());
    }
    // Project/profile come from the original fence; native identity is already in the snapshot.
    let selection = IssueSourceSelection {
      connection: self.claim.target.read_connection().cloned(),
      project_id: self.claim.project_id,
      source: self.claim.target.source.clone(),
      item: candidate.snapshot.item.clone(),
    };
    let inner = &self.owner.inner;
    inner
      .reader
      .authorize_connection(
        self.context,
        self.claim.project_id,
        self.claim.target.source.clone(),
        self.claim.target.read_connection(),
      )
      .await?;
    let at = inner.clock.now()?;
    let scope = self.context.security_scope().to_store();
    let receipt_id = candidate.observation.id(selection.native_correlation(&scope))?;
    let status = Some(source_status(&candidate.snapshot));
    let content = source_draft(candidate.snapshot);
    let issue = if let Some(issue) = self.claim.target.issue_id {
      inner.visible(self.context, issue).await?;
      let link = inner
        .store
        .issue_source_link(self.claim.project_id, issue, &scope)
        .await?
        .ok_or_else(ApplicationError::capability_unavailable)?;
      if link.native_correlation != selection.native_correlation(&scope) {
        return Ok(());
      }
      issue
    } else {
      let resource = crate::management_security::owned_collection_resource(
        ManagementResourceKind::Issue,
        ManagementResourceKind::Project,
        self.claim.project_id,
      )
      .map_err(|_| ApplicationError::invalid())?;
      let create = inner
        .policy
        .authorize(self.context, ManagementAction::Create, &resource)
        .await
        .map_err(|_| ApplicationError::invalid())?;
      if !create
        .visibility_for::<ListProjectsQuery>()
        .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?
        .allows(&self.claim.project_id)
      {
        return Err(ApplicationError::capability_unavailable());
      }
      let visibility = inner
        .policy
        .authorize(self.context, ManagementAction::View, &resource)
        .await
        .map_err(|_| ApplicationError::invalid())?
        .visibility_for::<ListProjectIssuesQuery>()
        .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
      let outcome = inner
        .store
        .import_issue_source(crate::management_security::audited_mutation(
          self.context,
          ImportIssueSource {
            read_claim: Some(self.claim.clone()),
            selection: selection.clone(),
            observation: candidate.observation.clone(),
            content: content.clone(),
            id: IssueId::generate(),
            visibility,
            idempotency_key: IdempotencyKey::new(format!("source-import.{receipt_id}"))
              .map_err(|_| ApplicationError::invalid())?,
            imported_at: at,
          },
        )?)
        .await?;
      let IssueSourceOutcome::Linked { link, disposition } = outcome else {
        return Err(ApplicationError::invalid());
      };
      if disposition == octacity_server_store::MutationDisposition::Applied {
        return Ok(());
      }
      // Independent discovery may still encounter a paused link; it grants no refresh authority.
      if !inner
        .store
        .issue_source_refresh_state(self.claim.project_id, link.issue_id, &scope)
        .await?
        .ok_or_else(ApplicationError::capability_unavailable)?
        .enabled
      {
        return Ok(());
      }
      link.issue_id
    };
    let resource = crate::management_security::instance_resource(ManagementResourceKind::Issue, issue)
      .map_err(|_| ApplicationError::invalid())?;
    let grant = inner
      .policy
      .authorize(self.context, ManagementAction::Update, &resource)
      .await
      .map_err(|_| ApplicationError::invalid())?;
    crate::issue::visible_issue(&grant, issue)?;
    inner
      .store
      .refresh_issue_source(crate::management_security::audited_mutation(
        self.context,
        octacity_server_store::RefreshIssueSource {
          explicit_read: self.explicit.as_ref().map(|e| e.digest),
          status,
          read_claim: Some(self.claim.clone()),
          selection,
          issue_id: issue,
          observation: candidate.observation,
          content,
          idempotency_key: match &self.explicit {
            Some(e) => e.key.clone(),
            None => {
              IdempotencyKey::new(format!("source-refresh.{receipt_id}")).map_err(|_| ApplicationError::invalid())?
            }
          },
          refreshed_at: at,
        },
      )?)
      .await?;
    if self.explicit.is_some() {
      self.complete.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    Ok(())
  }
  async fn checkpoint(
    &self,
    at: Timestamp,
    expected: Option<octacity_server_store::WorkSourceProgress>,
    progress: octacity_server_store::WorkSourceProgress,
  ) -> Result<(), ApplicationError> {
    Ok(
      self
        .owner
        .inner
        .store
        .checkpoint_issue_source_read(
          &self.context.security_scope().to_store(),
          self.claim.clone(),
          at,
          expected,
          progress,
        )
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: IssueSourceReadStore> ManagementCommandUseCase<RunIssueSourceReadCommand> for IssueSourceReadHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: RunIssueSourceReadCommand,
  ) -> Result<Option<SourcePollOutcome>, ApplicationError> {
    let projects = grant
      .visibility_for::<ListProjectsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    if !projects.allows(&command.project_id) {
      return Err(
        octacity_server_store::StoreError::NotFound {
          entity: octacity_server_domain::EntityKind::Project,
        }
        .into(),
      );
    }
    self.run(context, command, None).await
  }
}
impl<S: IssueSourceReadStore> IssueSourceReadHandlers<S> {
  async fn run(
    &self,
    context: &ManagementRequestContext,
    command: RunIssueSourceReadCommand,
    explicit: Option<ExplicitRefresh>,
  ) -> Result<Option<SourcePollOutcome>, ApplicationError> {
    let now = self.inner.clock.now()?;
    if let Some(issue) = command.target.issue_id {
      self.inner.visible(context, issue).await?;
      let link = self
        .inner
        .store
        .issue_source_link(command.project_id, issue, &context.security_scope().to_store())
        .await?
        .ok_or_else(ApplicationError::capability_unavailable)?;
      if link.selection.source.installation.identity() != command.target.source.installation.identity() {
        return Err(ApplicationError::invalid());
      }
      let state = self
        .inner
        .store
        .issue_source_refresh_state(command.project_id, issue, &context.security_scope().to_store())
        .await?
        .ok_or_else(ApplicationError::capability_unavailable)?;
      if !state.enabled {
        return if explicit.is_some() {
          Err(ApplicationError::capability_unavailable())
        } else {
          Ok(None)
        };
      }
    }
    let scan = self
      .inner
      .store
      .claim_issue_source_read(
        &context.security_scope().to_store(),
        ClaimWorkSourceScan {
          project_id: command.project_id,
          target: command.target,
          owner: command.owner,
          now,
          until: command.until,
        },
      )
      .await?;
    let Some(scan) = scan else {
      return Ok(None);
    };
    let lease = SourcePollingLease {
      project_id: scan.claim.project_id,
      target: serde_json::to_vec(&(context.security_scope().as_str(), &scan.claim.target))
        .map_err(|_| ApplicationError::invalid())?,
      acquired_at: scan.claim.acquired_at,
      until: scan.claim.until,
    };
    let session = IssueReadSession {
      owner: self,
      context,
      claim: scan.claim.clone(),
      explicit,
      complete: std::sync::atomic::AtomicBool::new(false),
    };
    let result = SourcePollingDriver {
      session: &session,
      clock: self.inner.clock.as_ref(),
      policy: &self.settings.polling,
      lease,
    }
    .run_once(scan.progress)
    .await;
    let now = self.inner.clock.now()?;
    let schedule = source_scan_schedule(
      scan.schedule.as_ref(),
      now,
      std::time::Duration::from_millis(self.settings.poll_interval_millis),
      self.settings.retry,
      result.as_ref().err(),
    )?;
    self
      .inner
      .store
      .finish_issue_source_read(&context.security_scope().to_store(), scan.claim, now, schedule)
      .await?;
    result.map(Some)
  }
}

impl<S: IssueSourceReadStore> IssueSourceHandlers<S> {
  pub(super) async fn refresh_paged(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    mut command: RefreshIssueSourceCommand,
  ) -> Result<octacity_server_store::IssueSourceRefreshOutcome, ApplicationError> {
    crate::issue::visible_issue(grant, command.issue_id)?;
    self.visible(context, command.issue_id).await?;
    let scope = context.security_scope().to_store();
    let link = self
      .store
      .issue_source_link(command.project_id, command.issue_id, &scope)
      .await?
      .ok_or_else(ApplicationError::capability_unavailable)?;
    if command.read.project_id != command.project_id
      || command.read.source.installation.identity() != link.selection.source.installation.identity()
    {
      return Err(ApplicationError::invalid());
    }
    if command.read.connection.is_some() && command.read.connection != link.selection.connection {
      return Err(ApplicationError::invalid());
    }
    command.read.connection = link.selection.connection.clone();
    let selection = IssueSourceSelection {
      source: command.read.source.clone(),
      ..link.selection
    };
    let digest = octacity_server_factory::FactoryDigest::content_sha256(
      &serde_json::to_vec(&(
        &selection,
        command.issue_id,
        command.read.project_id,
        command.read.scan_id,
        command.read.window,
        &command.read.continuation,
        command.read.limit,
      ))
      .map_err(|_| ApplicationError::invalid())?,
    );
    if let Some(receipt) = self
      .store
      .issue_source_refresh_replay(
        command.project_id,
        command.issue_id,
        &scope,
        digest,
        &command.idempotency_key,
      )
      .await?
    {
      return Ok(receipt);
    }
    if command.read.continuation.is_some() || command.read.window.is_some() {
      return Err(ApplicationError::invalid());
    }
    self
      .reader
      .authorize_connection(
        context,
        command.project_id,
        command.read.source.clone(),
        selection.connection.as_ref(),
      )
      .await?;
    let policy = if let Some(provider) = &self.source_profile_provider {
      provider.refresh_policy(context, &selection).await?
    } else {
      self
        .refresh_policy
        .clone()
        .ok_or_else(ApplicationError::capability_unavailable)?
    };
    policy.validate()?;
    let now = self.clock.now()?;
    let until = Timestamp::from_unix_millis(
      now
        .unix_millis()
        .checked_add(policy.claim_lifetime.as_millis() as i64)
        .ok_or_else(ApplicationError::invalid)?,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let owner = IssueSourceReadHandlers::new(
      self.store.clone(),
      self.reader.clone(),
      self.policy.clone(),
      self.clock.clone(),
      policy.read.clone(),
    )?;
    let result = owner
      .run(
        context,
        RunIssueSourceReadCommand {
          project_id: command.project_id,
          target: IssueSourceReadTarget {
            import: None,
            pull_generation: 0,
            source: command.read.source.clone(),
            issue_id: Some(command.issue_id),
            connection: selection.connection.clone(),
          },
          owner: format!("issue-refresh.{digest}")
            .parse()
            .map_err(|_| ApplicationError::invalid())?,
          until,
        },
        Some(ExplicitRefresh {
          digest,
          key: command.idempotency_key.clone(),
        }),
      )
      .await;
    // A lost checkpoint or later failure cannot hide an already committed original receipt.
    if let Some(receipt) = self
      .store
      .issue_source_refresh_replay(
        command.project_id,
        command.issue_id,
        &scope,
        digest,
        &command.idempotency_key,
      )
      .await?
    {
      return Ok(receipt);
    }
    result?;
    Ok(octacity_server_store::IssueSourceRefreshOutcome {
      issue_id: command.issue_id,
      disposition: IssueSourceRefreshDisposition::Scanning,
      status: None,
    })
  }
}
