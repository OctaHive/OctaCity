//! Retained task synchronization review, independent of provider availability.
use super::*;
pub use octacity_server_domain::IssueSourceMappingVersion;
pub use octacity_server_factory::MAX_WORK_SOURCE_PREDICATE_STATE_BYTES;
pub use octacity_server_factory::{
  FactoryDigest, ImmutableReference, MAX_EXTERNAL_WORK_IDENTITY_BYTES, WorkSourceBinding, WorkSourceReportingPolicy,
};
use octacity_server_store::IssueSourceMappingStore;
pub use octacity_server_store::MAX_ISSUE_SOURCE_MAPPING_BYTES;
pub use octacity_server_store::{ISSUE_SOURCE_PULL_FIELDS, ISSUE_SOURCE_PUSH_FIELDS};
pub use octacity_server_store::{IssueSourceRefreshState, IssueWorkflowPin, PublishedIssueSourceMapping};
pub use octacity_server_store::{MAX_SOURCE_POLL_CURSOR_BYTES, MAX_SOURCE_POLL_PAGE_ITEMS, WorkSourceItem};

/// Private-page continuation with only safe native identities, never task bodies or credentials.
#[derive(Clone, serde::Serialize)]
pub struct IssueSourceCandidates {
  /// Original provider read pass, retained when requesting another page.
  pub scan_id: FactoryDigest,
  /// Bounded identities observed within the authorized installed container.
  pub items: Vec<WorkSourceItem>,
  /// Opaque provider continuation; it grants no resource authority.
  pub next_cursor: Option<String>,
}
/// Explicit bounded native choice preview, with no import, attachment or progress publication.
#[derive(Clone)]
pub struct PreviewIssueSourceCommand {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Visible task to administer.
  pub issue_id: IssueId,
  /// Exact installed read choice.
  pub source: WorkSourceBinding,
  /// Previous page's pass, or absence for a new independent read.
  pub scan_id: Option<FactoryDigest>,
  /// Previous page's opaque continuation.
  pub continuation: Option<String>,
}
impl Command for PreviewIssueSourceCommand {
  type Outcome = IssueSourceCandidates;
}
impl ManagementAuthorizationTarget for PreviewIssueSourceCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = AttachIssueSourceCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
#[async_trait::async_trait]
impl<S: IssueSourceMappingStore> ManagementCommandUseCase<PreviewIssueSourceCommand> for IssueSourceHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: PreviewIssueSourceCommand,
  ) -> Result<IssueSourceCandidates, ApplicationError> {
    crate::issue::visible_issue(grant, command.issue_id)?;
    self.visible(context, command.issue_id).await?;
    self.store.issue(command.project_id, command.issue_id, None).await?;
    octacity_server_store::validate_source_cursor(&command.continuation).map_err(|_| ApplicationError::invalid())?;
    let connection = self
      .original_source_connection(context, command.project_id, command.issue_id)
      .await?;
    let mappings = self
      .installed_source_mappings(context, command.project_id, connection.as_ref())
      .await?;
    let installed = mappings
      .iter()
      .find(|m| m.project_id == command.project_id && m.source == command.source)
      .ok_or_else(ApplicationError::capability_unavailable)?;
    self
      .reader
      .authorize_connection(
        context,
        installed.project_id,
        installed.source.clone(),
        connection.as_ref(),
      )
      .await?;
    let scan_id = command
      .scan_id
      .unwrap_or_else(|| FactoryDigest::content_sha256(uuid::Uuid::new_v4().as_bytes()));
    let page = self
      .reader
      .read(
        context,
        ReadWorkSourceObservations {
          connection: connection.clone(),
          project_id: command.project_id,
          source: command.source.clone(),
          scan_id,
          window: None,
          continuation: command.continuation,
          limit: MAX_SOURCE_POLL_PAGE_ITEMS,
        },
      )
      .await?;
    if page.observations.len() > usize::from(MAX_SOURCE_POLL_PAGE_ITEMS) {
      return Err(ApplicationError::invalid());
    }
    octacity_server_store::validate_source_cursor(&page.next_cursor).map_err(|_| ApplicationError::invalid())?;
    let now = self.clock.now()?;
    let mut items = Vec::with_capacity(page.observations.len());
    for observed in page.observations {
      if observed.snapshot.item.resource != installed.native_resource {
        return Err(ApplicationError::invalid());
      }
      let selection = IssueSourceSelection {
        connection: connection.clone(),
        project_id: command.project_id,
        source: command.source.clone(),
        item: observed.snapshot.item.clone(),
      };
      observed.validate_issue_read(
        context,
        &selection,
        now,
        octacity_server_store::StoreOperation::AttachIssueSource,
      )?;
      items.push(observed.snapshot.item);
    }
    Ok(IssueSourceCandidates {
      scan_id,
      items,
      next_cursor: page.next_cursor,
    })
  }
}

/// Separately authorized installed Reporter values; never a credential or grant.
#[derive(Clone, Debug, serde::Serialize)]
pub struct IssueSourceWriterChoice {
  /// Exact installed write profile.
  pub binding: WorkSourceBinding,
  /// Finite supported managed effects.
  pub effects: WorkSourceReportingPolicy,
}
/// One current authorized native read profile, independent of workflow selection.
#[derive(Clone, Debug, serde::Serialize)]
pub struct IssueSourceChoice {
  /// Installed read selection.
  pub source: WorkSourceBinding,
  /// Verified native container; clients cannot select an endpoint.
  pub native_resource: octacity_server_factory::ExternalWorkIdentity,
  /// Literal advertised read vocabulary.
  pub readable: WorkSourceReportingPolicy,
  /// Supported managed pull fields.
  pub pull_fields: Vec<crate::IssueField>,
  /// Supported managed push fields within current independent write grants.
  pub push_fields: Vec<crate::IssueField>,
  /// Absent without an independently authorized installed writer.
  pub writer: Option<IssueSourceWriterChoice>,
}
/// Bounded installed choices after current Issue and read authorization.
#[derive(Clone, Debug, serde::Serialize)]
pub struct IssueSourceChoices {
  /// Reviewed task head; changed heads require another review.
  pub issue_version: crate::IssueVersion,
  /// Only current visible installed choices.
  pub items: Vec<IssueSourceChoice>,
}
/// Source administration choices; enumerating them grants no mutation authority.
#[derive(Clone, Debug)]
pub struct GetIssueSourceChoicesQuery {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Visible task to administer.
  pub issue_id: IssueId,
}
impl Query for GetIssueSourceChoicesQuery {
  type Outcome = IssueSourceChoices;
}
impl ManagementAuthorizationTarget for GetIssueSourceChoicesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = AttachIssueSourceCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
#[async_trait::async_trait]
impl<S: IssueSourceMappingStore> ManagementQueryUseCase<GetIssueSourceChoicesQuery> for IssueSourceHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueSourceChoicesQuery,
  ) -> Result<IssueSourceChoices, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue_id)?;
    self.visible(context, query.issue_id).await?;
    let issue = self.store.issue(query.project_id, query.issue_id, None).await?;
    let link = self
      .store
      .issue_source_link(query.project_id, query.issue_id, &context.security_scope().to_store())
      .await?;
    let mut items = Vec::new();
    let connection = link.as_ref().and_then(|l| l.selection.connection.as_ref());
    let mappings = self
      .installed_source_mappings(context, query.project_id, connection)
      .await?;
    for installed in mappings.iter().filter(|m| m.project_id == query.project_id) {
      if link
        .as_ref()
        .is_some_and(|l| !l.supports_profile(&installed.source, &installed.native_resource))
      {
        continue;
      }
      match self
        .reader
        .authorize_connection(context, installed.project_id, installed.source.clone(), connection)
        .await
      {
        Err(ApplicationError::CapabilityUnavailable) => continue,
        result => result?,
      }
      let writer = if let Some(writer) = &installed.writer {
        match writer
          .authorize(
            self.policy.as_ref(),
            context,
            installed.project_id,
            &installed.native_resource,
          )
          .await
        {
          Ok(()) => Some(IssueSourceWriterChoice {
            binding: writer.binding.clone(),
            effects: writer.effects.clone(),
          }),
          Err(ApplicationError::CapabilityUnavailable | ApplicationError::InvalidInput) => None,
          Err(error) => return Err(error),
        }
      } else {
        None
      };
      items.push(IssueSourceChoice {
        source: installed.source.clone(),
        native_resource: installed.native_resource.clone(),
        readable: installed.readable.clone(),
        pull_fields: ISSUE_SOURCE_PULL_FIELDS.to_vec(),
        push_fields: if writer.is_some() {
          ISSUE_SOURCE_PUSH_FIELDS.to_vec()
        } else {
          vec![]
        },
        writer,
      });
    }
    Ok(IssueSourceChoices {
      issue_version: issue.version,
      items,
    })
  }
}

/// Separate retained Source metadata; this read claims no atomic provider snapshot.
#[derive(Clone, Debug, serde::Serialize)]
pub struct IssueSynchronization {
  /// Requirements head reviewed independently of mapping versions.
  pub issue_version: crate::IssueVersion,
  /// Exact current business vocabulary.
  pub workflow: crate::IssueWorkflowPin,
  /// Current retained field authority.
  pub field_policy: crate::IssueFieldPolicy,
  /// Original primary link, absent on ordinary manual tasks.
  pub link: Option<IssueSourceLink>,
  /// Current immutable reviewed mapping, absent until explicitly published.
  pub mapping: Option<PublishedIssueSourceMapping>,
  /// Exact server-computed mapping pin used by reviewed replacement; clients never hash definitions.
  pub mapping_reference: Option<ImmutableReference>,
  /// Safe retained read freshness and status conditions; no native body or credential.
  pub observation: Option<IssueSourceRefreshState>,
}
/// Visible task synchronization metadata; never performs provider I/O.
#[derive(Clone, Debug)]
pub struct GetIssueSynchronizationQuery {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Visible task.
  pub issue_id: IssueId,
}
impl Query for GetIssueSynchronizationQuery {
  type Outcome = IssueSynchronization;
}
impl ManagementAuthorizationTarget for GetIssueSynchronizationQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
#[async_trait::async_trait]
impl<S: IssueSourceMappingStore> ManagementQueryUseCase<GetIssueSynchronizationQuery> for IssueSourceHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueSynchronizationQuery,
  ) -> Result<IssueSynchronization, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue_id)?;
    let issue = self.store.issue(query.project_id, query.issue_id, None).await?;
    let scope = context.security_scope().to_store();
    let link = self
      .store
      .issue_source_link(query.project_id, query.issue_id, &scope)
      .await?;
    let (mapping, observation) = if link.is_some() {
      (
        self
          .store
          .issue_source_mapping(query.project_id, query.issue_id, &scope, None)
          .await?,
        self
          .store
          .issue_source_refresh_state(query.project_id, query.issue_id, &scope)
          .await?,
      )
    } else {
      (None, None)
    };
    Ok(IssueSynchronization {
      issue_version: issue.version,
      workflow: crate::IssueWorkflowPin {
        id: issue.workflow_id,
        version: issue.workflow_version,
      },
      field_policy: self.store.issue_field_policy(query.project_id, query.issue_id).await?,
      mapping_reference: mapping.as_ref().map(PublishedIssueSourceMapping::reference),
      link,
      mapping,
      observation,
    })
  }
}

/// Reads one original mapping publication under current Issue and link visibility.
#[derive(Clone, Debug)]
pub struct GetIssueSourceMappingQuery {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Visible task.
  pub issue_id: IssueId,
  /// Exact historical mapping version; never a mutable latest lookup.
  pub version: IssueSourceMappingVersion,
}
impl Query for GetIssueSourceMappingQuery {
  type Outcome = PublishedIssueSourceMapping;
}
impl ManagementAuthorizationTarget for GetIssueSourceMappingQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
#[async_trait::async_trait]
impl<S: IssueSourceMappingStore> ManagementQueryUseCase<GetIssueSourceMappingQuery> for IssueSourceHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueSourceMappingQuery,
  ) -> Result<PublishedIssueSourceMapping, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue_id)?;
    self.store.issue(query.project_id, query.issue_id, None).await?;
    let scope = context.security_scope().to_store();
    let missing = || {
      ApplicationError::Store(octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Issue,
      })
    };
    if self
      .store
      .issue_source_link(query.project_id, query.issue_id, &scope)
      .await?
      .is_none()
    {
      return Err(missing());
    }
    self
      .store
      .issue_source_mapping(query.project_id, query.issue_id, &scope, Some(query.version))
      .await?
      .ok_or_else(missing)
  }
}
