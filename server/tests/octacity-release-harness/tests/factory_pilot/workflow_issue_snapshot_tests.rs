//! The qualifier consumes public HTTP metadata; reviewed and retained snapshots may share bad provenance.
use super::*;

#[derive(Clone, Copy)]
enum Attribution {
  Exact,
  ReceiptRecord,
  DifferentWork,
}

async fn verify(attribution: Attribution) -> Result<(), tokio::task::JoinError> {
  let triage = Uuid::from_u128(1).to_string();
  let source_work = Uuid::from_u128(2).to_string();
  let delivery = Uuid::from_u128(3).to_string();
  let execution = Uuid::from_u128(4).to_string();
  let assessment = FactoryDigest::from_bytes([11; 32]).to_string();
  let receipt = FactoryDigest::from_bytes([12; 32]).to_string();
  let form = FactoryDigest::from_bytes([14; 32]).to_string();
  let actor = json!({"kind":"unauthenticated_management","identity":null});
  let mut origins = serde_json::Map::new();
  for (alias, field) in [
    ("priority", json!({"kind":"priority","definition_version":1})),
    ("component", json!({"kind":"component","definition_version":1})),
    ("kind", json!({"kind":"type","definition_version":1})),
    ("size", json!({"kind":"size"})),
  ] {
    let wrong_record = alias == "size" && matches!(attribution, Attribution::ReceiptRecord);
    let wrong_work = alias == "size" && matches!(attribution, Attribution::DifferentWork);
    let field: octacity_server_store::IssueClassificationAttribution = serde_json::from_value(json!({
      "field":field,"origin":{"kind":"pipeline","actor":actor,
        "run_id":triage,"work_id":if wrong_work {&delivery} else {&source_work},
        "record":if wrong_record {&receipt} else {&assessment},"form":form,
        "policy":"triage","operation_id":receipt},
      "assessed_version":1,"requirements_version":1,"assessed_at":1,"evidence":[]
    }))
    .unwrap();
    origins.insert(alias.into(), json!(field));
  }
  let snapshot = json!({"version":2,"definition_head":1,
    "values":{"priority":"high","component":"api","kind":"feature_request","size":"small"},
    "target":{},"attribution":origins});
  let digest = FactoryDigest::from_bytes([13; 32]).to_string();
  let path = "/api/v1/projects/project/issues/issue/executions";
  let mut responses = std::collections::BTreeMap::new();
  responses.insert(
    format!("{path}/{execution}"),
    json!({"association":{"run_id":delivery},"snapshot_digest":digest}),
  );
  responses.insert(
    format!("{path}/{execution}/snapshot"),
    json!({"availability":"available","snapshot":snapshot}),
  );
  responses.insert(
    format!("/api/v1/factory-runs/{triage}"),
    json!({"id":triage,"work_id":source_work}),
  );
  responses.insert(
    format!("/api/v1/factory-runs/{triage}/node-completions"),
    json!({"items":[
      {"record":{"node_key":"assess","outcome":"eligible","output_digest":assessment}},
      {"record":{"node_key":"apply-classification","outcome":"applied","output_digest":receipt}}
    ]}),
  );
  let responses = std::sync::Arc::new(responses);
  let router = axum::Router::new().fallback(move |uri: axum::http::Uri| {
    let responses = responses.clone();
    async move {
      use axum::response::IntoResponse;
      match responses.get(uri.path()) {
        Some(value) => axum::Json(value.clone()).into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
      }
    }
  });
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let origin = format!("http://{}", listener.local_addr().unwrap());
  let server = tokio::spawn(async {
    axum::serve(listener, router).await.unwrap();
  });
  let result = tokio::spawn(async move {
    let retained = Retained {
      execution: execution.clone(),
      reviewed: json!({"snapshot":snapshot,"snapshot_digest":digest}),
    };
    let run = json!({"id":delivery,"issue_association":{"request_id":execution}});
    retained
      .verify(&Client::new(), &origin, path, Some(&triage), &run)
      .await;
  })
  .await;
  server.abort();
  assert!(server.await.unwrap_err().is_cancelled());
  result
}

#[tokio::test]
async fn retained_snapshot_accepts_the_exact_assessment_record_and_work() {
  assert!(verify(Attribution::Exact).await.is_ok());
}

#[tokio::test]
async fn retained_snapshot_rejects_the_receipt_record_from_the_same_triage_run() {
  assert!(
    verify(Attribution::ReceiptRecord)
      .await
      .is_err_and(|error| error.is_panic()),
    "the qualifier must reject another valid record digest from the same Run"
  );
}

#[tokio::test]
async fn retained_snapshot_rejects_a_different_work_for_the_same_triage_run() {
  assert!(
    verify(Attribution::DifferentWork)
      .await
      .is_err_and(|error| error.is_panic()),
    "the qualifier must reject a valid but unrelated Work ID"
  );
}
