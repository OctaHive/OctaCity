use octacity_server_application::{
  CancelBuildCommand, GetAttemptQuery, GetBuildQuery, GetBuildResultRetentionQuery, GetJobQuery,
  ListProjectBuildsQuery, PlaceBuildResultHoldCommand, ReadJobEventsQuery, ReleaseBuildResultHoldCommand,
  RetryBuildCommand, SearchBuildLogsQuery,
};

use super::{ManagementOperation, ParameterProfile};

pub(super) const OPERATIONS: &[ManagementOperation] = &[
  operation!(
    ListProjectBuildsQuery,
    "GET",
    "/api/v1/projects/{project_id}/builds",
    "listProjectBuilds",
    "Builds",
    "List Builds owned by a Project",
    None,
    "BuildSummaryPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::ProjectBuildList)
  .with_visibility::<ListProjectBuildsQuery>(),
  operation!(
    GetBuildQuery,
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
    GetBuildResultRetentionQuery,
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
    PlaceBuildResultHoldCommand,
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
    ReleaseBuildResultHoldCommand,
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
    CancelBuildCommand,
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
    RetryBuildCommand,
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
    GetAttemptQuery,
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
    GetJobQuery,
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
    ReadJobEventsQuery,
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
  .with_parameters(ParameterProfile::JobEvents)
  .with_visibility::<ReadJobEventsQuery>(),
  operation!(
    SearchBuildLogsQuery,
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
  .with_parameters(ParameterProfile::BuildLogSearch)
  .with_visibility::<SearchBuildLogsQuery>(),
];
