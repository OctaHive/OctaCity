//! Explicit bounded content expansion; metadata reads never enter this path.
use super::*;
/// Existing retained Artifact reader download ceiling, shared with HTTP documentation.
pub use octacity_artifact_store::MAX_ARTIFACT_READ_BYTES as MAX_ISSUE_ARTIFACT_DOWNLOAD_BYTES;
/// Hard encoded preview ceiling; raw downloads retain the existing reader bound.
pub const MAX_ISSUE_ARTIFACT_PREVIEW_BYTES: u64 = crate::raster_preview::MAX_RASTER_BYTES;
/// Explicit safe expansion mode; there is no arbitrary URL or executable preview mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueArtifactReadMode {
  /// Bounded independently validated text/raster preview.
  Preview,
  /// Explicit attachment download through existing retained byte authority.
  Download,
}
/// Exact original execution membership plus independent Artifact download permission.
#[derive(Clone, Debug)]
pub struct ReadIssueArtifactContentQuery {
  /// Original visible Issue-associated Work.
  pub execution: GetIssueWorkExecutionQuery,
  /// Logical identity must be in that frozen input/output inventory.
  pub artifact_id: octacity_server_domain::ArtifactId,
  /// Explicit preview or download; list visibility grants neither.
  pub mode: IssueArtifactReadMode,
}
impl Query for ReadIssueArtifactContentQuery {
  type Outcome = IssueArtifactContent;
}
impl ManagementAuthorizationTarget for ReadIssueArtifactContentQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.execution.management_resource()
  }
}
/// Dedicated bounded body; deliberately not serializable into metadata, audit or generic events.
pub struct IssueArtifactContent {
  /// Safe forced content type, never a provider-selected executable rendering mode.
  pub media_type: String,
  /// Original bounded verified bytes; the REST boundary supplies no-store and disposition.
  pub bytes: Vec<u8>,
}
impl std::fmt::Debug for IssueArtifactContent {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("IssueArtifactContent")
      .field("media_type", &self.media_type)
      .finish_non_exhaustive()
  }
}
#[async_trait::async_trait]
impl<S: IssueExecutionStore + IssueStore, F: FactoryDiscoveryStore + octacity_server_store::FactoryRunStore + 'static>
  ManagementQueryUseCase<ReadIssueArtifactContentQuery> for IssueArtifactHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ReadIssueArtifactContentQuery,
  ) -> Result<IssueArtifactContent, ApplicationError> {
    let association = self.executions.visible_issue_work(grant, &query.execution).await?;
    self.executions.visible_execution_run(context, &association).await?;
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Artifact,
      query.artifact_id,
    )
    .await?;
    crate::factory_configuration_choices::require_authorized_reference(
      self.policy.as_ref(),
      context,
      ManagementAction::Download,
      ManagementResourceKind::Artifact,
      query.artifact_id,
    )
    .await?;
    let snapshot = self.executions.factory.factory_run_snapshot(association.run_id).await?;
    let item = self
      .visible_inventory(context, &snapshot)
      .await?
      .into_iter()
      .find(|a| a.artifact.artifact_id() == query.artifact_id)
      .ok_or_else(ApplicationError::invalid)?;
    let artifact = item.artifact.clone();
    let now = self.clock.now()?;
    let maximum_bytes = match query.mode {
      IssueArtifactReadMode::Preview => MAX_ISSUE_ARTIFACT_PREVIEW_BYTES,
      IssueArtifactReadMode::Download => MAX_ISSUE_ARTIFACT_DOWNLOAD_BYTES,
    };
    if artifact.encoded_size() > maximum_bytes {
      return Err(ApplicationError::capability_unavailable());
    }
    let request = ReadFactoryArtifact {
      project_id: association.project_id,
      artifact,
      maximum_bytes,
      observed_at: now,
      required_until: Timestamp::from_unix_millis(
        now.unix_millis().checked_add(1).ok_or_else(ApplicationError::invalid)?,
      )
      .map_err(|_| ApplicationError::invalid())?,
    };
    let metadata = self.reader.metadata(request.clone()).await?;
    if metadata.availability != IssueArtifactAvailability::Available {
      return Err(ApplicationError::unavailable());
    }
    let bytes = self.reader.content(request).await?;
    if bytes.len() as u64 > maximum_bytes {
      return Err(ApplicationError::unavailable());
    }
    self.executions.visible_issue_work(grant, &query.execution).await?;
    self.executions.visible_execution_run(context, &association).await?;
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Artifact,
      query.artifact_id,
    )
    .await?;
    crate::factory_configuration_choices::require_authorized_reference(
      self.policy.as_ref(),
      context,
      ManagementAction::Download,
      ManagementResourceKind::Artifact,
      query.artifact_id,
    )
    .await?;
    if !self.visible_artifact(context, &item).await {
      return Err(ApplicationError::unavailable());
    }
    let result = if query.mode == IssueArtifactReadMode::Download {
      IssueArtifactContent {
        media_type: "application/octet-stream".into(),
        bytes,
      }
    } else {
      safe_preview(&metadata.media_type, bytes)?
    };
    if metadata
      .expires_at
      .is_some_and(|end| self.clock.now().map_or(true, |now| now >= end))
    {
      return Err(ApplicationError::unavailable());
    }
    Ok(result)
  }
}

fn safe_preview(media_type: &str, bytes: Vec<u8>) -> Result<IssueArtifactContent, ApplicationError> {
  match media_type {
    "text/plain" | "application/json" | "text/markdown" => {
      std::str::from_utf8(&bytes).map_err(|_| ApplicationError::unavailable())?;
      Ok(IssueArtifactContent {
        media_type: "text/plain; charset=utf-8".into(),
        bytes,
      })
    }
    "image/png" | "image/jpeg" | "image/webp" => {
      let bytes = crate::raster_preview::png(media_type, bytes)?;
      Ok(IssueArtifactContent {
        media_type: "image/png".into(),
        bytes,
      })
    }
    _ => Err(ApplicationError::capability_unavailable()),
  }
}
/// Strict decoded raster dimension ceiling.
pub const MAX_ISSUE_ARTIFACT_PREVIEW_DIMENSION: u32 = crate::raster_preview::MAX_RASTER_DIMENSION;
/// Bounded raster decoder allocation budget.
pub const MAX_ISSUE_ARTIFACT_PREVIEW_ALLOC_BYTES: u64 = crate::raster_preview::MAX_RASTER_ALLOC_BYTES;
