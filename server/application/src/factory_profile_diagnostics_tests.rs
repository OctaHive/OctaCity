//! Safe profile history through the same authorized paged discovery boundary.
use crate::*;
use std::sync::Arc;

#[test]
fn profile_diagnostics_preserve_pending_and_selected_history_without_private_content() {
  for (parameter, value, display) in [
    (Some("deliberation"), serde_json::json!("deep"), Some("deep")),
    (Some("analysis_budget"), serde_json::json!(2048), Some("2048")),
    (Some("effort_ratio"), serde_json::json!(0.5), Some("0.5")),
    (Some("thinking_enabled"), serde_json::json!(false), Some("false")),
    (None, serde_json::json!("medium"), None),
  ] {
    crate::factory_admission_tests::run_ready(async {
      let fixture = crate::factory_test_support::pending_factory_profile_fixture_with_settings(|definition| {
        definition.thinking_parameter = parameter.map(|parameter| parameter.parse().unwrap());
        definition.model_parameters = [
          (parameter.unwrap_or("reasoning_effort").parse().unwrap(), value.clone()),
          (
            "private_option".parse().unwrap(),
            serde_json::json!("private-runtime-value"),
          ),
        ]
        .into();
      })
      .await;
      let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let queries = AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(
          FactoryDiscoveryHandlers::with_policy(fixture.store.clone(), policy.clone())
            .with_profile_diagnostics(fixture.store.clone()),
        ),
      );
      let collection: FactoryDiagnosticCollection = serde_json::from_str("\"profile-selections\"").unwrap();
      let read =
        || ListFactoryDiagnosticsQuery::try_new(fixture.run_id.parse().unwrap(), collection, None, 100).unwrap();
      let page = queries.handle_query(&context, read()).await.unwrap();
      let pending = serde_json::to_value(page).unwrap();
      let record = &pending["items"][0]["record"];
      assert_eq!(record["method"], "manual");
      assert_eq!(record["input_digest"], fixture.input_digest);
      assert_eq!(record["generation"], 0);
      assert_eq!(record["superseded"], false);
      assert_eq!(record["eligible_candidates"].as_array().unwrap().len(), 3);
      assert!(record["deadline"].as_i64().unwrap() > record["started_at"].as_i64().unwrap());
      let controls = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(FactoryOperatorHandlers::new(fixture.store.clone(), policy)),
      );
      controls
        .handle_command(
          &context,
          SelectFactoryProfileCommand {
            run_id: fixture.run_id.parse().unwrap(),
            input_digest: fixture.input_digest.parse().unwrap(),
            profile: serde_json::from_value(fixture.profiles[0].clone()).unwrap(),
            expected_version: octacity_server_factory::FactoryRunVersion::new(fixture.version).unwrap(),
            idempotency_key: "diagnostic-choice".parse().unwrap(),
            requested_at: SystemFactoryExecutionClock.now().unwrap(),
          },
        )
        .await
        .unwrap();
      let accepted = serde_json::to_value(queries.handle_query(&context, read()).await.unwrap()).unwrap();
      let control = &accepted["items"][0]["record"]["pending_control"];
      assert_eq!(control["profile"], fixture.profiles[0]);
      assert_eq!(control["version"], fixture.version + 1);
      assert!(!control["id"].is_null());
      assert!(
        accepted["items"][0]["record"]["profile"].is_null(),
        "accepted intent has not been consumed"
      );
      fixture.commit_build_intent().await;
      let selected = serde_json::to_value(queries.handle_query(&context, read()).await.unwrap()).unwrap();
      let records = selected["items"].as_array().unwrap();
      assert_eq!(records.len(), 2);
      let old = records.iter().find(|r| r["record"]["profile"].is_null()).unwrap();
      assert_eq!(old["record"]["superseded"], true);
      let current = records
        .iter()
        .find(|r| r["record"]["profile"] == fixture.profiles[0])
        .unwrap();
      assert_eq!(current["record"]["generation"], 1);
      assert_eq!(current["record"]["thinking_level"], serde_json::json!(display));
      assert!(!selected.to_string().contains("private-runtime-value"));
      assert!(!selected.to_string().contains("private_option"));
      assert!(!current["record"]["control_id"].is_null());
      assert!(!current["record"]["predecessor"].is_null());
      assert_eq!(current["record"]["eligible_candidates"], serde_json::json!([]));
      assert_eq!(current["record"]["builds"].as_array().unwrap().len(), 1);
      assert!(!current["record"]["builds"][0]["operation_id"].is_null());
      fixture
        .store
        .disable_project(fixture.profiles[0]["project_id"].as_str().unwrap().parse().unwrap())
        .unwrap();
      let retained = serde_json::to_value(queries.handle_query(&context, read()).await.unwrap()).unwrap();
      assert!(retained["items"].as_array().unwrap().iter().any(|row| {
        row["record"]["profile"] == fixture.profiles[0] && row["record"]["thinking_level"] == serde_json::json!(display)
      }));
      for forbidden in [
        "instructions",
        "environment",
        "cli_arguments",
        "secret_variables",
        "payload",
        "ownership",
        "request_id",
        "files",
        "model_parameters",
      ] {
        assert!(
          !selected.to_string().contains(&format!("\"{forbidden}\"")),
          "leaked {forbidden}"
        );
      }
    });
  }
}

/// Reuses real owner/provider fixtures through the public authorization and paging boundary.
pub(crate) async fn safe_history(
  store: Arc<octacity_server_store::testing::InMemoryFactoryConfigurationStore>,
  run: octacity_server_factory::FactoryRunId,
) -> serde_json::Value {
  let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let queries = AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(FactoryDiscoveryHandlers::with_policy(store.clone(), policy).with_profile_diagnostics(store)),
  );
  let collection = serde_json::from_str("\"profile-selections\"").unwrap();
  let page = queries
    .handle_query(
      &context,
      ListFactoryDiagnosticsQuery::try_new(run, collection, None, 100).unwrap(),
    )
    .await
    .unwrap();
  let value = serde_json::to_value(page).unwrap();
  for key in [
    "input",
    "instructions",
    "answers",
    "questions",
    "skills",
    "request",
    "environment",
    "ownership",
    "payload",
  ] {
    assert!(!value.to_string().contains(&format!("\"{key}\"")), "leaked {key}");
  }
  value
}
