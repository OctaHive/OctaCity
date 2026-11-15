//! Selected forms publish through normal HTTP owners and retain exact receipts after restart.
use super::*;
use octacity_server_factory::*;
use octacity_server_store::{FactoryCapabilitySource, FactoryConfigurationAvailability, StoreError};

struct FieldsInstallation;
fn source() -> octacity_server_store::testing::FactoryNodeJournalContractFixture {
  octacity_server_store::testing::factory_issue_field_journal_fixture()
}
fn selected_build(project: octacity_server_domain::ProjectId) -> FlowBuildBinding {
  let source = source();
  let mut build = source.admission.flow.closure().definitions()[0].nodes()[0]
    .build()
    .unwrap()
    .clone();
  let old = &build.build_configuration;
  build.build_configuration = BuildConfigurationRef::new(
    "00000000-0000-4000-8000-000000000600".parse().unwrap(),
    old.version(),
    project,
    old.definition_digest(),
  );
  build
}
#[async_trait::async_trait]
impl FactoryCapabilitySource for FieldsInstallation {
  async fn availability(
    &self,
    project: octacity_server_domain::ProjectId,
  ) -> Result<FactoryConfigurationAvailability, StoreError> {
    let source = source();
    let build = selected_build(project);
    let writer = source.admission.flow.closure().definitions()[0].nodes()[0].outcomes()[0]
      .issue_fields()
      .unwrap()
      .writer
      .clone();
    let pins = [
      &build.tool,
      build.result_tool(),
      &build.plugin,
      &build.model_or_tool,
      &writer,
    ]
    .into_iter()
    .chain(
      build
        .evidence
        .iter()
        .flat_map(|output| [output.tool(), output.plugin()]),
    );
    let references = pins
      .enumerate()
      .map(|(n, reference)| FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: format!("choice.{n}").parse().unwrap(),
        reference: reference.clone(),
      })
      .collect();
    let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
      references,
      build_configurations: vec![("analysis".parse().unwrap(), build.build_configuration)],
    })
    .unwrap()
    .with_choice_metadata(
      FactoryChoiceKind::NodeCapability,
      "choice.4".parse().unwrap(),
      FactoryChoiceMetadata {
        node_kinds: vec![FlowNodeKind::TrustedAction],
        issue_classification: Some(FactoryIssueClassificationMapping {
          policy: "issue-fields".parse().unwrap(),
          size_labels: Default::default(),
        }),
        ..Default::default()
      },
    )
    .unwrap();
    Ok(FactoryConfigurationAvailability::Available(choices))
  }
}
#[async_trait::async_trait]
impl octacity_server::FactoryRuntimeInstaller for FieldsInstallation {
  async fn install(
    &self,
    resources: octacity_server::FactoryRuntimeResources,
  ) -> Result<octacity_server::FactoryRuntimeInstallation, octacity_server::FactoryRuntimeInstallationError> {
    let mut installed =
      octacity_server::FactoryRuntimeInstaller::install(&installed_fixture::Installation, resources).await?;
    installed.capabilities = Arc::new(FieldsInstallation);
    Ok(installed)
  }
}
#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn selected_issue_fields_http_publication_preserves_the_form_and_replays_after_restart() {
  let f = Fixture::with_installer(Some(Arc::new(FieldsInstallation))).await;
  let base = format!("http://{}/api/v1", f.runtime.management_addr());
  let selection =
    json!({"project_id":f.project,"definition_version":0,"fields":[{"field":{"kind":"priority"},"required":true}]});
  let response = f
    .client
    .post(format!("{base}/flow-data-schemas/validate"))
    .json(&json!({"project_id":f.project,"identity":"selected.priority","version":"1","issue_fields":selection}))
    .send()
    .await
    .unwrap();
  let status = response.status();
  let schema: Value = response.json().await.unwrap();
  assert_eq!(status, StatusCode::OK, "{schema}");
  let source = source();
  let flow = &source.admission.flow;
  let mut root = serde_json::to_value(flow.closure().definitions()[0].clone().without_code().unwrap()).unwrap();
  root.as_object_mut().unwrap().remove("reference");
  root["nodes"][0]["build"] = serde_json::to_value(selected_build(f.project.parse().unwrap())).unwrap();
  root["nodes"][0]["outcomes"][0]["schema"] = schema["reference"].clone();
  root["nodes"][0]["outcomes"][0]["issue_fields"]["form"] = selection.clone();
  let mut schemas = serde_json::to_value(flow.data_schemas()).unwrap();
  for current in schemas.as_array_mut().unwrap() {
    if current["reference"]["identity"] == "selected.priority" {
      *current = schema.clone();
    }
  }
  let id = uuid::Uuid::new_v4().to_string();
  let request = json!({"id":id,"project_id":f.project,"definition":{"root":root,"limits":flow.limits(),"data_schemas":schemas,"pools":{}}});
  let response = f
    .client
    .post(format!("{base}/flow-definitions"))
    .header("idempotency-key", "selected-form")
    .json(&request)
    .send()
    .await
    .unwrap();
  let status = response.status();
  let receipt: Value = response.json().await.unwrap();
  assert_eq!(status, StatusCode::CREATED, "{receipt}");
  openapi_contract::assert_json_matches_component(
    &octacity_server_api_rest::v1::openapi_document(),
    "CreateFactoryFlowDefinitionRequest",
    &request,
  );
  assert_eq!(
    receipt["resource"]["flow"]["closure"]["definitions"][0]["nodes"][0]["outcomes"][0]["issue_fields"]["form"],
    selection
  );
  // A lost publication acknowledgement still replays its original form after
  // the Project publishes an unrelated field in a newer catalog.
  let updated=f.client.post(format!("{base}/projects/{}/issue-property-definitions",f.project)).header("idempotency-key","later-catalog")
    .json(&json!({"expected_version":0,"definition":{"labels":[],"properties":[{"id":"note","name":"Note","retired":false,"kind":{"type":"boolean"}}]}})).send().await.unwrap();
  assert_eq!(updated.status(), StatusCode::OK);
  let f = f.restart_with_installer(Some(Arc::new(FieldsInstallation))).await;
  let response = f
    .client
    .post(format!(
      "http://{}/api/v1/flow-definitions",
      f.runtime.management_addr()
    ))
    .header("idempotency-key", "selected-form")
    .json(&request)
    .send()
    .await
    .unwrap();
  assert!(response.status().is_success());
  let replay: Value = response.json().await.unwrap();
  assert_eq!(replay["resource"], receipt["resource"]);
  f.stop().await;
}
