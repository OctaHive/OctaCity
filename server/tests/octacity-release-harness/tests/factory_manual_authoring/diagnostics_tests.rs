//! Failure collection through the same HTTP and private-file boundaries as the release journey.
use super::*;
use axum::{Router, extract::Request, response::IntoResponse};
use std::os::unix::fs::PermissionsExt as _;

const BUILD: &str = "10000000-0000-4000-8000-000000000001";
const ATTEMPT: &str = "20000000-0000-4000-8000-000000000001";
const JOB: &str = "30000000-0000-4000-8000-000000000001";
const RESULT: &str = r#"{"status":"failed","error":"private-model-failure"}"#;
const TRACE: &str = "{\"type\":\"stderr\",\"text\":\"private-model-stderr\"}\n";

async fn fixture(unavailable: Option<&'static str>) -> (String, tokio::task::JoinHandle<()>) {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let origin = format!("http://{}", listener.local_addr().unwrap());
  let server_origin = origin.clone();
  let router = Router::new().fallback(move |request: Request| {
    let origin = server_origin.clone();
    async move {
      let path = request.uri().path();
      let value = if path == "/api/v1/projects/project/builds" {
        json!({"items":[{"id":BUILD,"state":"failed","current_attempt_id":ATTEMPT}]})
      } else if path == format!("/api/v1/attempts/{ATTEMPT}") {
        if unavailable == Some("attempt") { return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response(); }
        json!({"attempt":{"id":ATTEMPT},"jobs":[{"id":JOB,"state":"failed"}],"edges":[]})
      } else if path == format!("/api/v1/jobs/{JOB}/events") {
        if unavailable == Some("events") { return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response(); }
        if request.uri().query().unwrap_or_default().contains("after=0&") {
          json!({"items":(1..=100).map(|sequence| json!({"sequence":sequence,"kind":"output","payload":{"text":"private-job-output"}})).collect::<Vec<_>>(),"cursor":100})
        } else {
          json!({"items":[{"sequence":101,"kind":"failed","payload":{"reason":"model exited 64"}}],"cursor":101})
        }
      } else if path == format!("/api/v1/builds/{BUILD}/artifacts") {
        json!({"items":[{"id":"result","name":"codex-run-result"},{"id":"trace","name":"codex-run-trace"}]})
      } else if path == "/api/v1/artifacts/result/download" {
        json!({"get_url":format!("{origin}/objects/result?signature=private-download-signature")})
      } else if path == "/api/v1/artifacts/trace/download" {
        json!({"get_url":format!("{origin}/objects/trace?signature=private-download-signature")})
      } else if path == "/objects/result" {
        if unavailable == Some("result") { return axum::http::StatusCode::FORBIDDEN.into_response(); }
        if unavailable == Some("oversized_result") { return "x".repeat(4 * 1024 * 1024 + 1).into_response(); }
        return RESULT.into_response();
      } else if path == "/objects/trace" {
        return TRACE.into_response();
      } else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
      };
      axum::Json(value).into_response()
    }
  });
  let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
  (origin, server)
}

#[tokio::test]
async fn failed_build_diagnostics_are_private_and_complete_before_the_original_assert() {
  let (origin, server) = fixture(None).await;
  let evidence = tempfile::tempdir().unwrap();
  let directory = evidence.path().to_path_buf();
  let failure = tokio::spawn(async move {
    diagnostics::assert_no_failed_builds(&Client::new(), &origin, "project", &directory).await;
  })
  .await
  .unwrap_err();
  server.abort();
  assert!(failure.is_panic());
  let panic = failure.into_panic();
  let message = panic.downcast_ref::<String>().unwrap();
  assert!(message.contains("ordinary Build failed"));
  assert!(!message.contains("private-model") && !message.contains("private-download"));
  let retained = evidence.path().join(format!("failed-build-{BUILD}"));
  assert_eq!(fs::metadata(&retained).unwrap().permissions().mode() & 0o777, 0o700);
  assert_eq!(
    fs::read(retained.join("codex-run-result.json")).unwrap(),
    RESULT.as_bytes()
  );
  assert_eq!(
    fs::read(retained.join("codex-run-trace.ndjson")).unwrap(),
    TRACE.as_bytes()
  );
  let events: Value =
    serde_json::from_slice(&fs::read(retained.join(format!("job-{JOB}-events.json"))).unwrap()).unwrap();
  assert_eq!(events["items"].as_array().unwrap().len(), 101);
  assert_eq!(events["items"][100]["payload"]["reason"], "model exited 64");
  for entry in fs::read_dir(retained).unwrap() {
    let entry = entry.unwrap();
    assert_eq!(entry.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    assert!(!String::from_utf8_lossy(&fs::read(entry.path()).unwrap()).contains("private-download-signature"));
  }
}

#[tokio::test]
async fn failed_build_diagnostics_keep_available_sources_when_another_source_fails() {
  for unavailable in ["result", "events", "attempt", "oversized_result"] {
    let (origin, server) = fixture(Some(unavailable)).await;
    let evidence = tempfile::tempdir().unwrap();
    let directory = evidence.path().to_path_buf();
    let failure = tokio::spawn(async move {
      diagnostics::assert_no_failed_builds(&Client::new(), &origin, "project", &directory).await;
    })
    .await
    .unwrap_err();
    server.abort();
    assert!(failure.is_panic());
    assert!(
      failure
        .into_panic()
        .downcast_ref::<String>()
        .unwrap()
        .contains("ordinary Build failed")
    );
    let retained = evidence.path().join(format!("failed-build-{BUILD}"));
    assert_eq!(
      fs::read(retained.join("codex-run-trace.ndjson")).unwrap(),
      TRACE.as_bytes(),
      "{unavailable}"
    );
    if unavailable != "result" && unavailable != "oversized_result" {
      assert_eq!(
        fs::read(retained.join("codex-run-result.json")).unwrap(),
        RESULT.as_bytes()
      );
    } else {
      assert!(!retained.join("codex-run-result.json").exists());
      assert!(retained.join(format!("job-{JOB}-events.json")).is_file());
    }
    let errors = fs::read_to_string(retained.join("collection-error.json")).unwrap();
    assert!(!errors.contains("private-download-signature"));
    assert!(errors.contains(if unavailable == "oversized_result" {
      "diagnostic_response_limit"
    } else {
      "diagnostic_http_failed"
    }));
  }
}
