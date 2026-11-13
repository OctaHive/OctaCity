//! Pure installed admission data for real Issue/Factory HTTP conformance.
use crate::*;
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_store::testing as store_testing;
use octacity_server_store::*;
use std::{collections::BTreeSet, sync::Arc};
mod admission {
  use super::*;
  fn time(value: i64) -> Timestamp {
    Timestamp::from_unix_millis(value).unwrap()
  }
  include!("../factory/admission/tests/factory_admission_test_fixture.rs");
}

/// Reuses the admission fixtures and caller-declared public schema/input grammar.
/// No execution is started or producer result manufactured by this fixture.
pub fn issue_execution_factory(
  project: ProjectId,
  input: serde_json::Value,
  shape: serde_json::Value,
) -> (
  Arc<store_testing::InMemoryFactoryConfigurationStore>,
  IssueExecutionTarget,
) {
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  store.seed_project(project).unwrap();
  let repository = admission::repository(
    project,
    RepositoryId::generate(),
    Some(SourceReference::new("refs/heads/main").unwrap()),
  );
  let id = FactoryConfigurationId::generate();
  let ordinary = admission::configuration(project, id);
  let configuration = if input.is_null() {
    ordinary
  } else {
    let original = ordinary.flow();
    let schema = FlowDataSchema::new(
      "http.issue".parse().unwrap(),
      "v1".parse().unwrap(),
      serde_json::from_value(shape).unwrap(),
    )
    .unwrap();
    let root = original.closure.definition(original.closure.root()).unwrap();
    let mut nodes = root.nodes().to_vec();
    let node = nodes.iter_mut().find(|n| n.key() == root.entry()).unwrap();
    *node = node
      .clone()
      .with_input_binding(
        FlowInputBinding::new(
          [(
            "issue".parse().unwrap(),
            FlowInputSource::Incoming {
              schema: schema.reference().clone(),
              path: String::new(),
            },
          )]
          .into(),
          FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::json!(true),
          })
          .unwrap(),
        )
        .unwrap(),
      )
      .unwrap();
    let definition = FlowDefinition::new(FlowDefinitionInput {
      id: root.reference().id(),
      version: root.reference().version(),
      input_schema: root.input_schema().cloned(),
      entry: root.entry().clone(),
      nodes,
      transitions: root.transitions().to_vec(),
      terminals: root.terminals().to_vec(),
      context_projections: root.context_projections().to_vec(),
      data_projections: root.data_projections().to_vec(),
      execution: root.execution().clone(),
    })
    .unwrap();
    let mut schemas = original.data_schemas.clone();
    schemas.push(schema.clone());
    let mut input = input;
    input["schema"] = serde_json::to_value(schema.reference()).unwrap();
    let flow = FactoryFlowConfiguration::new(
      PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
      original.limits.clone(),
      schemas,
      original.pools.clone(),
    )
    .unwrap()
    .with_issue_snapshot(serde_json::from_value(input).unwrap())
    .unwrap();
    admission::configuration_with_flow(project, id, Some(flow))
  };
  let reference = configuration.reference().clone();
  let policies = store_testing::factory_project_policies_with_access(
    project,
    true,
    true,
    configuration.flow().limits.permissions().clone(),
  );
  store
    .seed_factory_configuration_version(PublishedFactoryConfiguration {
      configuration,
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    })
    .unwrap();
  store.seed_factory_configuration_policies(&reference, policies).unwrap();
  let target = serde_json::from_value(serde_json::json!({"configuration_id":id,"configuration_version":1,
    "repository_id":repository.id,"repository_version":1,"source":{"kind":"exact_revision","value":"original-base"},
    "mapping":"title_description_acceptance_v1"}))
  .unwrap();
  store.seed_repository_version(repository).unwrap();
  (store, target)
}
