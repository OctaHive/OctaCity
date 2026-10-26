//! Explicit native association consumes the existing authorized, bounded Source reader.
use crate::*;
pub use octacity_server_domain::IssueId;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_store::{AttachIssueSource, IdempotencyKey, ImportIssueSource};
pub use octacity_server_store::{
  AttachIssueSourceIntent, IssueSourceLink, IssueSourceOutcome, IssueSourceRefreshDisposition, IssueSourceSelection,
  IssueSourceStore,
};
use std::sync::Arc;

#[path = "issue_source_management.rs"]
mod management;
#[path = "issue_source_project_choices.rs"]
mod project_choices;
pub use project_choices::*;
#[path = "issue_source_project_import.rs"]
mod project_import;
#[path = "issue_source_project_preview.rs"]
mod project_preview;
pub use management::*;
pub use project_import::*;
pub use project_preview::*;
#[path = "issue_synchronization.rs"]
mod synchronization;
pub use synchronization::*;

/// Source association owner; no admission, Reporter or credential port is installed here.
pub struct IssueSourceHandlers<S> {
  project_profiles: Option<Arc<dyn FactorySourceConfigurationProvider<S>>>,
  mappings: Vec<InstalledIssueSourceMapping>,
  refresh_policy: Option<IssueSourceRefreshPolicy>,
  store: Arc<S>,
  reader: Arc<dyn WorkSourceObservations>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S> IssueSourceHandlers<S> {
  /// Reuses installed scoped Source reads and ordinary Issue authorization.
  pub fn new(
    store: Arc<S>,
    reader: Arc<dyn WorkSourceObservations>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    Self {
      project_profiles: None,
      mappings: vec![],
      refresh_policy: None,
      store,
      reader,
      policy,
      clock,
    }
  }
}
/// Frozen explicit attachment after reviewing the manual Issue and its ownership.
#[derive(Clone)]
pub struct AttachIssueSourceCommand {
  /// Exact task/native/version/ownership review.
  pub intent: AttachIssueSourceIntent,
  /// Existing bounded read selection, including any opaque continuation.
  pub read: ReadWorkSourceObservations,
  /// Scoped replay key.
  pub idempotency_key: IdempotencyKey,
  /// Trusted server time.
  pub attached_at: Timestamp,
}
impl Command for AttachIssueSourceCommand {
  type Outcome = IssueSourceOutcome;
}
impl ManagementAuthorizationTarget for AttachIssueSourceCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Administer, ManagementResourceKind::Issue);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.intent.issue_id)
  }
}
/// One bounded import handoff; repeated native identity never refreshes existing requirements.
#[derive(Clone)]
pub struct ImportIssueSourceCommand {
  /// Reviewed installed/native selection.
  pub selection: IssueSourceSelection,
  /// Existing scoped Source read, independent of a Factory input or target.
  pub read: ReadWorkSourceObservations,
  /// Caller replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted server time.
  pub imported_at: Timestamp,
}
impl Command for ImportIssueSourceCommand {
  type Outcome = IssueSourceOutcome;
}
impl ManagementAuthorizationTarget for ImportIssueSourceCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = CreateIssueCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::owned_collection_resource(
      ManagementResourceKind::Issue,
      ManagementResourceKind::Project,
      self.selection.project_id,
    )
  }
}
/// Read-only association in the caller's accepted partition; no provider access.
#[derive(Clone, Debug)]
pub struct GetIssueSourceLinkQuery {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Visible task.
  pub issue_id: IssueId,
}
impl Query for GetIssueSourceLinkQuery {
  type Outcome = Option<IssueSourceLink>;
}
impl ManagementAuthorizationTarget for GetIssueSourceLinkQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
impl<S: IssueSourceStore> IssueSourceHandlers<S> {
  async fn visible(&self, context: &ManagementRequestContext, id: IssueId) -> Result<(), ApplicationError> {
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Issue,
      id,
    )
    .await
  }
  async fn observe(
    &self,
    context: &ManagementRequestContext,
    selection: &IssueSourceSelection,
    read: ReadWorkSourceObservations,
    now: Timestamp,
    operation: octacity_server_store::StoreOperation,
  ) -> Result<(WorkSourceObservation, Timestamp), ApplicationError> {
    if read.project_id != selection.project_id || read.source != selection.source || read.limit == 0 {
      return Err(ApplicationError::invalid());
    }
    octacity_server_store::validate_source_cursor(&read.continuation).map_err(|_| ApplicationError::invalid())?;
    let limit = usize::from(read.limit);
    let page = self.reader.read(context, read).await?;
    // Read again after asynchronous provider access; an observation can expire in flight.
    let now = self.clock.now()?.max(now);
    if page.observations.len() > limit {
      return Err(ApplicationError::invalid());
    }
    let mut matches = page
      .observations
      .into_iter()
      .filter(|o| o.snapshot.item == selection.item);
    let observation = matches.next().ok_or_else(ApplicationError::capability_unavailable)?;
    if matches.next().is_some() {
      return Err(ApplicationError::invalid());
    }
    observation.validate_issue_read(context, selection, now, operation)?;
    Ok((observation, now))
  }
  async fn visible_outcome(
    &self,
    context: &ManagementRequestContext,
    outcome: IssueSourceOutcome,
  ) -> Result<IssueSourceOutcome, ApplicationError> {
    match outcome {
      IssueSourceOutcome::Linked { ref link, .. } => {
        self.visible(context, link.issue_id).await?;
        Ok(outcome)
      }
      IssueSourceOutcome::Conflict { canonical: Some(link) } if self.visible(context, link.issue_id).await.is_err() => {
        Ok(IssueSourceOutcome::Conflict { canonical: None })
      }
      other => Ok(other),
    }
  }
}
#[async_trait::async_trait]
impl<S: IssueSourceStore> ManagementCommandUseCase<ImportIssueSourceCommand> for IssueSourceHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: ImportIssueSourceCommand,
  ) -> Result<IssueSourceOutcome, ApplicationError> {
    let projects = grant
      .visibility_for::<ListProjectsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    if !projects.allows(&command.selection.project_id) {
      return Err(
        octacity_server_store::StoreError::NotFound {
          entity: octacity_server_domain::EntityKind::Project,
        }
        .into(),
      );
    }
    let target = crate::management_security::owned_collection_resource(
      ManagementResourceKind::Issue,
      ManagementResourceKind::Project,
      command.selection.project_id,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let visibility = self
      .policy
      .authorize(context, ManagementAction::View, &target)
      .await
      .map_err(|_| ApplicationError::invalid())?
      .visibility_for::<ListProjectIssuesQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let (observation, imported_at) = self
      .observe(
        context,
        &command.selection,
        command.read,
        command.imported_at,
        octacity_server_store::StoreOperation::ImportIssueSource,
      )
      .await?;
    let outcome = self
      .store
      .import_issue_source(crate::management_security::audited_mutation(
        context,
        ImportIssueSource {
          read_claim: None,
          observation: observation.issue_receipt(),
          visibility,
          selection: command.selection,
          id: IssueId::generate(),
          content: source_draft(observation.snapshot),
          idempotency_key: command.idempotency_key,
          imported_at,
        },
      )?)
      .await?;
    self.visible_outcome(context, outcome).await
  }
}
#[async_trait::async_trait]
impl<S: IssueSourceStore> ManagementCommandUseCase<AttachIssueSourceCommand> for IssueSourceHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: AttachIssueSourceCommand,
  ) -> Result<IssueSourceOutcome, ApplicationError> {
    crate::issue::visible_issue(grant, command.intent.issue_id)?;
    self.visible(context, command.intent.issue_id).await?;
    self
      .store
      .issue(command.intent.selection.project_id, command.intent.issue_id, None)
      .await?;
    if let Some(replay) = self
      .store
      .issue_source_attachment_replay(
        &context.security_scope().to_store(),
        &command.intent,
        &command.idempotency_key,
      )
      .await?
    {
      return self.visible_outcome(context, replay).await;
    }
    let (observation, attached_at) = self
      .observe(
        context,
        &command.intent.selection,
        command.read,
        command.attached_at,
        octacity_server_store::StoreOperation::AttachIssueSource,
      )
      .await?;
    let outcome = self
      .store
      .attach_issue_source(crate::management_security::audited_mutation(
        context,
        AttachIssueSource {
          observation: observation.issue_receipt(),
          intent: command.intent,
          idempotency_key: command.idempotency_key,
          attached_at,
        },
      )?)
      .await?;
    self.visible_outcome(context, outcome).await
  }
}
#[async_trait::async_trait]
impl<S: IssueSourceStore> ManagementQueryUseCase<GetIssueSourceLinkQuery> for IssueSourceHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueSourceLinkQuery,
  ) -> Result<Option<IssueSourceLink>, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue_id)?;
    Ok(
      self
        .store
        .issue_source_link(query.project_id, query.issue_id, &context.security_scope().to_store())
        .await?,
    )
  }
}

/// One explicitly authorized retained-link read; execution input selectors are not consulted.
#[derive(Clone)]
pub struct RefreshIssueSourceCommand {
  /// Owning visible Project.
  pub project_id: ProjectId,
  /// Retained task with one native association.
  pub issue_id: IssueId,
  /// Installed read profile, which may be broader than an execution selector.
  pub read: ReadWorkSourceObservations,
  /// Original operation key.
  pub idempotency_key: IdempotencyKey,
  /// Trusted ingress time, rechecked after the read.
  pub refreshed_at: Timestamp,
}
impl Command for RefreshIssueSourceCommand {
  type Outcome = octacity_server_store::IssueSourceRefreshOutcome;
}
impl ManagementAuthorizationTarget for RefreshIssueSourceCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Update, ManagementResourceKind::Issue);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
#[async_trait::async_trait]
impl<S: octacity_server_store::IssueSourceRefreshStore> ManagementCommandUseCase<RefreshIssueSourceCommand>
  for IssueSourceHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: RefreshIssueSourceCommand,
  ) -> Result<octacity_server_store::IssueSourceRefreshOutcome, ApplicationError> {
    self.refresh_source(context, grant, command).await
  }
}
/// One explicit client read; exact retries recover the first accepted observation without provider access.
#[derive(Clone)]
pub struct ObserveIssueSourceCommand {
  /// Existing authorized Source read and its original client operation key.
  pub refresh: RefreshIssueSourceCommand,
}
impl Command for ObserveIssueSourceCommand {
  type Outcome = octacity_server_store::IssueSourceRefreshOutcome;
}
impl ManagementAuthorizationTarget for ObserveIssueSourceCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = RefreshIssueSourceCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.refresh.management_resource()
  }
}
#[async_trait::async_trait]
impl<S: octacity_server_store::IssueSourceReadStore> ManagementCommandUseCase<ObserveIssueSourceCommand>
  for IssueSourceHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: ObserveIssueSourceCommand,
  ) -> Result<octacity_server_store::IssueSourceRefreshOutcome, ApplicationError> {
    self.refresh_paged(context, grant, command.refresh).await
  }
}
impl<S: octacity_server_store::IssueSourceRefreshStore> IssueSourceHandlers<S> {
  async fn refresh_source(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: RefreshIssueSourceCommand,
  ) -> Result<octacity_server_store::IssueSourceRefreshOutcome, ApplicationError> {
    crate::issue::visible_issue(grant, command.issue_id)?;
    self.visible(context, command.issue_id).await?;
    let link = self
      .store
      .issue_source_link(
        command.project_id,
        command.issue_id,
        &context.security_scope().to_store(),
      )
      .await?
      .ok_or_else(ApplicationError::capability_unavailable)?;
    if command.read.source.installation.identity() != link.selection.source.installation.identity() {
      return Err(ApplicationError::invalid());
    }
    let selection = IssueSourceSelection {
      source: command.read.source.clone(),
      ..link.selection
    };
    if !self
      .store
      .issue_source_refresh_state(
        command.project_id,
        command.issue_id,
        &context.security_scope().to_store(),
      )
      .await?
      .is_some_and(|state| state.enabled)
    {
      return Err(ApplicationError::capability_unavailable());
    }
    let (observation, refreshed_at) = self
      .observe(
        context,
        &selection,
        command.read,
        command.refreshed_at,
        octacity_server_store::StoreOperation::RefreshIssueSource,
      )
      .await?;
    let receipt = observation.issue_receipt();
    Ok(
      self
        .store
        .refresh_issue_source(crate::management_security::audited_mutation(
          context,
          octacity_server_store::RefreshIssueSource {
            explicit_read: None,
            status: Some(source_status(&observation.snapshot)),
            read_claim: None,
            content: source_draft(observation.snapshot),
            selection,
            issue_id: command.issue_id,
            observation: receipt,
            idempotency_key: command.idempotency_key,
            refreshed_at,
          },
        )?)
        .await?,
    )
  }
}

#[path = "issue_source_read.rs"]
mod reads;
pub use reads::*;
#[path = "issue_source_resolution.rs"]
mod resolution;
pub use resolution::*;
#[path = "issue_source_mapping.rs"]
mod mapping;
pub use mapping::*;

/// Reviewed control shared with the trusted store request; no provider or Factory settings.
pub use octacity_server_store::SetIssueSourcePull as SetIssueSourcePullCommand;
impl Command for SetIssueSourcePullCommand {
  type Outcome = octacity_server_store::IssueSourcePullOutcome;
}
impl ManagementAuthorizationTarget for SetIssueSourcePullCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Administer, ManagementResourceKind::Issue);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
#[async_trait::async_trait]
impl<S: octacity_server_store::IssueSourceRefreshStore> ManagementCommandUseCase<SetIssueSourcePullCommand>
  for IssueSourceHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: SetIssueSourcePullCommand,
  ) -> Result<octacity_server_store::IssueSourcePullOutcome, ApplicationError> {
    crate::issue::visible_issue(grant, command.issue_id)?;
    self.visible(context, command.issue_id).await?;
    Ok(
      self
        .store
        .set_issue_source_pull(crate::management_security::audited_mutation(context, command)?)
        .await?,
    )
  }
}

// Initial normalized text is data; acceptance and planning are never fabricated from a native snapshot.
fn source_draft(snapshot: WorkSourceSnapshot) -> IssueContent {
  IssueContent {
    title: snapshot.title,
    description: snapshot.body.unwrap_or_default(),
    acceptance: None,
    priority: None,
  }
}
fn source_status(snapshot: &WorkSourceSnapshot) -> octacity_server_store::IssueSourceStatusObservation {
  octacity_server_store::IssueSourceStatusObservation {
    state: snapshot.state.clone(),
    labels: snapshot.labels.clone(),
  }
}
