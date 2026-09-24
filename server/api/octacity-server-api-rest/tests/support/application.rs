use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use octacity_server_api_rest::v1::{
  AgentManagementApplication, ArtifactManagementApplication, BuildLogSearchManagementApplication,
  BuildManagementApplication, BuildResultRetentionManagementApplication, CacheManagementApplication,
  CatalogManagementApplication, ConfigurationManagementApplication, DefinitionManagementApplication,
  ExecutionManagementApplication, InternalTriggerManagementApplication, JobEventManagementApplication,
  ManagementApplication, ManagementApplicationHandlers, ManualTriggerManagementApplication,
  PipelineManagementApplication, ProjectManagementApplication, ScheduleManagementApplication,
};
use octacity_server_application::{
  ApplicationError, CommandHandler, GetBuildResultRetentionQuery, JobEventPageProjection, JobEventProjection,
  PlaceBuildResultHoldCommand, QueryHandler, ReadJobEventsQuery, ReleaseBuildResultHoldCommand,
};

use crate::RecordingApplication;

pub struct JobEventApplication;

#[async_trait]
impl QueryHandler<ReadJobEventsQuery> for JobEventApplication {
  type Error = ApplicationError;

  async fn handle_query(&self, query: ReadJobEventsQuery) -> Result<JobEventPageProjection, Self::Error> {
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
  E: QueryHandler<ReadJobEventsQuery, Error = ApplicationError> + 'static,
{
  recording_management_application_with_retention(application.clone(), job_events, application)
}

pub fn recording_management_application_with_retention<E, R>(
  application: Arc<RecordingApplication>,
  job_events: Arc<E>,
  retention: Arc<R>,
) -> ManagementApplication
where
  E: QueryHandler<ReadJobEventsQuery, Error = ApplicationError> + 'static,
  R: QueryHandler<GetBuildResultRetentionQuery, Error = ApplicationError>
    + CommandHandler<PlaceBuildResultHoldCommand, Error = ApplicationError>
    + CommandHandler<ReleaseBuildResultHoldCommand, Error = ApplicationError>
    + 'static,
{
  ManagementApplication::new(
    ["native".to_owned()],
    Duration::from_secs(900),
    octacity_server_application::AgentEnrollmentSecretKey::new([7; 32]),
    ManagementApplicationHandlers::new(
      CatalogManagementApplication::new(
        ProjectManagementApplication::new(Arc::clone(&application)),
        PipelineManagementApplication::new(Arc::clone(&application)),
        ConfigurationManagementApplication::new(Arc::clone(&application)),
        DefinitionManagementApplication::new(Arc::clone(&application), Arc::clone(&application)),
        ScheduleManagementApplication::new(Arc::clone(&application)),
        InternalTriggerManagementApplication::new(Arc::clone(&application)),
      ),
      AgentManagementApplication::new(
        Arc::clone(&application),
        Arc::clone(&application),
        Arc::clone(&application),
      ),
      ExecutionManagementApplication::new(
        BuildManagementApplication::new(Arc::clone(&application)),
        ManualTriggerManagementApplication::new(Arc::clone(&application)),
        JobEventManagementApplication::new(job_events),
        ArtifactManagementApplication::new(Arc::clone(&application)),
        CacheManagementApplication::new(Arc::clone(&application)),
        BuildLogSearchManagementApplication::new(Arc::clone(&application)),
        BuildResultRetentionManagementApplication::new(retention),
      ),
    ),
  )
  .unwrap()
}
