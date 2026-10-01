use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use octacity_server_api_rest::v1::{
  AgentManagementApplication, ArtifactManagementApplication, AuditManagementApplication,
  BuildLogSearchManagementApplication, BuildManagementApplication, BuildResultRetentionManagementApplication,
  CacheManagementApplication, CatalogManagementApplication, ConfigurationManagementApplication,
  DefinitionManagementApplication, ExecutionManagementApplication, InternalTriggerManagementApplication,
  JobEventManagementApplication, ManagementApplication, ManagementApplicationHandlers,
  ManualTriggerManagementApplication, OperationalMetadataManagementApplication, PipelineManagementApplication,
  ProjectManagementApplication, ScheduleManagementApplication,
};
use octacity_server_application::{
  ApplicationError, AuthorizedCommandHandler, AuthorizedQueryHandler, GetBuildResultRetentionQuery,
  JobEventPageProjection, JobEventProjection, ManagementAuthorizationGrant, ManagementAuthorizationPolicy,
  ManagementCommandUseCase, ManagementQueryUseCase, ManagementRequestContext, PlaceBuildResultHoldCommand,
  ReadJobEventsQuery, ReleaseBuildResultHoldCommand, TrustedNetworkManagementPolicy,
};

use crate::RecordingApplication;

pub struct JobEventApplication;

#[async_trait]
impl ManagementQueryUseCase<ReadJobEventsQuery> for JobEventApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: ReadJobEventsQuery,
  ) -> Result<JobEventPageProjection, Self::Error> {
    assert_eq!(query.after_sequence, 4);
    assert_eq!(query.limit, 2);
    assert_eq!(query.wait.as_millis(), 25);
    Ok(JobEventPageProjection {
      events: vec![JobEventProjection {
        sequence: 5,
        kind: "progress".to_owned(),
        occurred_at_unix_ms: 1_234,
        payload: serde_json::json!({"step": "compile"}),
      }],
      cursor: 5,
    })
  }
}

pub fn recording_management_application<E>(
  application: Arc<RecordingApplication>,
  job_events: Arc<E>,
) -> ManagementApplication
where
  E: ManagementQueryUseCase<ReadJobEventsQuery, Error = ApplicationError> + 'static,
{
  recording_management_application_with_policy(application, job_events, Arc::new(TrustedNetworkManagementPolicy))
}

pub fn recording_management_application_with_policy<E>(
  application: Arc<RecordingApplication>,
  job_events: Arc<E>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
) -> ManagementApplication
where
  E: ManagementQueryUseCase<ReadJobEventsQuery, Error = ApplicationError> + 'static,
{
  recording_management_application_with_retention_and_policy(application.clone(), job_events, application, policy)
}

pub fn recording_management_application_with_retention<E, R>(
  application: Arc<RecordingApplication>,
  job_events: Arc<E>,
  retention: Arc<R>,
) -> ManagementApplication
where
  E: ManagementQueryUseCase<ReadJobEventsQuery, Error = ApplicationError> + 'static,
  R: ManagementQueryUseCase<GetBuildResultRetentionQuery, Error = ApplicationError>
    + ManagementCommandUseCase<PlaceBuildResultHoldCommand, Error = ApplicationError>
    + ManagementCommandUseCase<ReleaseBuildResultHoldCommand, Error = ApplicationError>
    + 'static,
{
  recording_management_application_with_retention_and_policy(
    application,
    job_events,
    retention,
    Arc::new(TrustedNetworkManagementPolicy),
  )
}

fn recording_management_application_with_retention_and_policy<E, R>(
  application: Arc<RecordingApplication>,
  job_events: Arc<E>,
  retention: Arc<R>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
) -> ManagementApplication
where
  E: ManagementQueryUseCase<ReadJobEventsQuery, Error = ApplicationError> + 'static,
  R: ManagementQueryUseCase<GetBuildResultRetentionQuery, Error = ApplicationError>
    + ManagementCommandUseCase<PlaceBuildResultHoldCommand, Error = ApplicationError>
    + ManagementCommandUseCase<ReleaseBuildResultHoldCommand, Error = ApplicationError>
    + 'static,
{
  let commands = Arc::new(AuthorizedCommandHandler::new(policy.clone(), application.clone()));
  let queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), application.clone()));
  let job_event_queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), job_events));
  let retention_commands = Arc::new(AuthorizedCommandHandler::new(policy.clone(), retention.clone()));
  let retention_queries = Arc::new(AuthorizedQueryHandler::new(policy, retention));
  ManagementApplication::new(
    ["native".to_owned()],
    Duration::from_secs(900),
    octacity_server_application::AgentEnrollmentSecretKey::new([7; 32]),
    ManagementApplicationHandlers::new(
      OperationalMetadataManagementApplication::new(queries.clone()),
      CatalogManagementApplication::new(
        ProjectManagementApplication::new(commands.clone(), queries.clone()),
        PipelineManagementApplication::new(commands.clone(), queries.clone()),
        ConfigurationManagementApplication::new(commands.clone(), queries.clone()),
        DefinitionManagementApplication::new(commands.clone(), commands.clone()),
        ScheduleManagementApplication::new(commands.clone(), queries.clone()),
        InternalTriggerManagementApplication::new(commands.clone(), queries.clone()),
      ),
      AgentManagementApplication::new(
        commands.clone(),
        queries.clone(),
        commands.clone(),
        queries.clone(),
        commands.clone(),
      ),
      ExecutionManagementApplication::new(
        BuildManagementApplication::new(commands.clone(), queries.clone()),
        ManualTriggerManagementApplication::new(commands),
        JobEventManagementApplication::new(job_event_queries),
        ArtifactManagementApplication::new(queries.clone()),
        CacheManagementApplication::new(queries.clone()),
        BuildLogSearchManagementApplication::new(queries.clone()),
        BuildResultRetentionManagementApplication::new(retention_commands, retention_queries),
      ),
      AuditManagementApplication::new(queries),
    ),
  )
  .unwrap()
}
