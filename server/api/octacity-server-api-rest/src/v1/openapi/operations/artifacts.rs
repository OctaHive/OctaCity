use super::{ManagementOperation, ParameterProfile};

pub(super) const OPERATIONS: &[ManagementOperation] = &[
  operation!(
    "GET",
    "/api/v1/builds/{build_id}/artifacts",
    "listBuildArtifacts",
    "Artifacts",
    "List published logical outputs for a Build",
    None,
    "ArtifactPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::ArtifactList),
  operation!(
    "GET",
    "/api/v1/artifacts/{artifact_id}",
    "getArtifact",
    "Artifacts",
    "Get published logical output metadata",
    None,
    "ArtifactResource",
    "200",
    false,
    false
  ),
  operation!(
    "POST",
    "/api/v1/artifacts/{artifact_id}/download",
    "authorizeArtifactDownload",
    "Artifacts",
    "Create a short-lived Artifact download capability",
    None,
    "ArtifactDownload",
    "200",
    false,
    false
  ),
  operation!(
    "GET",
    "/api/v1/builds/{build_id}/cache-sessions",
    "listBuildCacheSessions",
    "Cache",
    "List secret-free cache-session diagnostics for a Build",
    None,
    "CacheSessionPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::CacheSessionList),
  operation!(
    "GET",
    "/api/v1/cache-sessions/{cache_session_id}",
    "getCacheSession",
    "Cache",
    "Get secret-free cache-session diagnostics",
    None,
    "CacheSessionResource",
    "200",
    false,
    false
  ),
];
