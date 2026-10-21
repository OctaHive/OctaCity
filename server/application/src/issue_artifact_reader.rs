//! Safe metadata projection at the existing retained Artifact authority.
use super::*;
use crate::{IssueArtifactAvailability, IssueArtifactMetadata, IssueArtifactOrigin, IssueArtifactReader};

#[async_trait::async_trait]
impl<A: ArtifactRecordStore, S: BuildQueryStore, B: ArtifactByteReader> IssueArtifactReader
  for FactoryArtifactReader<A, S, B>
{
  async fn metadata(&self, request: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    if request.artifact.encoded_size() > request.maximum_bytes || request.maximum_bytes > MAX_ARTIFACT_READ_BYTES {
      return Err(ApplicationError::invalid());
    }
    if let Some(store) = &self.source_inputs
      && let Some(input) = store
        .source_artifact(request.project_id, request.artifact.artifact_id())
        .await?
    {
      return private_metadata(
        &request,
        input.project_id,
        &input.object,
        input.published_at,
        input.expires_at,
        IssueArtifactOrigin::SourceInput,
      );
    }
    if let Some(store) = &self.project_inputs
      && let Some(input) = store
        .project_input_artifact(request.project_id, request.artifact.artifact_id())
        .await?
    {
      return private_metadata(
        &request,
        input.project_id,
        &input.object,
        input.published_at,
        input.expires_at,
        IssueArtifactOrigin::ProjectInput,
      );
    }
    let record = self.artifacts.artifact(request.artifact.artifact_id()).await?;
    self.verify_record_reference(&request, &record).await?;
    let identity = record.identity();
    let expires_at = identity.retention.delete_after();
    let availability = if record.state() == octacity_server_store::ArtifactState::Expired
      || expires_at.is_some_and(|end| request.observed_at >= end)
    {
      IssueArtifactAvailability::Expired
    } else if record.state() == octacity_server_store::ArtifactState::Published
      && record.published_at().is_some_and(|at| at <= request.observed_at)
    {
      IssueArtifactAvailability::Available
    } else {
      IssueArtifactAvailability::Unavailable
    };
    Ok(IssueArtifactMetadata {
      name: identity.logical_name.as_str().to_owned(),
      media_type: identity.media_type.as_str().to_owned(),
      origin: IssueArtifactOrigin::BuildOutput,
      availability,
      published_at: record.published_at(),
      expires_at,
    })
  }
  async fn content(&self, request: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    Ok(self.read_input(request).await?.bytes)
  }
}
fn private_metadata(
  request: &ReadFactoryArtifact,
  project: ProjectId,
  object: &octacity_server_store::FactoryInputObject,
  published_at: Timestamp,
  expires_at: Timestamp,
  origin: IssueArtifactOrigin,
) -> Result<IssueArtifactMetadata, ApplicationError> {
  if project != request.project_id || object.artifact != request.artifact || published_at > request.observed_at {
    return Err(ApplicationError::invalid());
  }
  Ok(IssueArtifactMetadata {
    name: match object.role {
      octacity_server_store::FactoryInputRole::Task => "task",
      octacity_server_store::FactoryInputRole::Acceptance => "acceptance",
    }
    .to_owned(),
    media_type: object.media_type.as_str().to_owned(),
    origin,
    availability: if request.observed_at >= expires_at {
      IssueArtifactAvailability::Expired
    } else {
      IssueArtifactAvailability::Available
    },
    published_at: Some(published_at),
    expires_at: Some(expires_at),
  })
}
