use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{body::to_bytes, http::StatusCode};
use octacity_server_application::{
  ApplicationError, BuildResultHoldProjection, BuildResultHoldStateProjection, BuildResultRetentionCommandOutcome,
  BuildResultRetentionDeadlinesProjection, BuildResultRetentionProjection, BuildResultVisibilityProjection,
  CommandHandler, GetBuildResultRetentionQuery, MutationDisposition, PlaceBuildResultHoldCommand, QueryHandler,
  ReleaseBuildResultHoldCommand, RetentionAuditIdentityProjection, RetentionHoldVersion, Timestamp,
};
use tower::ServiceExt as _;

use super::{
  RecordingApplication, empty_request, json_request, management_router_with_application,
  recording_management_application_with_retention,
};

macro_rules! retention_projection {
  ($build_id:expr, $observed_at:expr, $state:expr, $version:expr, $release_audit:expr $(,)?) => {{
    let state = $state;
    let created_at = Timestamp::from_unix_millis($observed_at.unix_millis() - 1).unwrap();
    BuildResultRetentionProjection {
      build_id: $build_id,
      deadlines: BuildResultRetentionDeadlinesProjection {
        metadata: $observed_at,
        logs: $observed_at,
        artifacts: $observed_at,
        reports: $observed_at,
      },
      visibility: BuildResultVisibilityProjection {
        metadata: true,
        logs: true,
        artifacts: true,
        reports: true,
      },
      hold: Some(BuildResultHoldProjection {
        version: RetentionHoldVersion::new($version).unwrap(),
        reason: "incident investigation".to_owned(),
        created_at,
        expires_at: (state == BuildResultHoldStateProjection::Expired).then_some($observed_at),
        released_at: (state == BuildResultHoldStateProjection::Released).then_some($observed_at),
        state,
        creation_audit: RetentionAuditIdentityProjection {
          actor_kind: "unauthenticated_management".to_owned(),
          actor_identity: None,
          request_identity: "place-request".to_owned(),
        },
        release_audit: $release_audit,
      }),
    }
  }};
}

#[derive(Default)]
struct RetentionApplication {
  placement_count: Mutex<u8>,
}

#[async_trait]
impl QueryHandler<GetBuildResultRetentionQuery> for RetentionApplication {
  type Error = ApplicationError;

  async fn handle_query(
    &self,
    query: GetBuildResultRetentionQuery,
  ) -> Result<BuildResultRetentionProjection, Self::Error> {
    Ok(retention_projection!(
      query.build_id,
      query.observed_at,
      BuildResultHoldStateProjection::Expired,
      2,
      None,
    ))
  }
}

#[async_trait]
impl CommandHandler<PlaceBuildResultHoldCommand> for RetentionApplication {
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: PlaceBuildResultHoldCommand,
  ) -> Result<BuildResultRetentionCommandOutcome, Self::Error> {
    let mut count = self.placement_count.lock().unwrap();
    let disposition = if *count == 0 {
      MutationDisposition::Applied
    } else {
      MutationDisposition::Replayed
    };
    *count += 1;
    Ok(BuildResultRetentionCommandOutcome {
      disposition,
      retention: retention_projection!(
        command.build_id,
        command.placed_at,
        BuildResultHoldStateProjection::Active,
        1,
        None
      ),
    })
  }
}

#[async_trait]
impl CommandHandler<ReleaseBuildResultHoldCommand> for RetentionApplication {
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: ReleaseBuildResultHoldCommand,
  ) -> Result<BuildResultRetentionCommandOutcome, Self::Error> {
    if command.expected_version.get() != 1 {
      return Err(ApplicationError::PreconditionFailed);
    }
    Ok(BuildResultRetentionCommandOutcome {
      disposition: MutationDisposition::Applied,
      retention: retention_projection!(
        command.build_id,
        command.released_at,
        BuildResultHoldStateProjection::Released,
        2,
        Some(RetentionAuditIdentityProjection {
          actor_kind: "unauthenticated_management".to_owned(),
          actor_identity: None,
          request_identity: "release-request".to_owned(),
        }),
      ),
    })
  }
}

#[tokio::test]
async fn retention_routes_serialize_success_replay_expiry_and_release_audit() {
  let application = Arc::new(RecordingApplication::default());
  let retention = Arc::new(RetentionApplication::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application_with_retention(
      Arc::clone(&application),
      Arc::clone(&application),
      Arc::clone(&retention),
    ),
  );
  let build_id = "99999999-9999-4999-8999-999999999999";

  let response = routes
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!("/api/v1/builds/{build_id}/retention"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = json_body(response).await;
  assert_eq!(body["hold"]["state"], "expired");

  for (key, expected_disposition) in [("hold", "applied"), ("hold", "replayed")] {
    let response = routes
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("/api/v1/builds/{build_id}/retention/hold"),
        key,
        None,
        r#"{"reason":"incident investigation"}"#,
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(json_body(response).await["disposition"], expected_disposition);
  }

  let response = routes
    .clone()
    .oneshot(empty_request(
      "POST",
      &format!("/api/v1/builds/{build_id}/retention/hold/release"),
      Some(("release", "\"1\"")),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = json_body(response).await;
  assert_eq!(body["retention"]["hold"]["state"], "released");
  assert_eq!(
    body["retention"]["hold"]["release_audit"]["request_identity"],
    "release-request"
  );

  let response = routes
    .oneshot(empty_request(
      "POST",
      &format!("/api/v1/builds/{build_id}/retention/hold/release"),
      Some(("stale-release", "\"99\"")),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);
  assert_eq!(json_body(response).await["code"], "precondition_failed");
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
  serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}
