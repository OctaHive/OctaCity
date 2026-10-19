//! Authorized optional planning; no provider, Factory or execution dependency.
use super::*;
pub use octacity_server_store::{
  IssuePlanning, IssuePropertyDefinitionSet, IssuePropertyStore, MAX_ISSUE_LABELS, MAX_ISSUE_PROPERTIES,
  MAX_ISSUE_PROPERTY_CHOICES, MAX_ISSUE_PROPERTY_KEY_BYTES, MAX_ISSUE_PROPERTY_NUMBER, MAX_ISSUE_PROPERTY_TEXT_BYTES,
  PublishedIssueProperties,
};

/// Available organizational person identities; a service principal is not a person.
#[derive(Clone, Debug, Default)]
pub struct IssueAssigneeChoices {
  /// False when no authorized person directory is installed.
  pub available: bool,
  /// Only identities the current operator may select.
  pub items: Vec<IssueAssignee>,
}
/// One server-resolved organizational person, with no implied permissions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueAssignee {
  /// Opaque deployment-local identity.
  pub id: String,
  /// Safe display name.
  pub name: String,
}
/// Current Project planning settings and honest directory availability.
#[derive(Clone, Debug)]
pub struct IssuePropertyChoices {
  /// Exact bounded Project catalog.
  pub definitions: PublishedIssueProperties,
  /// Independently resolved organizational identity choices.
  pub assignees: IssueAssigneeChoices,
}
/// Reads planning choices without requiring provider or Factory configuration.
#[derive(Clone, Debug)]
pub struct GetIssuePropertyChoicesQuery {
  /// Visible owning Project.
  pub project_id: ProjectId,
}
impl Query for GetIssuePropertyChoicesQuery {
  type Outcome = IssuePropertyChoices;
}
impl ManagementAuthorizationTarget for GetIssuePropertyChoicesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
#[async_trait::async_trait]
impl<S: IssuePropertyStore> ManagementQueryUseCase<GetIssuePropertyChoicesQuery> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssuePropertyChoicesQuery,
  ) -> Result<IssuePropertyChoices, ApplicationError> {
    visible_project(grant, query.project_id)?;
    Ok(IssuePropertyChoices {
      definitions: self.store.issue_properties(query.project_id, None).await?,
      assignees: self.assignees(context, query.project_id).await?,
    })
  }
}

pub use octacity_server_store::{IssuePropertiesOutcome, PublishIssuePropertiesIntent};
/// Publish immutable Project property choices, independently of editing tasks.
#[derive(Clone, Debug)]
pub struct PublishIssuePropertiesCommand {
  /// Frozen reviewed Project catalog and current version.
  pub intent: PublishIssuePropertiesIntent,
  /// Scoped original replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted original publication time.
  pub published_at: Timestamp,
}
impl Command for PublishIssuePropertiesCommand {
  type Outcome = IssuePropertiesOutcome;
}
impl ManagementAuthorizationTarget for PublishIssuePropertiesCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Administer, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Project, self.intent.project_id)
  }
}
/// Dedicated retained definition read, independent of current edits.
#[derive(Clone, Debug)]
pub struct GetIssuePropertiesQuery {
  /// Owning visible Project.
  pub project_id: ProjectId,
  /// Exact retained publication or current when absent.
  pub version: Option<u64>,
}
impl Query for GetIssuePropertiesQuery {
  type Outcome = PublishedIssueProperties;
}
impl ManagementAuthorizationTarget for GetIssuePropertiesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssuePropertyChoicesQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    GetIssuePropertyChoicesQuery {
      project_id: self.project_id,
    }
    .management_resource()
  }
}
fn visible_project(grant: &ManagementAuthorizationGrant, project_id: ProjectId) -> Result<(), ApplicationError> {
  let visibility = grant
    .visibility_for::<ListProjectsQuery>()
    .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
  if !visibility.allows(&project_id) {
    return Err(
      octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Project,
      }
      .into(),
    );
  }
  Ok(())
}
#[async_trait::async_trait]
impl<S: IssuePropertyStore> ManagementCommandUseCase<PublishIssuePropertiesCommand> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: PublishIssuePropertiesCommand,
  ) -> Result<IssuePropertiesOutcome, ApplicationError> {
    visible_project(grant, command.intent.project_id)?;
    Ok(
      self
        .store
        .publish_issue_properties(crate::management_security::audited_mutation(
          context,
          octacity_server_store::PublishIssueProperties {
            intent: command.intent,
            idempotency_key: command.idempotency_key,
            published_at: command.published_at,
          },
        )?)
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: IssuePropertyStore> ManagementQueryUseCase<GetIssuePropertiesQuery> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssuePropertiesQuery,
  ) -> Result<PublishedIssueProperties, ApplicationError> {
    visible_project(grant, query.project_id)?;
    Ok(self.store.issue_properties(query.project_id, query.version).await?)
  }
}

pub use octacity_server_store::EditIssuePlanningIntent;
/// Planning uses ordinary Issue edit rights, separate from definition administration.
#[derive(Clone, Debug)]
pub struct EditIssuePlanningCommand {
  /// Exact reviewed organizational replacement.
  pub intent: EditIssuePlanningIntent,
  /// Original scoped replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted mutation time.
  pub edited_at: Timestamp,
}
impl Command for EditIssuePlanningCommand {
  type Outcome = IssueMutationOutcome;
}
impl ManagementAuthorizationTarget for EditIssuePlanningCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = EditIssueCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.intent.id)
  }
}
#[async_trait::async_trait]
impl<S: IssuePropertyStore> ManagementCommandUseCase<EditIssuePlanningCommand> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: EditIssuePlanningCommand,
  ) -> Result<IssueMutationOutcome, ApplicationError> {
    visible_issue(grant, command.intent.id)?;
    self.visible_parent(context, command.intent.planning.parent_id).await?;
    let assignees = self
      .planning_assignees(context, command.intent.project_id, &command.intent.planning)
      .await?;
    Ok(
      self
        .store
        .edit_issue_planning(crate::management_security::audited_mutation(
          context,
          octacity_server_store::EditIssuePlanning {
            assignees,
            intent: command.intent,
            idempotency_key: command.idempotency_key,
            edited_at: command.edited_at,
          },
        )?)
        .await?,
    )
  }
}

/// Existing deployment identity boundary; implementations return only selectable people.
#[async_trait::async_trait]
pub trait IssueAssigneeDirectory: Send + Sync {
  /// Resolves bounded choices under current operator and Project visibility.
  async fn choices(
    &self,
    context: &ManagementRequestContext,
    project_id: ProjectId,
  ) -> Result<IssueAssigneeChoices, ApplicationError>;
}
/// Maximum organizational identities returned in one directory choice set.
pub const MAX_ISSUE_ASSIGNEES: usize = 200;
impl<S> IssueHandlers<S> {
  /// Installs an authorized person directory; omission leaves all tasks usable unassigned.
  pub fn with_assignee_directory(mut self, directory: Arc<dyn IssueAssigneeDirectory>) -> Self {
    self.directory = Some(directory);
    self
  }
  async fn assignees(
    &self,
    context: &ManagementRequestContext,
    project_id: ProjectId,
  ) -> Result<IssueAssigneeChoices, ApplicationError> {
    let Some(directory) = &self.directory else {
      return Ok(IssueAssigneeChoices::default());
    };
    let choices = match directory.choices(context, project_id).await {
      Err(e) if e.classification() == ApplicationFailure::Unavailable => return Ok(IssueAssigneeChoices::default()),
      result => result?,
    };
    if choices.items.len() > MAX_ISSUE_ASSIGNEES
      || (!choices.available && !choices.items.is_empty())
      || choices.items.iter().any(|p| {
        p.id.is_empty()
          || p.id.len() > MAX_ISSUE_TITLE_BYTES
          || p.id.trim() != p.id
          || p.id.chars().any(char::is_control)
          || p.name.is_empty()
          || p.name.len() > MAX_ISSUE_TITLE_BYTES
          || p.name.trim() != p.name
          || p.name.chars().any(char::is_control)
      })
      || choices
        .items
        .iter()
        .map(|p| &p.id)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != choices.items.len()
    {
      return Err(ApplicationError::InvalidInput);
    }
    Ok(choices)
  }
  pub(super) async fn planning_assignees(
    &self,
    context: &ManagementRequestContext,
    project_id: ProjectId,
    planning: &IssuePlanning,
  ) -> Result<std::collections::BTreeSet<String>, ApplicationError> {
    if planning.assignee.is_none() {
      return Ok(Default::default());
    }
    Ok(
      self
        .assignees(context, project_id)
        .await?
        .items
        .into_iter()
        .map(|p| p.id)
        .collect(),
    )
  }
}

impl<S> IssueHandlers<S> {
  pub(super) async fn visible_parent(
    &self,
    context: &ManagementRequestContext,
    parent: Option<IssueId>,
  ) -> Result<(), ApplicationError> {
    let Some(id) = parent else {
      return Ok(());
    };
    let target = crate::management_security::instance_resource(ManagementResourceKind::Issue, id)
      .map_err(|_| ApplicationError::InvalidInput)?;
    let grant = self
      .policy
      .authorize(context, ManagementAction::View, &target)
      .await
      .map_err(|_| octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Issue,
      })?;
    visible_issue(&grant, id)
  }
}

/// Bounded visible organizational children; relationships grant no access.
#[derive(Clone, Debug)]
pub struct ListIssueChildrenQuery {
  page: crate::CurrentDefinitionPageInput<IssueId>,
  parent_id: IssueId,
}
impl ListIssueChildrenQuery {
  /// Validates the shared Issue page bounds before dispatch.
  pub fn try_new(
    project_id: ProjectId,
    parent_id: IssueId,
    after: Option<IssueId>,
    limit: u16,
  ) -> Result<Self, crate::CurrentDefinitionPageError> {
    Ok(Self {
      page: crate::CurrentDefinitionPageInput::try_new(project_id, after, limit)?,
      parent_id,
    })
  }
}
impl Query for ListIssueChildrenQuery {
  type Outcome = IssuePage;
}
impl ManagementAuthorizationTarget for ListIssueChildrenQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.parent_id)
  }
}
#[async_trait::async_trait]
impl<S: IssuePropertyStore> ManagementQueryUseCase<ListIssueChildrenQuery> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueChildrenQuery,
  ) -> Result<IssuePage, ApplicationError> {
    visible_issue(grant, query.parent_id)?;
    let visibility = grant
      .visibility_for::<ListIssueChildrenQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    Ok(
      self
        .store
        .issue_children(
          octacity_server_store::ListProjectIssues::new(
            query.page.project_id(),
            query.page.after().copied(),
            query.page.limit(),
            visibility,
          )?,
          query.parent_id,
        )
        .await?,
    )
  }
}
