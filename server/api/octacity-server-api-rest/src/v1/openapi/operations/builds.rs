use super::{ManagementOperation, ParameterProfile};

pub(super) const OPERATIONS: &[ManagementOperation] = &[
  operation!(
    "GET",
    "/api/v1/builds/{build_id}",
    "getBuild",
    "Builds",
    "Get a Build and its current Attempt",
    None,
    "BuildResource",
    "200",
    false,
    false
  ),
  operation!(
    "GET",
    "/api/v1/builds/{build_id}/retention",
    "getBuildResultRetention",
    "Build Results",
    "Get automatic-retention deadlines and hold state",
    None,
    "BuildResultRetentionResource",
    "200",
    false,
    false
  ),
  operation!(
    "POST",
    "/api/v1/builds/{build_id}/retention/hold",
    "placeBuildResultRetentionHold",
    "Build Results",
    "Place a permanent or time-bounded Build Result hold",
    Some("PlaceBuildResultHoldRequest"),
    "BuildResultRetentionMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    "POST",
    "/api/v1/builds/{build_id}/retention/hold/release",
    "releaseBuildResultRetentionHold",
    "Build Results",
    "Release the active Build Result hold",
    None,
    "BuildResultRetentionMutationResponse",
    "200",
    true,
    true
  )
  .with_precondition_failed_response(),
  operation!(
    "POST",
    "/api/v1/builds/{build_id}/cancel",
    "cancelBuild",
    "Builds",
    "Cancel an active Build",
    None,
    "CancelBuildResponse",
    "200",
    true,
    false
  ),
  operation!(
    "POST",
    "/api/v1/builds/{build_id}/retry",
    "retryBuild",
    "Builds",
    "Retry a failed Build",
    None,
    "RetryBuildResponse",
    "201",
    true,
    false
  ),
  operation!(
    "GET",
    "/api/v1/attempts/{attempt_id}",
    "getAttempt",
    "Builds",
    "Get an Attempt diagnostic DAG",
    None,
    "AttemptResource",
    "200",
    false,
    false
  ),
  operation!(
    "GET",
    "/api/v1/jobs/{job_id}",
    "getJob",
    "Jobs",
    "Get Job execution diagnostics",
    None,
    "JobResource",
    "200",
    false,
    false
  ),
  operation!(
    "GET",
    "/api/v1/jobs/{job_id}/events",
    "readJobEvents",
    "Jobs",
    "Read or follow ordered Job events",
    None,
    "JobEventPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::JobEvents),
  operation!(
    "GET",
    "/api/v1/projects/{project_id}/build-logs/search",
    "searchBuildLogs",
    "Build Logs",
    "Search redacted committed Build logs",
    None,
    "BuildLogSearchPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::BuildLogSearch),
];
