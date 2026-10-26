//! Bounded Project preview consumes only the existing observation port; it owns no import journal.
use super::*;
use octacity_server_factory::WorkSourceConnectionRef;
use octacity_server_store::{
  IntegrationAdministrationStore, IntegrationConnectionStore, IntegrationProjectAccessStore,
};
use serde::Serialize;

/// Read-only task-source preview before an Issue or Factory configuration exists.
#[derive(Clone, Debug)]
pub struct PreviewProjectIssueSourcesQuery {
  /// Exact reviewed Project and connection metadata.
  pub selection: ListWorkSourceInputChoicesQuery,
  /// Current Project grant revision reviewed by the caller.
  pub access_version: octacity_server_domain::IntegrationVersion,
  /// Exact Project/connection-bound installed reader.
  pub source: WorkSourceBinding,
  /// Positive native summary ceiling, at most the shared preview bound.
  pub limit: u16,
}
impl Query for PreviewProjectIssueSourcesQuery {
  type Outcome = ProjectIssueSourcePreview;
}
impl ManagementAuthorizationTarget for PreviewProjectIssueSourcesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ListWorkSourceInputChoicesQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.selection.management_resource()
  }
}
/// Safe native display facts; task bodies, private cursors and acceptance content are absent.
#[derive(Clone, Debug, Serialize)]
pub struct ProjectIssueSourcePreviewItem {
  /// Verified native container/item, independent of any code Repository.
  pub item: WorkSourceItem,
  /// Provider's bounded human-facing key.
  pub display_id: String,
  /// Bounded untrusted display title.
  pub title: String,
  /// Opaque provider state, with no workflow meaning.
  pub state: String,
  /// Bounded literal provider labels.
  pub labels: Vec<String>,
}
/// Preview is display evidence only; it authorizes no import, execution or remote write.
#[derive(Clone, Debug, Serialize)]
pub struct ProjectIssueSourcePreview {
  /// Original exact reader selection.
  pub source: WorkSourceBinding,
  /// Trusted operation start, independent of a later import.
  #[serde(rename = "observed_at_unix_ms")]
  pub observed_at: Timestamp,
  /// Classified provider failure; an empty successful page has no failure.
  pub failure: Option<WorkSourceFailure>,
  /// Bounded provider advice, without scheduling a retry.
  pub retry_after_millis: Option<u64>,
  /// Safe bounded summaries, never requirements or admission promises.
  pub items: Vec<ProjectIssueSourcePreviewItem>,
  /// Additional native data exists; no continuation or progress is exposed.
  pub has_more: bool,
}
#[async_trait::async_trait]
impl<S: IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore + 'static>
  ManagementQueryUseCase<PreviewProjectIssueSourcesQuery> for IssueSourceHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: PreviewProjectIssueSourcesQuery,
  ) -> Result<ProjectIssueSourcePreview, ApplicationError> {
    if !(1..=MAX_WORK_SOURCE_PREVIEW_ITEMS).contains(&query.limit) {
      return Err(ApplicationError::invalid());
    }
    let observed_at = self.clock.now()?;
    let project = query.selection.project_id;
    let choices = self
      .execute_management_query(
        context,
        grant,
        ListProjectIssueSourceChoicesQuery {
          selection: query.selection,
        },
      )
      .await?;
    if choices.access_version != query.access_version {
      return Err(ApplicationError::PreconditionFailed);
    }
    let choice = choices
      .items
      .iter()
      .find(|c| c.binding == query.source)
      .ok_or_else(|| FactoryValidationDiagnostics::unavailable("/source"))?;
    let request = ReadWorkSourceObservations {
      connection: Some(WorkSourceConnectionRef {
        id: choices.connection_id,
        version: choices.connection_version,
        access_version: choices.access_version,
        source_configuration: choice.connection_configuration.clone(),
        reporter_configuration: None,
      }),
      project_id: project,
      source: query.source.clone(),
      scan_id: FactoryDigest::content_sha256(uuid::Uuid::new_v4().as_bytes()),
      window: None,
      continuation: None,
      limit: query.limit,
    };
    let mut result = ProjectIssueSourcePreview {
      source: query.source,
      observed_at,
      failure: None,
      retry_after_millis: None,
      items: vec![],
      has_more: false,
    };
    let page = match self.reader.read(context, request).await {
      Ok(page) => page,
      Err(ApplicationError::SourceDependency {
        class,
        retry_after_millis,
      }) => {
        result.failure = Some(class);
        result.retry_after_millis = retry_after_millis.map(|v| u64::from(v.min(MAX_WORK_SOURCE_PREVIEW_RETRY_MILLIS)));
        return Ok(result);
      }
      Err(error) => return Err(error),
    };
    if page.observations.len() > usize::from(query.limit) {
      return Err(ApplicationError::invalid());
    }
    let now = self.clock.now()?;
    result.has_more = page.next_cursor.is_some();
    for observation in page.observations {
      if observation.snapshot.item.resource != choice.native_id {
        return Err(ApplicationError::invalid());
      }
      observation.validate_issue_read(
        context,
        &IssueSourceSelection {
          project_id: project,
          source: result.source.clone(),
          item: observation.snapshot.item.clone(),
        },
        now,
        octacity_server_store::StoreOperation::ImportIssueSource,
      )?;
      let snapshot = observation.snapshot;
      let mut title = snapshot.title;
      let mut end = title.len().min(MAX_WORK_SOURCE_PREVIEW_TITLE_BYTES);
      while !title.is_char_boundary(end) {
        end -= 1;
      }
      title.truncate(end);
      result.items.push(ProjectIssueSourcePreviewItem {
        item: snapshot.item,
        display_id: snapshot.display_id,
        title,
        state: snapshot.state,
        labels: snapshot.labels,
      });
    }
    Ok(result)
  }
}
