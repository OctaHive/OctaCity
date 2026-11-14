//! Mutation controls for the qualifier's public HTTP observation after restart.
use super::*;

#[derive(Clone, Copy)]
enum CurrentSelection {
  Exact,
  MissingComment,
  AdditionalComment,
}

async fn observe(selection: CurrentSelection) -> Result<(), tokio::task::JoinError> {
  let project = Uuid::from_u128(1);
  let issue = Uuid::from_u128(2);
  let execution = Uuid::from_u128(3).to_string();
  let run = json!({"id":Uuid::from_u128(4)});
  let digest = FactoryDigest::from_bytes([5; 32]);
  let actor = json!({"kind":"unauthenticated_management","identity":null});
  let artifact = |id| json!({"artifact_id":Uuid::from_u128(id),"content_digest":digest,"encoded_size":22});
  let references = [
    json!({"kind":"issue_document","project_id":project,"issue_id":issue,"artifact":artifact(6)}),
    json!({"kind":"project_document","project_id":project,"artifact":artifact(7)}),
    json!({"kind":"comment","project_id":project,"issue_id":issue,"comment_id":Uuid::from_u128(8),"body_digest":digest}),
  ];
  let selections: Vec<octacity_server_store::IssueMaterialSelection> = references
    .into_iter()
    .map(|reference| {
      serde_json::from_value(json!({"reference":reference,"selected_by":actor,"selected_at":1})).unwrap()
    })
    .collect();
  let mut current = json!(selections);
  match selection {
    CurrentSelection::Exact => {}
    CurrentSelection::MissingComment => {
      current.as_array_mut().unwrap().pop();
    }
    CurrentSelection::AdditionalComment => {
      let mut extra = current[2].clone();
      extra["reference"]["comment_id"] = json!(Uuid::from_u128(9));
      current.as_array_mut().unwrap().push(extra);
    }
  }
  let mut attribution = serde_json::Map::new();
  for (alias, field) in [
    ("priority", json!({"kind":"priority","definition_version":1})),
    ("component", json!({"kind":"component","definition_version":1})),
    ("kind", json!({"kind":"type","definition_version":1})),
    ("size", json!({"kind":"size"})),
    (
      "severity",
      json!({"kind":"property","id":"severity","definition_version":1}),
    ),
  ] {
    let origin: octacity_server_store::IssueClassificationAttribution = serde_json::from_value(json!({
      "field":field,"origin":{"kind":"manual","actor":actor},
      "assessed_version":1,"requirements_version":1,"assessed_at":1
    }))
    .unwrap();
    attribution.insert(alias.into(), json!(origin));
  }
  let snapshot = json!({"version":6,"target":{},"attribution":attribution,
    "values":{"priority":"high","component":"api","kind":"feature_request","size":"small",
      "environment":"staging-arm64","severity":"significant",
      "context":["ISSUE_TEXT_SELECTED_v1","PROJECT_TEXT_SELECTED_v1","COMMENT_TEXT_SELECTED_v1"]},
    "materials":{"context":selections.iter().map(|entry| json!({"selection":entry})).collect::<Vec<_>>()}
  });
  let issue_path = format!("/api/v1/projects/{project}/issues/{issue}");
  let path = format!("{issue_path}/executions");
  let responses = std::sync::Arc::new(std::collections::BTreeMap::from([
    (issue_path, json!({"materials":current})),
    (
      format!("{path}/{execution}"),
      json!({"association":{"run_id":run["id"]},"snapshot_digest":digest}),
    ),
    (
      format!("{path}/{execution}/snapshot"),
      json!({"availability":"available","snapshot":snapshot}),
    ),
  ]));
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
    verify_retained(
      &Client::new(),
      &origin,
      &path,
      &workflow_issue_snapshot::Retained {
        execution,
        reviewed: json!({"snapshot":snapshot,"snapshot_digest":digest}),
      },
      &run,
    )
    .await;
  })
  .await;
  server.abort();
  assert!(server.await.unwrap_err().is_cancelled());
  result
}

#[tokio::test]
async fn material_qualifier_accepts_the_complete_current_selection() {
  assert!(observe(CurrentSelection::Exact).await.is_ok());
}

#[test]
fn ordinary_http_observation_does_not_create_native_evidence() {
  let directory = tempfile::tempdir().unwrap();
  // Scope the release environment to the child, without mutating this parallel test process.
  let output = std::process::Command::new(env::current_exe().unwrap())
    .args([
      "workflow_issue_materials::tests::material_qualifier_accepts_the_complete_current_selection",
      "--exact",
    ])
    .env("OCTACITY_RELEASE_EVIDENCE_DIR", directory.path())
    .output()
    .unwrap();
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(
    output.status.success(),
    "HTTP observation failed: {stdout}{}",
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(
    stdout.contains("test result: ok. 1 passed; 0 failed;"),
    "the child must execute the HTTP observation: {stdout}"
  );
  assert!(
    fs::read_dir(directory.path()).unwrap().next().is_none(),
    "ordinary HTTP observation must not create a qualification proof, even in the release environment"
  );
}

#[tokio::test]
async fn material_qualifier_rejects_a_missing_selected_comment_after_restart() {
  assert!(
    observe(CurrentSelection::MissingComment)
      .await
      .is_err_and(|error| error.is_panic()),
    "a matching prefix must not hide the missing comment"
  );
}

#[tokio::test]
async fn material_qualifier_rejects_an_additional_current_material_after_restart() {
  assert!(
    observe(CurrentSelection::AdditionalComment)
      .await
      .is_err_and(|error| error.is_panic()),
    "the qualifier must inspect the complete current selection"
  );
}
