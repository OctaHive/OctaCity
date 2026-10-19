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
}
#[async_trait::async_trait]
impl<S: IssueSourceReadStore> SourcePollingSession for IssueReadSession<'_, S> {
  async fn read(&self, request: SourcePollingRequest) -> Result<SourcePollingPage, ApplicationError> {
    let page = self
      .owner
      .inner
      .reader
      .read(
        self.context,
        ReadWorkSourceObservations {
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
        project_id: self.claim.project_id,
        source: self.claim.target.source.clone(),
        item: observation.snapshot.item.clone(),
      };
      let receipt = observation.issue_receipt();
      receipt.validate(
        &selection,
        self.owner.inner.clock.now()?,
        octacity_server_store::StoreOperation::RefreshIssueSource,
      )?;
      if observation.project_id != self.claim.project_id
        || observation.security_scope != *self.context.security_scope()
        || observation.native_correlation != selection.native_correlation(&self.context.security_scope().to_store())
        || observation.snapshot_digest
          != octacity_server_factory::FactoryDigest::content_sha256(
            &serde_json::to_vec(&observation.snapshot).map_err(|_| ApplicationError::invalid())?,
          )
      {
        return Err(ApplicationError::invalid());
      }
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
    // Project/profile come from the original fence; native identity is already in the snapshot.
    let selection = IssueSourceSelection {
      project_id: self.claim.project_id,
      source: self.claim.target.source.clone(),
      item: candidate.snapshot.item.clone(),
    };
    let inner = &self.owner.inner;
    inner
      .reader
      .authorize(self.context, self.claim.project_id, self.claim.target.source.clone())
      .await?;
    let at = inner.clock.now()?;
    let scope = self.context.security_scope().to_store();
    let receipt_id = candidate.observation.id(selection.native_correlation(&scope))?;
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
          read_claim: Some(self.claim.clone()),
          selection,
          issue_id: issue,
          observation: candidate.observation,
          content,
          idempotency_key: IdempotencyKey::new(format!("source-refresh.{receipt_id}"))
            .map_err(|_| ApplicationError::invalid())?,
          refreshed_at: at,
        },
      )?)
      .await?;
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
        return Ok(None);
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
