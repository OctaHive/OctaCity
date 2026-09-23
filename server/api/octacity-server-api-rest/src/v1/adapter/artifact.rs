use std::sync::Arc;

use axum::{
  Extension, Json,
  extract::{Path, Query, State, rejection::QueryRejection},
};
use octacity_server_application::{
  ArtifactDownloadProjection, ArtifactProjection, ArtifactProjectionKind, AuthorizeArtifactDownloadQuery,
  GetArtifactQuery, ListBuildArtifactsQuery,
};
use serde::Deserialize;

use super::{ApiError, ManagementApplication, application_error, now_unix_ms, query_error};
use crate::{
  RequestId,
  v1::{ArtifactDownload, ArtifactOutputType, ArtifactPage, ArtifactResource, ErrorCode},
};

pub(crate) const DEFAULT_ARTIFACT_LIMIT: u16 = 50;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArtifactListQuery {
  #[serde(default = "default_limit")]
  limit: u16,
}

const fn default_limit() -> u16 {
  DEFAULT_ARTIFACT_LIMIT
}

pub(super) async fn get_artifact(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(artifact_id): Path<String>,
) -> Result<Json<ArtifactResource>, ApiError> {
  let artifact_id = artifact_id.parse().map_err(|_| invalid(&request_id))?;
  application
    .artifacts
    .get
    .handle_query(GetArtifactQuery { artifact_id })
    .await
    .map(resource)
    .map(Json)
    .map_err(|error| application_error(error.classification(), &request_id))
}

pub(super) async fn list_build_artifacts(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(build_id): Path<String>,
  query: Result<Query<ArtifactListQuery>, QueryRejection>,
) -> Result<Json<ArtifactPage>, ApiError> {
  let build_id = build_id.parse().map_err(|_| invalid(&request_id))?;
  let Query(query) = query.map_err(|error| query_error(error, &request_id))?;
  application
    .artifacts
    .list
    .handle_query(ListBuildArtifactsQuery {
      build_id,
      limit: query.limit,
    })
    .await
    .map(|items| ArtifactPage {
      items: items.into_iter().map(resource).collect(),
    })
    .map(Json)
    .map_err(|error| application_error(error.classification(), &request_id))
}

pub(super) async fn authorize_artifact_download(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(artifact_id): Path<String>,
) -> Result<Json<ArtifactDownload>, ApiError> {
  let artifact_id = artifact_id.parse().map_err(|_| invalid(&request_id))?;
  application
    .artifacts
    .download
    .handle_query(AuthorizeArtifactDownloadQuery {
      artifact_id,
      observed_at_unix_ms: now_unix_ms(&request_id)?,
    })
    .await
    .map(download)
    .map(Json)
    .map_err(|error| application_error(error.classification(), &request_id))
}

fn resource(value: ArtifactProjection) -> ArtifactResource {
  ArtifactResource {
    id: value.id.to_string(),
    build_id: value.build_id.to_string(),
    attempt_id: value.attempt_id.to_string(),
    job_id: value.job_id.to_string(),
    name: value.name,
    output_type: match value.kind {
      ArtifactProjectionKind::Artifact => ArtifactOutputType::Artifact,
      ArtifactProjectionKind::Report { format } => ArtifactOutputType::Report { format },
    },
    media_type: value.media_type,
    size_bytes: value.size_bytes,
    sha256: value.sha256,
    published_at_unix_ms: value.published_at.unix_millis(),
  }
}

fn download(value: ArtifactDownloadProjection) -> ArtifactDownload {
  ArtifactDownload {
    artifact: resource(value.artifact),
    get_url: value.url,
    expires_at_unix_ms: value.expires_at_unix_ms,
  }
}

fn invalid(request_id: &RequestId) -> ApiError {
  ApiError::new(
    axum::http::StatusCode::BAD_REQUEST,
    ErrorCode::InvalidRequest,
    "Artifact identity is invalid",
    request_id,
  )
}
